//! 增量更新调度器集成测试（全程在临时目录 + 本地 HTTP 服务，**不触碰真实安装**）。
//!
//! 覆盖 `docs/Sereno 增量更新接入与使用指南.md` 的完整生命周期：
//! 补丁选择 → 下载 → 双哈希校验合并 → 替换脚本生成，
//! 以及四条后路：无补丁回退全量、坏补丁拒绝并记失败、安全模式、验签 fail-closed。

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

use serde_json::json;
use sereno_core::updater::{prepare_delta_update, verify_manifest_signature};
use tauri_delta_updater::{UpdateManifest, calculate_hash, create_patch};

/// 极简 HTTP 服务：按路径返回预置内容，单线程、每个连接一个响应。
struct TestServer {
    port: u16,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    /// `routes_by_port` 接收绑定后的端口（路由里的 URL 可能要用它拼绝对地址）。
    fn start<F>(routes_by_port: F) -> Self
    where
        F: FnOnce(u16) -> Vec<(String, Vec<u8>)>,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定测试端口");
        let port = listener.local_addr().unwrap().port();
        let routes = routes_by_port(port);

        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let request = String::from_utf8_lossy(&buf);
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();

                let body = routes
                    .iter()
                    .find(|(route, _)| *route == path)
                    .map(|(_, body)| body.clone())
                    .unwrap_or_else(|| b"not found".to_vec());

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });

        Self {
            port,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{port}{path}", port = self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // drop listener 会停止 accept；线程随进程退出
        drop(self.handle.take());
    }
}

fn craft_manifest(version: &str, patch_url: &str, patch_hash: &str, target_hash: &str) -> Vec<u8> {
    let mut manifest = json!({
        "version": version,
        "notes": "test",
        "pub_date": "2026-11-05T00:00:00Z",
        "platforms": {},
        "patches": []
    });
    if !patch_url.is_empty() {
        manifest["patches"] = json!([{
            "from_version": "0.1.0",
            "to_version": version,
            "patch_url": patch_url,
            "patch_hash": patch_hash,
            "target_hash": target_hash,
        }]);
    }
    serde_json::to_vec(&manifest).unwrap()
}

/// 用引擎同款 create_patch 生成补丁（哈希走引擎的 calculate_hash）。
async fn make_patch(old: &[u8], new: &[u8], dir: &Path) -> (Vec<u8>, String, String) {
    let old_path = dir.join("_old.bin");
    let new_path = dir.join("_new.bin");
    let patch_path = dir.join("_patch.bin");
    std::fs::write(&old_path, old).unwrap();
    std::fs::write(&new_path, new).unwrap();

    create_patch(&old_path, &new_path, &patch_path)
        .await
        .unwrap();
    let patch = std::fs::read(&patch_path).unwrap();
    let patch_hash = calculate_hash(&patch_path).await.unwrap();
    let target_hash = calculate_hash(&new_path).await.unwrap();
    (patch, patch_hash, target_hash)
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}

async fn prepare(
    exe: &Path,
    client: &reqwest::Client,
    manifest: &[u8],
) -> sereno_core::Result<Option<sereno_core::updater::StagedDelta>> {
    let manifest: UpdateManifest = serde_json::from_slice(manifest).unwrap();
    prepare_delta_update(client, exe, "0.1.0", &manifest).await
}

