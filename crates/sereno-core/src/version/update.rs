//! 启动器自身的更新检查与下载。
//!
//! ## 为什么不用 `tauri-plugin-updater`
//!
//! Tauri 官方的更新插件依赖 **minisign 签名**：发布端要用私钥给更新清单签名，
//! 客户端用公钥验签。私钥的管理是一整套独立的密钥基础设施，
//! 而本项目的代码签名证书**尚未采购**（《发布与防投毒》第五节已记录）。
//!
//! 与其放一个「看起来能更新、实际没有签名保护」的通道，不如沿用项目
//! **已经建立的分发信任链**：
//!
//! - 官方 GitHub Releases 提供安装包与 `SHA256SUMS.txt`；
//! - 客户端下载后**逐字节校验 SHA-256**，对不上就拒绝执行；
//! - 校验通过后由用户确认再启动安装程序。
//!
//! 这与《发布与防投毒》第二节「第 4 层：分发信任」是同一套逻辑：
//! 用户能自己核对哈希，就能分辨官方版与倒卖版。
//!
//! ## 安全边界
//!
//! - 只信任 `zuoqiuning/Sereno` 这一个仓库的 Release；
//! - 下载地址必须来自 Release 的资产列表，**不接受前端传入任意 URL**；
//! - 安装包必须通过 SHA-256 校验才会被启动。

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{LauncherError, Result};

/// 发布仓库（唯一可信来源）。
pub const REPO: &str = "zuoqiuning/Sereno";

/// GitHub Releases API。
const RELEASES_API: &str = "https://api.github.com/repos/zuoqiuning/Sereno/releases/latest";

/// 校验和清单的文件名。
///
/// 与 `scripts/gen-checksums.mjs` 生成的产物同名。
pub const CHECKSUM_FILE: &str = "SHA256SUMS.txt";

/// 单个 Release 的元数据。
#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// 更新检查的结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// 是否有可用更新。
    pub available: bool,
    /// 最新版本号（不带 `v` 前缀）。
    pub latest_version: String,
    /// 当前版本号。
    pub current_version: String,
    /// 更新说明（Markdown 原文）。
    pub notes: String,
    /// 安装包文件名。
    pub asset_name: Option<String>,
    /// 安装包大小（字节）。
    #[specta(type = specta_typescript::Number)]
    pub asset_size: u64,
    /// 是否是预发布版本。
    pub prerelease: bool,
}

/// 下载更新的结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownload {
    /// 安装包的本地路径。
    pub path: String,
    /// 文件大小（字节）。
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
    /// 校验用的 SHA-256。
    pub sha256: String,
}

/// 检查是否有新版本。
pub async fn check(client: &reqwest::Client) -> Result<UpdateInfo> {
    let current = crate::APP_VERSION.to_string();
    let release = fetch_release(client).await?;

    let Some(release) = release else {
        // 仓库还没有发布任何 Release——不是错误，只是没有更新可装。
        return Ok(UpdateInfo {
            available: false,
            latest_version: current.clone(),
            current_version: current,
            notes: String::new(),
            asset_name: None,
            asset_size: 0,
            prerelease: false,
        });
    };

    let latest = release.tag_name.trim_start_matches('v').to_string();

    // 找安装包：NSIS 的 setup.exe，或 zip。
    let installer = release
        .assets
        .iter()
        .find(|asset| is_installer(&asset.name))
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|asset| asset.name.to_ascii_lowercase().ends_with(".zip"))
        });

    let available =
        !release.draft && compare_versions(&latest, &current) == std::cmp::Ordering::Greater;

    Ok(UpdateInfo {
        available,
        latest_version: latest,
        current_version: current,
        notes: release.body,
        asset_name: installer.map(|asset| asset.name.clone()),
        asset_size: installer.map(|asset| asset.size).unwrap_or(0),
        prerelease: release.prerelease,
    })
}

/// 拉取最新的 Release；仓库还没发过 Release 时返回 `None`。
async fn fetch_release(client: &reqwest::Client) -> Result<Option<Release>> {
    let response = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("检查更新失败：{e}"),
            url: RELEASES_API.to_string(),
        })?;

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("检查更新失败：服务器返回 HTTP {status}"),
            url: RELEASES_API.to_string(),
        });
    }

    let release: Release = crate::utils::http::read_json_limited(response, 2 * 1024 * 1024).await?;
    Ok(Some(release))
}

/// 是否是安装包。
fn is_installer(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with("-setup.exe") || lower.ends_with("_setup.exe") || lower.ends_with(".msi")
}

