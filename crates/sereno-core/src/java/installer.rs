//! 自管 Java 运行时：通过 Azul Zulu 的公开 API 获取 Windows x64 JRE 直链，
//! 解压到 `%APPDATA%\Sereno\java\zulu-{major}`，探测逻辑会自动发现它。
//!
//! 选择 Azul 而不是 Adoptium：Adoptium 的下载会重定向到 GitHub，
//! 国内网络经常不可达；Azul 的 CDN 可以直连。

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::download::counter::SessionCounter;
use crate::download::queue::DownloadQueue;
use crate::download::task::{Checksum, DownloadTask, SourceUrl};
use crate::models::java::JavaCandidate;
use crate::models::settings::AppSettings;
use crate::utils::archive::extract_zip_safely;
use crate::utils::http::{MAX_METADATA_BYTES, build_client, read_json_limited};
use crate::utils::paths::{ensure_dir, state_dir};
use crate::{LauncherError, Result};

/// 支持的 Java 主版本，与 Minecraft 版本需求一一对应。
pub const SUPPORTED_MAJORS: [u32; 3] = [8, 17, 21];

#[derive(Deserialize)]
struct AzulPackage {
    download_url: String,
}

/// 下载并安装指定主版本的 Zulu JRE。
pub async fn install_jre(settings: &AppSettings, major: u32) -> Result<JavaCandidate> {
    if !SUPPORTED_MAJORS.contains(&major) {
        return Err(LauncherError::InvalidConfig {
            key: "major".into(),
            message: format!("不支持自动安装 Java {major}，可选 8 / 17 / 21"),
        });
    }

    let root = state_dir()?.join("java");
    ensure_dir(&root)?;
    let dest = root.join(format!("zulu-{major}"));

    // 已经装过就直接复用，避免重复下载几十 MB。
    if let Some(java_exe) = find_java_exe(&dest) {
        return crate::java::detector::probe(&java_exe.to_string_lossy()).await;
    }

    let client = build_client(settings)?;
    let download_url = resolve_download_url(&client, major).await?;

    let zip_path = root.join(format!("zulu-{major}.zip"));
    let _ = std::fs::remove_file(&zip_path);

    let queue = Arc::new(DownloadQueue::new(client, 1));
    let counter = Arc::new(SessionCounter::new("java-runtime", "java"));
    counter.add_total(1, 0);

    queue
        .execute_task(
            DownloadTask {
                id: format!("java-{major}"),
                display_name: format!("Zulu {major}"),
                sources: vec![SourceUrl {
                    url: download_url,
                    is_mirror: false,
                }],
                destination: zip_path.clone(),
                checksum: Checksum::None,
                size_hint: None,
                priority: 8,
                retry_policy: Default::default(),
            },
            CancellationToken::new(),
            None,
            "java-runtime",
            counter,
        )
        .await?;

    if let Some(parent) = dest.parent() {
        ensure_dir(parent)?;
    }
    let file = File::open(&zip_path).map_err(|e| LauncherError::Io {
        message: format!("打开 Java 安装包失败: {e}"),
    })?;
    extract_zip_safely(file, &dest)?;
    let _ = std::fs::remove_file(&zip_path);

    let java_exe = find_java_exe(&dest).ok_or_else(|| LauncherError::Io {
        message: "Java 解压完成，但没有找到 bin/java.exe".into(),
    })?;
    crate::java::detector::probe(&java_exe.to_string_lossy()).await
}

async fn resolve_download_url(client: &reqwest::Client, major: u32) -> Result<String> {
    let url = format!(
        "https://api.azul.com/metadata/v1/zulu/packages/?os=windows&arch=x64&archive_type=zip&java_package_type=jre&latest=true&page_size=1&java_version={major}"
    );
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("获取 Java {major} 下载地址失败: {e}"),
            url: url.clone(),
        })?;
    if !response.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("Java 下载接口返回 HTTP {}", response.status()),
            url,
        });
    }

    let packages: Vec<AzulPackage> = read_json_limited(response, MAX_METADATA_BYTES).await?;
    packages
        .into_iter()
        .next()
        .map(|package| package.download_url)
        .ok_or_else(|| LauncherError::DownloadFailed {
            message: format!("没有找到适配 Windows x64 的 Java {major} JRE"),
            url,
        })
}

/// Zulu 的 zip 通常多包一层 `zulu…/`，这里两种结构都兼容。
fn find_java_exe(root: &Path) -> Option<PathBuf> {
    let direct = root.join("bin").join("java.exe");
    if direct.is_file() {
        return Some(direct);
    }

    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let candidate = path.join("bin").join("java.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
