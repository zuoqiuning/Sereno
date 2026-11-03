//! 版本域命令。

use sereno_core::Result;
use sereno_core::models::version::{VersionDetail, VersionManifest};

use crate::state::AppState;

/// 获取 Mojang 版本清单。
///
/// 优先读内存缓存（10 分钟 TTL），避免每次进下载页都重新走网络。
#[tauri::command]
#[specta::specta]
pub async fn get_version_manifest(state: tauri::State<'_, AppState>) -> Result<VersionManifest> {
    if let Some(cached) = state.manifest_cache.get() {
        tracing::debug!("命中版本清单缓存");
        return Ok(cached);
    }

    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let manifest = sereno_core::version::fetch_manifest(&client).await?;

    state.manifest_cache.set(manifest.clone());
    Ok(manifest)
}

/// 获取指定版本的详情（已合并 inheritsFrom）。
#[tauri::command]
#[specta::specta]
pub async fn get_version_detail(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<VersionDetail> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;

    // 清单有 10 分钟缓存；详情页没必要每次都重新拉一遍 manifest
    let manifest = match state.manifest_cache.get() {
        Some(manifest) => manifest,
        None => {
            let manifest = sereno_core::version::fetch_manifest(&client).await?;
            state.manifest_cache.set(manifest.clone());
            manifest
        }
    };

    sereno_core::version::resolve_detail(&client, &manifest, &id).await
}
