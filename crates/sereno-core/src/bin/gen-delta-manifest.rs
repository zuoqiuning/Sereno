//! 发布工具：生成增量更新补丁与更新清单（`latest.json`）。
//!
//! 对应引擎 CLI（tauri-delta-updater 的 generate）在主项目里的等价实现，
//! 直接复用 workspace 已编译的依赖树，也保证补丁算法与客户端完全同源。
//!
//! ## 用法
//!
//! ```text
//! # 生成补丁 + 合并清单（产物写到当前目录）
//! cargo run --release -p sereno-core --bin gen-delta-manifest -- \
//!     gen <旧版安装包目录> <新版安装包> <新版版本号> <下载基地址> <现有 latest.json>
//!
//! # 验签（发布前的最后一道自检，通过则静默退出 0）
//! cargo run --release -p sereno-core --bin gen-delta-manifest -- \
//!     verify <latest.json> <latest.json.sig> <minisign 公钥 base64>
//! ```
//!
//! 签名本身由 tauri CLI 完成（与安装包同一把 minisign 密钥）：
//!
//! ```text
//! TAURI_SIGNING_PRIVATE_KEY=<私钥内容> pnpm tauri signer sign latest.json
//! ```
//!
//! ## 产物
//!
//! - `<from_version>_<to_version>.patch`：qbsdiff 增量补丁
//! - `latest.json`：合并了新旧补丁记录的完整更新清单（Tauri 插件 + 增量客户端共用）
//! - `latest.json.sig`：清单签名（增量客户端 fail-closed 验签的依据）

use std::path::Path;

use sereno_core::updater::{
    PatchInfo, UpdateManifest, calculate_hash, create_patch, verify_manifest_signature,
};
// apply_patch 在 sereno-core 内是私有使用，这里从引擎 crate 直接取
use tauri_delta_updater::apply_patch;

fn extract_version(file_name: &str) -> Option<String> {
    // 匹配 Sereno_0.2.0_x64-setup.exe / Sereno-v0.2.0-arm64.exe 等
    let re = regex::Regex::new(r"[vV]?(\d+\.\d+\.\d+(?:-[\w.]+)?)").ok()?;
    re.captures(file_name)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn detect_platform(file_name: &str) -> &'static str {
    let lower = file_name.to_lowercase();
    if lower.contains("aarch64") || lower.contains("arm64") {
        "windows-aarch64"
    } else if lower.contains("i686") || lower.contains("x86-") {
        "windows-i686"
    } else {
        "windows-x86_64"
    }
}

fn die(message: &str) -> ! {
    eprintln!("[gen-delta-manifest] {message}");
    std::process::exit(1);
}

