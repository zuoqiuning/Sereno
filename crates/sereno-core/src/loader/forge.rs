//! Forge 加载器安装。

use std::path::Path;

use crate::models::instance::LoaderInfo;
use crate::models::settings::AppSettings;
use crate::utils::http::{MAX_METADATA_BYTES, read_json_limited};
use crate::{LauncherError, Result};

/// 通过官方安装器安装 Forge（客户端模式）。
pub async fn install_forge(
    client: &reqwest::Client,
    settings: &AppSettings,
    instance_dir: &Path,
    mc_version: &str,
    java_exe: &str,
    specified_version: Option<&str>,
) -> Result<LoaderInfo> {
    let forge_ver = match specified_version {
        Some(v) => v.to_string(),
        None => recommended_version(client, mc_version).await?,
    };

    // Maven 路径：1.12 及以前是 `{mc}-{ver}-{mc}`，1.13+ 是 `{mc}-{ver}`
    let file_version = forge_file_version(mc_version, &forge_ver);
    let installer_name = format!("forge-{file_version}-installer.jar");
    let download_url = format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{file_version}/{installer_name}"
    );
    let installer_path = instance_dir.join(".launcher").join(&installer_name);
    let target_dir = instance_dir.to_string_lossy().to_string();

    tracing::info!("下载 Forge 安装器: {download_url}");
    crate::loader::fetch_installer(
        client,
        &download_url,
        &installer_path,
        "Forge",
        settings.download.prefer_mirror,
    )
    .await?;

    // 安装器需要这一层目录结构存在；缺了会直接报错退出
    write_launcher_profiles(instance_dir)?;

    // 1.13+ 的 Forge 与 NeoForge 一样需要 processors 与大量支持库：
    // 先用启动器自己的镜像下载器拉齐，安装器只做本地处理
    let modern = is_modern(mc_version);
    if modern {
        crate::loader::preinstall_libraries(
            client,
            &installer_path,
            instance_dir,
            settings,
            mc_version,
            "Forge",
        )
        .await?;
    }

    tracing::info!("运行 Forge 安装器（客户端模式）");
    let mut args = vec!["--install-client".to_string(), target_dir.clone()];
    if modern {
        // 支持库已预下载，优先离线安装；缺文件时再退回联网安装
        args.push("--offline".to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let result =
        crate::loader::run_installer(java_exe, &installer_path, &arg_refs, instance_dir, "Forge")
            .await;

    if result.is_err() && modern {
        tracing::warn!("Forge 离线安装失败，退回联网安装");
        crate::loader::run_installer(
            java_exe,
            &installer_path,
            &["--install-client", target_dir.as_str()],
            instance_dir,
            "Forge",
        )
        .await?;
    } else {
        result?;
    }

    let _ = tokio::fs::remove_file(&installer_path).await;

    Ok(LoaderInfo {
        r#type: "forge".to_string(),
        version: Some(forge_ver),
    })
}

/// 官方 promotions 接口，失败时回退 BMCLAPI。
async fn recommended_version(client: &reqwest::Client, mc_version: &str) -> Result<String> {
    let urls = [
        "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
        "https://bmclapi2.bangbang93.com/forge/promotions_slim.json",
    ];

    let mut last_error = None;
    for url in urls {
        let response = match client.get(url).send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                last_error = Some(format!("HTTP {}", response.status()));
                continue;
            }
            Err(e) => {
                last_error = Some(e.to_string());
                continue;
            }
        };

        let json: serde_json::Value = read_json_limited(response, MAX_METADATA_BYTES).await?;
        let promos = json.get("promos").and_then(|p| p.as_object());
        let recommended = format!("{mc_version}-recommended");
        let latest = format!("{mc_version}-latest");
        let version = promos
            .and_then(|p| p.get(&recommended).or_else(|| p.get(&latest)))
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(version) = version {
            return Ok(version);
        }
    }

    Err(LauncherError::LoaderNotFound {
        loader: "forge".into(),
        version: mc_version.into(),
    })
    .inspect_err(|_| {
        if let Some(error) = &last_error {
            tracing::warn!(%error, "Forge 版本列表获取失败");
        }
    })
}

fn forge_file_version(mc_version: &str, forge_version: &str) -> String {
    let mut parts = mc_version.split('.');
    let major: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
    let minor: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(99);

    if major == 1 && minor <= 12 {
        format!("{mc_version}-{forge_version}-{mc_version}")
    } else {
        format!("{mc_version}-{forge_version}")
    }
}

/// 1.13+ 的 Forge 安装器支持 `--mirror`，旧版不支持。
fn is_modern(mc_version: &str) -> bool {
    let mut parts = mc_version.split('.');
    let major: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
    let minor: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    major == 1 && minor >= 13
}

pub(crate) fn write_launcher_profiles(instance_dir: &Path) -> Result<()> {
    let profile_path = instance_dir.join("launcher_profiles.json");
    if !profile_path.exists() {
        crate::utils::fs::atomic_write_str(
            &profile_path,
            r#"{"profiles":{},"settings":{},"version":3}"#,
        )?;
    }
    Ok(())
}
