//! NeoForge 加载器安装。

use std::path::Path;

use crate::models::instance::LoaderInfo;
use crate::models::settings::AppSettings;
use crate::{LauncherError, Result};

const OFFICIAL_METADATA: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
const MIRROR_METADATA: &str =
    "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/maven-metadata.xml";

/// 列出某个 MC 版本可用的 NeoForge 稳定版本（新 → 旧）。
///
/// `maven-metadata.xml` 里混有 alpha / beta / snapshot 版本，顺序也是字典序，
/// 直接取用会把 `26.3.0.99` 排在 `26.3.0.101` 前面。这里只保留纯数字的稳定版，
/// 按构建号倒序排序，保证界面第一个选项就是最新版。
pub async fn list_versions(
    client: &reqwest::Client,
    prefer_mirror: bool,
    mc_version: &str,
) -> Result<Vec<String>> {
    let Some(prefix) = default_prefix(mc_version) else {
        return Ok(Vec::new());
    };

    let urls = if prefer_mirror {
        [MIRROR_METADATA, OFFICIAL_METADATA]
    } else {
        [OFFICIAL_METADATA, MIRROR_METADATA]
    };

    let mut versions = Vec::new();
    for url in urls {
        let Ok(resp) = client.get(url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(text) = resp.text().await else {
            continue;
        };

        versions = parse_versions(&text, &prefix);
        if !versions.is_empty() {
            break;
        }
    }

    Ok(versions)
}

/// 从 `maven-metadata.xml` 里筛出该前缀下的稳定版本，按构建号倒序。
fn parse_versions(text: &str, prefix: &str) -> Vec<String> {
    static VERSION_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = VERSION_RE.get_or_init(|| {
        regex::Regex::new(r"<version>([^<]+)</version>").expect("NeoForge 版本号正则必须合法")
    });

    let mut found: Vec<(u64, String)> = re
        .captures_iter(text)
        .filter_map(|cap| {
            let raw = cap[1].strip_prefix(prefix)?;
            // 只保留「前缀 + 纯数字构建号」，过滤 alpha / beta / snapshot
            if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Some((raw.parse().ok()?, cap[1].to_string()))
        })
        .collect();

    found.sort_by_key(|(build, _)| std::cmp::Reverse(*build));
    found.into_iter().map(|(_, version)| version).collect()
}

/// 安装 NeoForge（客户端模式）。
pub async fn install_neoforge(
    client: &reqwest::Client,
    settings: &AppSettings,
    instance_dir: &Path,
    mc_version: &str,
    java_exe: &str,
    specified_version: Option<&str>,
) -> Result<LoaderInfo> {
    let version = match specified_version {
        Some(v) => v.to_string(),
        None => list_versions(client, settings.download.prefer_mirror, mc_version)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| LauncherError::LoaderNotFound {
                loader: "neoforge".into(),
                version: mc_version.into(),
            })?,
    };

    let installer_name = format!("neoforge-{version}-installer.jar");
    let download_url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
    );
    let installer_path = instance_dir.join(".launcher").join(&installer_name);
    let target_dir = instance_dir.to_string_lossy().to_string();

    crate::loader::fetch_installer(
        client,
        &download_url,
        &installer_path,
        "NeoForge",
        settings.download.prefer_mirror,
    )
    .await?;
    crate::loader::forge::write_launcher_profiles(instance_dir)?;

    // 关键：安装器默认串行直连 maven.neoforged.net，国内经常超时。
    // 先用镜像 + 多线程预下载全部支持库，再让安装器只跑本地处理器。
    crate::loader::preinstall_libraries(
        client,
        &installer_path,
        instance_dir,
        settings,
        mc_version,
        "NeoForge",
    )
    .await?;

    // 所有依赖（含 universal jar 与 mappings）已预下载完毕，
    // 安装器加 `--offline`：只跑本地处理器，绝不联网超时。
    // 事务性：安装器失败时删除它留下的半成品版本目录
    let versions_before = crate::loader::snapshot_versions(instance_dir);
    crate::loader::run_installer(
        java_exe,
        &installer_path,
        &["--install-client", target_dir.as_str(), "--offline"],
        instance_dir,
        "NeoForge",
    )
    .await
    .map_err(|error| {
        crate::loader::rollback_new_versions(instance_dir, &versions_before);
        error
    })?;

    let _ = tokio::fs::remove_file(&installer_path).await;

    Ok(LoaderInfo {
        r#type: "neoforge".to_string(),
        version: Some(version),
    })
}

/// NeoForge 版本号前缀。
///
/// 两套命名规则都要覆盖：
/// - 旧体系 `1.x.y`：NeoForge 直接沿用 `{minor}.{patch}.`，例如 `1.21.1` → `21.1.`、
///   `1.21.11` → `21.11.`；`1.20.1` 是历史例外，用 `47.1.`。
/// - 新体系 `26.x[.y]`：Mojang 改用「年份.序号.补丁」，NeoForge 对应
///   `{major}.{minor}.{patch}.`，补丁缺省补 0，例如 `26.3` → `26.3.0.`、
///   `26.1.2` → `26.1.2.`。
///
/// 这样新增小版本不需要再维护一张映射表。
pub fn default_prefix(mc_version: &str) -> Option<String> {
    if mc_version == "1.20.1" {
        return Some("47.1.".to_string());
    }

    let mut parts = mc_version.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);

    if major >= 2 {
        return Some(format!("{major}.{minor}.{patch}."));
    }
    if major == 1 && minor >= 20 {
        return Some(format!("{minor}.{patch}."));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_follow_minecraft_versions() {
        assert_eq!(default_prefix("1.20.1").as_deref(), Some("47.1."));
        assert_eq!(default_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(default_prefix("1.21.11").as_deref(), Some("21.11."));
        assert_eq!(default_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(default_prefix("1.12.2"), None);
    }

    #[test]
    fn prefixes_follow_new_version_scheme() {
        assert_eq!(default_prefix("26.3").as_deref(), Some("26.3.0."));
        assert_eq!(default_prefix("26.2").as_deref(), Some("26.2.0."));
        assert_eq!(default_prefix("26.1.2").as_deref(), Some("26.1.2."));
        // 快照没有 NeoForge 支持，前缀解析失败即视为不支持
        assert_eq!(default_prefix("26.4-snapshot-6"), None);
        assert_eq!(default_prefix("26.3-rc-1"), None);
    }

    #[test]
    fn filters_prereleases_and_sorts_by_build_number() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <metadata><versioning><versions>
            <version>26.3.0.99</version>
            <version>26.3.0.101</version>
            <version>26.3.0.100</version>
            <version>26.3.0.0-alpha.1+snapshot-1</version>
            <version>26.4.0.5</version>
        </versions></versioning></metadata>"#;

        assert_eq!(
            parse_versions(xml, "26.3.0."),
            vec!["26.3.0.101", "26.3.0.100", "26.3.0.99"]
        );
        assert!(parse_versions(xml, "26.1.0.").is_empty());
    }
}