async fn generate_manifest(args: &[String]) {
    // gen <old_dir> <new_exe> <new_version> <base_url> <existing_json> [安装包exe]
    // 第 6 个参数是可选的安装包路径：提供时执行「防混合代际守卫」——
    // 清单 platforms 里的全量签名必须与这份安装包匹配。
    let [old_dir, new_exe, new_version, base_url, existing_json] = match args {
        [a, b, c, d, e, ..] => [a, b, c, d, e],
        _ => die(
            "gen 模式参数应为：<旧包目录> <新包应用exe> <新版本> <base_url> <现有 latest.json> [安装包exe]",
        ),
    };

    let mut manifest: UpdateManifest = match std::fs::read_to_string(existing_json) {
        Ok(content) => serde_json::from_str(&content)
            .unwrap_or_else(|e| die(&format!("解析现有 {existing_json} 失败: {e}"))),
        Err(_) => die(&format!(
            "现有清单 {existing_json} 不存在——先跑 gen-updater-manifest.mjs"
        )),
    };

    let new_path = Path::new(new_exe);
    if !new_path.is_file() {
        die(&format!("新版安装包不存在: {new_exe}"));
    }
    let new_file_name = new_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // 全量字段必须已经就位：签名是 gen-updater-manifest.mjs 从构建产物
    // 的 .sig 读出来的，这里绝不碰它，也绝不静默发布无签名清单。
    if manifest.platforms.is_empty() {
        die("现有清单 platforms 为空——先用 scripts/gen-updater-manifest.mjs 生成全量清单");
    }
    if manifest.version != new_version.as_str() {
        die(&format!(
            "现有清单版本 ({}) 与目标版本 ({new_version}) 不一致",
            manifest.version
        ));
    }

    let platform = detect_platform(&new_file_name);
    let new_hash = calculate_hash(new_path).await.expect("计算新版哈希失败");

    // 防混合代际守卫：清单 platforms 里的全量签名必须与**这份**安装包匹配。
    // 曾经发生过：gen-updater-manifest 写错目录，旧签名 + 新 exe 混进同一份
    // 清单，插件验签必然失败（用户看到 "The signature verification failed"）。
    // 提供安装包路径时在这里拦住，绝不让不一致的清单流出去。
    let Some(platform_info) = manifest.platforms.get(platform) else {
        die(&format!("清单 platforms 里没有 {platform} 条目"));
    };
    if let Some(installer) = args.get(5) {
        let installer_path = Path::new(installer);
        if !installer_path.is_file() {
            die(&format!("安装包不存在: {installer}"));
        }
        if let Err(e) = sereno_core::updater::verify_manifest_signature(
            &std::fs::read(installer_path).unwrap_or_else(|e| die(&format!("读取安装包失败: {e}"))),
            platform_info.signature.trim(),
            &std::env::var("SERENO_UPDATER_PUBKEY").unwrap_or_else(|_| {
                die("缺少环境变量 SERENO_UPDATER_PUBKEY（与 tauri.conf.json 的 pubkey 一致）")
            }),
        ) {
            die(&format!(
                "清单的全量签名与安装包不匹配——清单可能是旧构建生成的！\
                 重新执行 scripts/gen-updater-manifest.mjs 后再试。详情: {e}"
            ));
        }
        println!("[OK] 全量签名与安装包匹配（防混合代际守卫通过）");
    }

    // 遍历旧包目录（一层即可），为每个旧版本生成补丁（同版本跳过）
    let mut new_patches: Vec<PatchInfo> = Vec::new();
    let entries = std::fs::read_dir(old_dir)
        .unwrap_or_else(|e| die(&format!("读取旧包目录 {old_dir} 失败: {e}")));
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || path.extension().unwrap_or_default() != "exe" {
            continue;
        }
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(old_version) = extract_version(&file_name) else {
            eprintln!("[WARN] 跳过无法解析版本的文件: {file_name}");
            continue;
        };
        if old_version == new_version.as_str() {
            continue;
        }

        let patch_name = format!("{old_version}_{new_version}.patch");
        println!("[INFO] 生成补丁 {old_version} → {new_version}: {patch_name}");
        create_patch(&path, new_path, Path::new(&patch_name))
            .await
            .unwrap_or_else(|e| panic!("生成补丁失败 {patch_name}: {e}"));
        let patch_hash = calculate_hash(Path::new(&patch_name))
            .await
            .expect("补丁哈希失败");

        let patch_size = std::fs::metadata(&patch_name).map(|m| m.len()).ok();
        new_patches.push(PatchInfo {
            from_version: old_version,
            to_version: new_version.to_string(),
            patch_url: format!("{}/{}", base_url.trim_end_matches('/'), patch_name),
            patch_hash,
            target_hash: new_hash.clone(),
            patch_size,
        });
    }

    // 智能合并（分组格式，patches 按平台分组）：本平台内同 from_version
    // 的新补丁覆盖旧记录，其它平台与历史补丁全部保留
    let entry = manifest.patches.entry(platform.to_string()).or_default();
    entry.retain(|p| {
        !new_patches
            .iter()
            .any(|new_p| p.from_version == new_p.from_version)
    });
    entry.extend(new_patches);

    let json_bytes = serde_json::to_vec_pretty(&manifest).expect("序列化清单失败");
    std::fs::write("latest.json", &json_bytes).expect("写入 latest.json 失败");
    let patch_count = manifest
        .patches
        .get(platform)
        .map(Vec::len)
        .unwrap_or_default();
    println!(
        "[OK] latest.json 已生成：版本 {}，平台 {platform}，补丁 {patch_count} 条",
        manifest.version
    );
    println!(
        "[NEXT] 签名后上传：TAURI_SIGNING_PRIVATE_KEY=<私钥内容> pnpm tauri signer sign latest.json"
    );
}

/// 发布前的闭环自检：用引擎同款 apply_patch 把补丁打回旧包，
/// 产物必须与新版逐字节一致（模拟客户端完整合并路径）。
async fn apply(args: &[String]) {
    // apply <旧版应用exe> <patch> <输出文件> <patch_hash> <target_hash>
    let [old_exe, patch, out, patch_hash, target_hash] = match args {
        [a, b, c, d, e] => [a, b, c, d, e],
        _ => die("apply 模式参数应为：<旧版exe> <patch> <输出> <patch_hash> <target_hash>"),
    };
    apply_patch(
        Path::new(old_exe),
        Path::new(patch),
        Path::new(out),
        patch_hash,
        target_hash,
    )
    .await
    .unwrap_or_else(|e| die(&format!("补丁合并失败（双哈希校验未过）: {e}")));
    println!("[OK] 合并成功：{out}");
}

fn verify(args: &[String]) {
    // verify <latest.json> <latest.json.sig> <pubkey_b64>
    let [json_path, sig_path, pubkey] = match args {
        [a, b, c] => [a, b, c],
        _ => die("verify 模式参数应为：<latest.json> <latest.json.sig> <公钥 base64>"),
    };
    let manifest =
        std::fs::read(json_path).unwrap_or_else(|e| die(&format!("读取 {json_path} 失败: {e}")));
    let signature = std::fs::read_to_string(sig_path)
        .unwrap_or_else(|e| die(&format!("读取 {sig_path} 失败: {e}")));
    match verify_manifest_signature(&manifest, signature.trim(), pubkey) {
        Ok(()) => println!("[OK] 清单验签通过：{json_path}"),
        Err(e) => die(&format!("验签失败：{e}")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rt = tokio::runtime::Runtime::new().expect("创建运行时失败");
    rt.block_on(async {
        match args.first().map(String::as_str) {
            Some("gen") => generate_manifest(&args[1..]).await,
            Some("apply") => apply(&args[1..]).await,
            Some("verify") => verify(&args[1..]),
            _ => die("用法: gen-delta-manifest <gen|apply|verify> [...参数]，见文件头注释"),
        }
    });
}
