//! 加载器安装的统一入口。
//!
//! 「实例页装加载器」与「整合包安装」都要装加载器，
//! 差异只在调用时机，因此实现只有这一份。

use std::path::Path;

use crate::LauncherError;
use crate::Result;
use crate::models::instance::{InstanceMetadata, LoaderInfo};
use crate::models::settings::AppSettings;

/// 安装加载器，返回加载器信息（是否写回实例元数据由调用方决定）。
pub async fn install_loader_into(
    settings: &AppSettings,
    inst_dir: &Path,
    metadata: &InstanceMetadata,
    loader_type: &str,
    loader_version: Option<&str>,
) -> Result<LoaderInfo> {
    let client = crate::utils::http::build_client(settings)?;

    // 安装器自身对 Java 版本也有要求：旧 Forge 需要 Java 8，现代版本需要 17/21。
    // 优先用版本 JSON 里的权威字段，本地没有时才按版本号兜底。
    let required_java = crate::version::detail::read_local(inst_dir, &metadata.game_version)?
        .and_then(|detail| detail.required_java_major())
        .unwrap_or_else(|| installer_java_major(&metadata.game_version));
    let java =
        crate::java::detector::resolve(Some(required_java), settings.java.custom_path.as_deref())
            .await?;

    match loader_type {
        "fabric" => {
            crate::loader::fabric::install_fabric(
                &client,
                settings,
                inst_dir,
                &metadata.game_version,
                loader_version,
            )
            .await
        }
        "quilt" => {
            crate::loader::quilt::install_quilt(
                &client,
                settings,
                inst_dir,
                &metadata.game_version,
                loader_version,
            )
            .await
        }
        "forge" => {
            crate::loader::forge::install_forge(
                &client,
                settings,
                inst_dir,
                &metadata.game_version,
                &java.path,
                loader_version,
            )
            .await
        }
        "neoforge" => {
            crate::loader::neoforge::install_neoforge(
                &client,
                settings,
                inst_dir,
                &metadata.game_version,
                &java.path,
                loader_version,
            )
            .await
        }
        other => Err(LauncherError::InvalidConfig {
            key: "loaderType".into(),
            message: format!("未知的加载器类型: {other}"),
        }),
    }
}

/// 加载器安装器所需的 Java 主版本。
///
/// 只用于「版本 JSON 还没落盘」时的兜底；有本地 JSON 时以 `javaVersion` 为准。
pub fn installer_java_major(mc_version: &str) -> u32 {
    let mut parts = mc_version.split('.');
    let major: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
    let minor: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let patch: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);

    if major == 1 && minor <= 16 {
        8
    } else if major == 1 && minor >= 20 && patch >= 5 {
        21
    } else {
        17
    }
}
