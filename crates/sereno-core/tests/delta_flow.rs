//! 增量更新集成测试（临时目录 + 本地 HTTP 服务，**不触碰真实安装**）。
//!
//! 引擎仓库自带完整的端到端测试；这里守的是 **Sereno 的接入面**：
//! COS 风格的 URL 布局（`/latest.json` + `/latest.json.sig`）、
//! 分组与扁平两种清单格式都能吃、以及四条后路——
//! 无补丁回退、坏补丁拒绝并记失败、安全模式、已是最新。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, RwLock};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use sereno_core::updater::{DeltaUpdater, UpdateConfig, UpdateOutcome};
use tempfile::tempdir;

/// 一个静态文件服务器：先启动拿端口，再写入路由内容
/// （清单里的补丁地址依赖端口，必须先知道端口才能签名）。
struct MockServer {
    base_url: String,
    routes: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl MockServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let routes: Arc<RwLock<HashMap<String, Vec<u8>>>> = Arc::new(RwLock::new(HashMap::new()));
        let served = routes.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming().take(64) {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let request = String::from_utf8_lossy(&buf);
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();

                let body = served.read().unwrap().get(&path).cloned();
                let response = match body {
                    Some(body) => {
                        let head =
                            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                        [head.as_bytes().to_vec(), body].concat()
                    }
                    None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
                };
                let _ = stream.write_all(&response);
                let _ = stream.flush();
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            routes,
        }
    }

    fn set_routes(&self, files: HashMap<String, Vec<u8>>) {
        *self.routes.write().unwrap() = files;
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

/// 生成一对未加密签名密钥，返回 (私钥, 公钥 base64——与 tauri.conf.json 同格式)。
fn signing_keypair() -> (minisign::SecretKey, String) {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let pk_b64 = STANDARD.encode(kp.pk.to_box().unwrap().to_string().as_bytes());
    (kp.sk, pk_b64)
}

/// 对内容签名，返回 minisign 签名字符串（即 .sig 文件的内容）。
fn sign(sk: &minisign::SecretKey, content: &[u8]) -> String {
    minisign::sign(None, sk, std::io::Cursor::new(content), None, None)
        .unwrap()
        .to_string()
}

/// 用引擎同款 create_patch 生成补丁并计算哈希。
async fn make_patch(old: &[u8], new: &[u8], dir: &std::path::Path) -> (Vec<u8>, String, String) {
    let old_path = dir.join("_old.bin");
    let new_path = dir.join("_new.bin");
    let patch_path = dir.join("_patch.bin");
    std::fs::write(&old_path, old).unwrap();
    std::fs::write(&new_path, new).unwrap();

    sereno_core::updater::create_patch(&old_path, &new_path, &patch_path)
        .await
        .unwrap();
    let patch = std::fs::read(&patch_path).unwrap();
    let patch_hash = sereno_core::updater::calculate_hash(&patch_path)
        .await
        .unwrap();
    let target_hash = sereno_core::updater::calculate_hash(&new_path)
        .await
        .unwrap();
    (patch, patch_hash, target_hash)
}

/// 构造分组格式的清单 JSON（补丁地址用占位符，调用方替换后重签）。
fn manifest_json(
    version: &str,
    platform: &str,
    patch: Option<(&str, &str, &str)>,
) -> serde_json::Value {
    let mut manifest = serde_json::json!({
        "version": version,
        "notes": "test",
        "pub_date": "2026-11-05T00:00:00Z",
        "platforms": {},
        "patches": {}
    });
    // 动态键：json! 的裸标识符键会被当作字面量，必须用索引赋值
    manifest["platforms"][platform] =
        serde_json::json!({ "signature": "dummy", "url": "https://cdn.example.com/full.exe" });
    manifest["patches"][platform] = serde_json::json!([]);
    if let Some((from, patch_hash, target_hash)) = patch {
        manifest["patches"][platform] = serde_json::json!([{
            "from_version": from,
            "to_version": version,
            "patch_url": "PATCH_URL_PLACEHOLDER",
            "patch_hash": patch_hash,
            "target_hash": target_hash,
        }]);
    }
    manifest
}

/// 启动服务器、把占位符替换成真实补丁地址、签名并注入路由。
fn serve_signed(
    server: &MockServer,
    sk: &minisign::SecretKey,
    mut manifest: serde_json::Value,
    patch: Option<Vec<u8>>,
) {
    if patch.is_some() {
        let url = server.url("/update.patch");
        let platform = sereno_core::updater::current_platform().to_string();
        manifest["patches"][&platform][0]["patch_url"] = serde_json::json!(url);
    }
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    let sig = sign(sk, &bytes);
    let mut routes = HashMap::from([
        ("/latest.json".to_string(), bytes),
        ("/latest.json.sig".to_string(), sig.into_bytes()),
    ]);
    if let Some(patch) = patch {
        routes.insert("/update.patch".to_string(), patch);
    }
    server.set_routes(routes);
}

#[tokio::test]
async fn delta_flow_sereno_integration() {
    let dir = tempdir().unwrap();
    let work_dir = dir.path().to_path_buf();
    let old_exe = work_dir.join("sereno-desktop.exe");

    // 旧版 exe：2MB 伪数据；新版：中部改 4KB（补丁很小）
    let mut old_bytes = vec![7u8; 2 * 1024 * 1024];
    let mut new_bytes = old_bytes.clone();
    for byte in &mut new_bytes[1024 * 1024..1024 * 1024 + 4096] {
        *byte = 0xAB;
    }
    old_bytes[0] = 1;
    new_bytes[0] = 2;
    std::fs::write(&old_exe, &old_bytes).unwrap();

    let (sk, pk_b64) = signing_keypair();
    let platform = sereno_core::updater::current_platform().to_string();

    // ---- 场景 1：COS 上现有格式（0.1.1 的清单根本没有 patches 字段）→ 回退全量 ----
    {
        let legacy = serde_json::json!({
            "version": "0.2.0",
            "notes": "",
            "pub_date": "2026-11-04T16:35:22Z",
            "platforms": {}
        });
        let mut legacy = legacy;
        legacy["platforms"][&platform] =
            serde_json::json!({ "signature": "dummy", "url": "https://cdn.example.com/full.exe" });
        let bytes = serde_json::to_vec(&legacy).unwrap();
        let server = MockServer::start();
        server.set_routes(HashMap::from([
            ("/latest.json".to_string(), bytes.clone()),
            (
                "/latest.json.sig".to_string(),
                sign(&sk, &bytes).into_bytes(),
            ),
        ]));
        let updater = DeltaUpdater::new(
            UpdateConfig::new(server.url("/latest.json"), &pk_b64)
                .current_version("0.1.0")
                .executable_path(&old_exe)
                .work_dir(&work_dir),
        );
        assert!(
            matches!(
                updater.update().await.unwrap(),
                UpdateOutcome::FallbackToFullDownload { .. }
            ),
            "无 patches 字段的旧清单应回退全量而不是报错"
        );
    }

    // ---- 场景 2：正常增量 → Patched，合并产物与新版逐字节一致 ----
    {
        let (patch, patch_hash, target_hash) = make_patch(&old_bytes, &new_bytes, dir.path()).await;
        let server = MockServer::start();
        serve_signed(
            &server,
            &sk,
            manifest_json(
                "0.2.0",
                &platform,
                Some(("0.1.0", &patch_hash, &target_hash)),
            ),
            Some(patch),
        );
        let updater = DeltaUpdater::new(
            UpdateConfig::new(server.url("/latest.json"), &pk_b64)
                .current_version("0.1.0")
                .executable_path(&old_exe)
                .work_dir(&work_dir),
        );
        match updater.update().await.unwrap() {
            UpdateOutcome::Patched {
                staged_exe,
                manifest,
            } => {
                assert_eq!(manifest.version, "0.2.0");
                assert_eq!(
                    std::fs::read(&staged_exe).unwrap(),
                    new_bytes,
                    "合并产物必须与新版本逐字节一致"
                );
            }
            other => panic!("应当完成增量更新，实际得到: {other:?}"),
        }
    }

    // ---- 场景 3：坏补丁（哈希不匹配）→ Err，且失败被记录 ----
    {
        let (patch, _good_hash, target_hash) = make_patch(&old_bytes, &new_bytes, dir.path()).await;
        let bad_hash = "0".repeat(64);
        let server = MockServer::start();
        serve_signed(
            &server,
            &sk,
            manifest_json("0.2.0", &platform, Some(("0.1.0", &bad_hash, &target_hash))),
            Some(patch),
        );
        let updater = DeltaUpdater::new(
            UpdateConfig::new(server.url("/latest.json"), &pk_b64)
                .current_version("0.1.0")
                .executable_path(&old_exe)
                .work_dir(&work_dir),
        );
        assert!(updater.update().await.is_err(), "坏补丁必须被哈希校验拒绝");
        let failures = work_dir.join("update_failures.txt");
        assert!(failures.is_file(), "失败应被记录进 update_failures.txt");
        let count: u32 = std::fs::read_to_string(&failures)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(count, 1, "应恰好记录 1 次失败");
    }

    // ---- 场景 4：连续 3 次失败 → SafeMode（即使补丁是好的也拦截）----
    {
        let (patch, patch_hash, target_hash) = make_patch(&old_bytes, &new_bytes, dir.path()).await;
        let bad_hash = "0".repeat(64);
        // 再失败 2 次（累计 3 次）
        for _ in 0..2 {
            let server = MockServer::start();
            serve_signed(
                &server,
                &sk,
                manifest_json("0.2.0", &platform, Some(("0.1.0", &bad_hash, &target_hash))),
                Some(patch.clone()),
            );
            let updater = DeltaUpdater::new(
                UpdateConfig::new(server.url("/latest.json"), &pk_b64)
                    .current_version("0.1.0")
                    .executable_path(&old_exe)
                    .work_dir(&work_dir),
            );
            let _ = updater.update().await;
        }
        // 此时失败计数 = 3：换成好补丁也必须被安全模式拦截
        let server = MockServer::start();
        serve_signed(
            &server,
            &sk,
            manifest_json(
                "0.2.0",
                &platform,
                Some(("0.1.0", &patch_hash, &target_hash)),
            ),
            Some(patch),
        );
        let updater = DeltaUpdater::new(
            UpdateConfig::new(server.url("/latest.json"), &pk_b64)
                .current_version("0.1.0")
                .executable_path(&old_exe)
                .work_dir(&work_dir),
        );
        assert!(
            matches!(updater.update().await.unwrap(), UpdateOutcome::SafeMode),
            "连续失败 3 次后应进入安全模式"
        );
    }

    // ---- 场景 5：已是最新版本 → 无补丁命中，回退全量（前端继续问全量通道）----
    // 用全新的 work_dir：前面场景已把失败计数打到 3（安全模式），会污染本场景
    {
        let fresh = tempdir().unwrap();
        let fresh_exe = fresh.path().join("sereno-desktop.exe");
        std::fs::write(&fresh_exe, &old_bytes).unwrap();

        let server = MockServer::start();
        serve_signed(&server, &sk, manifest_json("0.1.0", &platform, None), None);
        let updater = DeltaUpdater::new(
            UpdateConfig::new(server.url("/latest.json"), &pk_b64)
                .current_version("0.1.0")
                .executable_path(&fresh_exe)
                .work_dir(fresh.path()),
        );
        assert!(
            matches!(
                updater.update().await.unwrap(),
                UpdateOutcome::FallbackToFullDownload { .. }
            ),
            "同版本应视为无需增量，交回全量通道"
        );
    }
}