#[tokio::test]
async fn delta_flow_happy_path_and_fallbacks() {
    let tmp = tempfile::tempdir().unwrap();
    let exe_dir = tmp.path().to_path_buf();
    let exe = exe_dir.join("sereno-desktop.exe");

    // 旧版 exe：2MB 伪数据；新版：中部改 4KB（补丁会很小）
    let mut old_bytes = vec![7u8; 2 * 1024 * 1024];
    let mut new_bytes = old_bytes.clone();
    for byte in &mut new_bytes[1024 * 1024..1024 * 1024 + 4096] {
        *byte = 0xAB;
    }
    old_bytes[0] = 1;
    new_bytes[0] = 2;
    std::fs::write(&exe, &old_bytes).unwrap();

    let (patch, patch_hash, target_hash) = make_patch(&old_bytes, &new_bytes, tmp.path()).await;
    let client = http_client();

    // ---- 场景 1：无补丁（清单没有 patches）→ None（走全量）----
    {
        let _server = TestServer::start(|_| {
            vec![("/latest.json".into(), craft_manifest("0.1.1", "", "", ""))]
        });
        let manifest = craft_manifest("0.1.1", "", "", "");
        let result = prepare(&exe, &client, &manifest).await.unwrap();
        assert!(result.is_none(), "无补丁应返回 None 走全量");
    }

    // ---- 场景 2：正常增量 → 合并产物 == 新版字节 + bat 生成 ----
    {
        let server = TestServer::start(|port| {
            let patch_url = format!("http://127.0.0.1:{port}/patch");
            vec![
                (
                    "/latest.json".into(),
                    craft_manifest("0.1.1", &patch_url, &patch_hash, &target_hash),
                ),
                ("/patch".into(), patch.clone()),
            ]
        });
        let manifest = craft_manifest("0.1.1", &server.url("/patch"), &patch_hash, &target_hash);
        let staged = prepare(&exe, &client, &manifest)
            .await
            .expect("正常补丁不应失败")
            .expect("应产出合并结果");

        let merged = std::fs::read(&staged.staged_exe).unwrap();
        assert_eq!(merged, new_bytes, "合并产物必须与新版逐字节一致");
        assert!(staged.bat_path.is_file(), "替换脚本应已生成");

        // 同版本清单 → None（已是最新）
        let manifest = craft_manifest("0.1.0", "", "", "");
        let same = prepare(&exe, &client, &manifest).await.unwrap();
        assert!(same.is_none(), "同版本应视为无需更新");
    }

    // ---- 场景 3：坏补丁（哈希不匹配）→ Err + 失败计数 ----
    {
        let server = TestServer::start(|port| {
            let patch_url = format!("http://127.0.0.1:{port}/patch");
            vec![(
                "/latest.json".into(),
                craft_manifest("0.1.1", &patch_url, &"0".repeat(64), &target_hash),
            )]
        });
        let manifest = craft_manifest(
            "0.1.1",
            &server.url("/patch"),
            &"0".repeat(64),
            &target_hash,
        );
        let result = prepare(&exe, &client, &manifest).await;
        assert!(result.is_err(), "坏补丁必须被哈希校验拒绝");
        let failures = exe_dir.join("update_failures.txt");
        assert!(failures.is_file(), "失败应被记录进 update_failures.txt");
        let count: u32 = std::fs::read_to_string(&failures)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(count, 1);
    }

    // ---- 场景 4：连续 3 次失败 → 安全模式（直接 None 走全量）----
    for _ in 0..2 {
        let server = TestServer::start(|port| {
            let patch_url = format!("http://127.0.0.1:{port}/patch");
            vec![(
                "/latest.json".into(),
                craft_manifest("0.1.1", &patch_url, &"0".repeat(64), &target_hash),
            )]
        });
        let manifest = craft_manifest(
            "0.1.1",
            &server.url("/patch"),
            &"0".repeat(64),
            &target_hash,
        );
        let _ = prepare(&exe, &client, &manifest).await;
    }
    {
        let server = TestServer::start(|port| {
            let patch_url = format!("http://127.0.0.1:{port}/patch");
            vec![(
                "/latest.json".into(),
                craft_manifest("0.1.1", &patch_url, &patch_hash, &target_hash),
            )]
        });
        let manifest = craft_manifest("0.1.1", &server.url("/patch"), &patch_hash, &target_hash);
        // 此时失败计数已是 3；即使补丁是好的，安全模式也应拦截
        let result = prepare(&exe, &client, &manifest).await.unwrap();
        assert!(result.is_none(), "安全模式下应拒绝增量、走全量");
    }
}

/// 验签是增量路径的硬门槛：伪造签名 / 坏公钥 / 非 base64 都必须拒绝。
#[test]
fn delta_manifest_signature_fails_closed() {
    let manifest = craft_manifest("0.1.1", "", "", "");
    // 1. 垃圾签名
    assert!(
        verify_manifest_signature(&manifest, "AAAA", "BBBB").is_err(),
        "无效签名必须被拒绝"
    );
    // 2. 签名内容正确但未经过真实密钥签名（用无效公钥格式也要拒绝）
    assert!(
        verify_manifest_signature(&manifest, "AAAA", "not-base64!").is_err(),
        "无效公钥必须被拒绝"
    );
}