/// 下载更新并校验 SHA-256。
///
/// **校验失败绝不返回成功路径**：调用方拿不到可执行的路径。
///
/// 下载地址**从 Release 的资产列表里取**（`browser_download_url`），
/// 不在前端与本函数之间传 URL——避免前端构造出任意地址。
pub async fn download(client: &reqwest::Client, info: &UpdateInfo) -> Result<UpdateDownload> {
    let release = fetch_release(client)
        .await?
        .ok_or_else(|| LauncherError::ResourceNotFound {
            project_id: "更新包".to_string(),
        })?;

    // 找到与检查结果同名的资产——用它的官方下载地址。
    let asset = release
        .assets
        .iter()
        .find(|asset| Some(asset.name.as_str()) == info.asset_name.as_deref())
        .ok_or_else(|| LauncherError::ResourceNotFound {
            project_id: info
                .asset_name
                .clone()
                .unwrap_or_else(|| "更新包".to_string()),
        })?;

    // 1. 先取校验和清单
    let checksum_asset = release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(CHECKSUM_FILE));
    let expected = match checksum_asset {
        Some(checksum) => {
            fetch_expected_checksum(client, &checksum.browser_download_url, &asset.name).await?
        }
        None => {
            return Err(LauncherError::DownloadFailed {
                message: format!("该版本没有附带 {CHECKSUM_FILE}，无法验证下载内容，拒绝安装"),
                url: asset.browser_download_url.clone(),
            });
        }
    };

    // 2. 下载安装包
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("下载更新失败：{e}"),
            url: asset.browser_download_url.clone(),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("下载更新失败：服务器返回 HTTP {status}"),
            url: asset.browser_download_url.clone(),
        });
    }

    let bytes = crate::utils::http::read_limited(response, 512 * 1024 * 1024).await?;

    // 3. 校验 SHA-256
    let actual = sha256_bytes(&bytes);
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(LauncherError::ChecksumMismatch {
            path: asset.name.clone(),
            expected,
            actual,
        });
    }

    // 4. 落盘到状态目录
    let dir = crate::utils::paths::state_dir()?.join("updates");
    crate::utils::paths::ensure_dir(&dir)?;
    let path = dir.join(&asset.name);
    crate::utils::fs::atomic_write(&path, &bytes)?;

    Ok(UpdateDownload {
        path: path.to_string_lossy().to_string(),
        size: bytes.len() as u64,
        sha256: actual,
    })
}

/// 从校验和清单里取出指定文件的哈希。
///
/// 清单格式是标准 sha256sum 格式：`<hash>  `（两个空格）。
async fn fetch_expected_checksum(
    client: &reqwest::Client,
    url: &str,
    asset_name: &str,
) -> Result<String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("获取校验和清单失败：{e}"),
            url: url.to_string(),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!(
                "获取校验和清单失败：服务器返回 HTTP {status}（该版本可能没有附带清单）"
            ),
            url: url.to_string(),
        });
    }

    let text = crate::utils::http::read_limited(response, 1024 * 1024).await?;
    let text = String::from_utf8_lossy(&text);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        // sha256sum 格式里文件名可能带 `*`（二进制模式标记）
        let name = name.trim().trim_start_matches('*');

        // 清单里的路径是**相对打包目录**的（如 `nsis/Sereno_0.2.0_x64-setup.exe`），
        // 而 Release 资产名只有文件名（`Sereno_0.2.0_x64-setup.exe`）。
        // 因此按文件名匹配：先比全名，再比最后一段。
        let matched = name == asset_name
            || name
                .rsplit('/')
                .next()
                .is_some_and(|last| last == asset_name);

        if matched {
            return Ok(hash.trim().to_ascii_lowercase());
        }
    }

    Err(LauncherError::ResourceNotFound {
        project_id: format!("校验和清单里没有 {asset_name}"),
    })
}

/// 计算字节的 SHA-256。
fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// 启动下载好的安装程序。
///
/// **只在校验通过后调用**：调用方拿到的路径来自 [`download`]，
/// 那一步已经做过 SHA-256 校验。
pub fn launch_installer(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(LauncherError::Io {
            message: format!("安装包不存在：{}", path.display()),
        });
    }

    opener::open(path).map_err(|e| LauncherError::Io {
        message: format!("启动安装程序失败：{e}"),
    })
}

