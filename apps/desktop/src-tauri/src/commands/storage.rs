//! 存储管理命令：占用统计与缓存清理。

use sereno_core::Result;
use sereno_core::models::storage::StorageUsage;

use crate::state::AppState;

/// 统计实例 / 缓存 / 日志的磁盘占用。
///
/// 目录遍历是阻塞 IO，放到阻塞线程里跑，避免卡住 IPC 运行时。
#[tauri::command]
#[specta::specta]
pub async fn get_storage_usage(state: tauri::State<'_, AppState>) -> Result<StorageUsage> {
    let settings = state.snapshot_settings()?;
    tokio::task::spawn_blocking(move || sereno_core::storage::usage::usage(&settings))
        .await
        .map_err(|error| sereno_core::LauncherError::Internal {
            message: format!("统计磁盘占用失败: {error}"),
        })?
}

/// 清空元数据缓存；实例、存档与日志不受影响。
#[tauri::command]
#[specta::specta]
pub async fn clear_storage_cache() -> Result<()> {
    tokio::task::spawn_blocking(sereno_core::storage::usage::clear_cache)
        .await
        .map_err(|error| sereno_core::LauncherError::Internal {
            message: format!("清理缓存失败: {error}"),
        })?
}