/// 比较两个版本号。
///
/// 支持 `1.2.3` 与 `1.2.3-beta.1` 两种形式：
/// 正式版大于同号预发布版；预发布之间按标识符逐段比较。
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    fn split(value: &str) -> (Vec<u64>, Option<String>) {
        let value = value.trim().trim_start_matches('v');
        let (core, pre) = match value.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_string())),
            None => (value, None),
        };
        let numbers = core
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect();
        (numbers, pre)
    }

    let (left, left_pre) = split(a);
    let (right, right_pre) = split(b);

    for index in 0..left.len().max(right.len()) {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        match l.cmp(&r) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    match (left_pre, right_pre) {
        (None, None) => std::cmp::Ordering::Equal,
        // 有预发布标记的版本小于同号的正式版
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(l), Some(r)) => compare_prerelease(&l, &r),
    }
}

/// 比较两个预发布标识符。
///
/// 语义化版本规定：纯数字的标识符按数值比较，数字优先于字母，
/// 字母标识符按 ASCII 顺序比较。
fn compare_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.split('.');
    let mut right = b.split('.');

    loop {
        match (left.next(), right.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(l), Some(r)) => {
                let l_num = l.parse::<u64>().ok();
                let r_num = r.parse::<u64>().ok();
                let ordering = match (l_num, r_num) {
                    (Some(lv), Some(rv)) => lv.cmp(&rv),
                    // 数字标识符优先于字母标识符
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => l.cmp(r),
                };
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

/// 更新文件在状态目录里的位置（供「稍后安装」用）。
pub fn pending_update_path(asset_name: &str) -> Result<PathBuf> {
    Ok(crate::utils::paths::state_dir()?
        .join("updates")
        .join(asset_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions() {
        use std::cmp::Ordering;
        assert_eq!(compare_versions("0.2.0", "0.1.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.1.0", "0.2.0"), Ordering::Less);
        assert_eq!(compare_versions("1.10.0", "1.9.0"), Ordering::Greater);
        // `v` 前缀不影响比较
        assert_eq!(compare_versions("v0.2.0", "0.1.0"), Ordering::Greater);
    }

    #[test]
    fn prerelease_is_lower_than_release() {
        use std::cmp::Ordering;
        assert_eq!(compare_versions("1.0.0", "1.0.0-beta.1"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0-beta.1", "1.0.0"), Ordering::Less);
        assert_eq!(
            compare_versions("1.0.0-beta.1", "1.0.0-beta.2"),
            Ordering::Less
        );
        assert_eq!(
            compare_versions("1.0.0-alpha", "1.0.0-beta"),
            Ordering::Less
        );
        // 数字标识符优先于字母
        assert_eq!(compare_versions("1.0.0-1", "1.0.0-alpha"), Ordering::Less);
        assert_eq!(
            compare_versions("1.0.0-beta", "1.0.0-beta.1"),
            Ordering::Less
        );
    }

    #[test]
    fn identifies_installer_assets() {
        assert!(is_installer("Sereno_0.2.0_x64-setup.exe"));
        assert!(is_installer("Sereno_0.2.0_x64-setup.EXE"));
        assert!(is_installer("Sereno-Setup.exe"));
        assert!(is_installer("Sereno.msi"));
        assert!(!is_installer("SHA256SUMS.txt"));
        assert!(!is_installer("screenshot.png"));
    }

    /// 清单里的路径是相对打包目录的，资产名只有文件名——两者要能对上。
    #[test]
    fn checksum_entry_matches_by_file_name() {
        let manifest = "# 注释行\n\
                        11b0261e4dde9169173b24b88cbc1622859f4a68  nsis/Sereno_0.2.0_x64-setup.exe\n\
                        2d2222865fd3a230a820830457bb99a2  nsis/Sereno_0.2.0_x64-setup.msi\n";

        let lookup = |asset: &str| -> Option<String> {
            manifest.lines().find_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let mut parts = line.splitn(2, char::is_whitespace);
                let hash = parts.next()?;
                let name = parts.next()?.trim().trim_start_matches('*');
                let matched =
                    name == asset || name.rsplit('/').next().is_some_and(|last| last == asset);
                matched.then(|| hash.to_string())
            })
        };

        assert_eq!(
            lookup("Sereno_0.2.0_x64-setup.exe").as_deref(),
            Some("11b0261e4dde9169173b24b88cbc1622859f4a68")
        );
        assert_eq!(
            lookup("Sereno_0.2.0_x64-setup.msi").as_deref(),
            Some("2d2222865fd3a230a820830457bb99a2")
        );
        assert!(lookup("other.exe").is_none());
    }
}
