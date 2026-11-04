//! 备份命令：世界（存档）备份 + 内容快照。

use crate::state::AppState;
use sereno_core::Result;
use sereno_core::models::backup::{ContentSnapshot, WorldBackup};

#[tauri::command]
#[specta::specta]
pub fn create_world_backup(
    instance_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<WorldBackup> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy { id: instance_id });
    }
    let settings = state.snapshot_settings()?;
    sereno_core::backup::world::create(&settings, &instance_id)
}

#[tauri::command]
#[specta::specta]
pub fn list_world_backups(
    instance_id: String,
    _state: tauri::State<'_, AppState>,
) -> Result<Vec<WorldBackup>> {
    sereno_core::backup::world::list(&instance_id)
}

#[tauri::command]
#[specta::specta]
pub fn restore_world_backup(
    instance_id: String,
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy { id: instance_id });
    }
    let settings = state.snapshot_settings()?;
    sereno_core::backup::world::restore(&settings, &instance_id, &name)
}

/// 创建内容快照（mods / config / 资源包 / 光影）。
///
/// 打包整个内容目录是 IO 密集型操作（几百 MB 的整合包要几秒），
/// 放到阻塞线程里执行，避免占住 IPC 运行时。
#[tauri::command]
#[specta::specta]
pub async fn create_content_snapshot(
    instance_id: String,
    reason: Option<String>,
    pinned: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<ContentSnapshot> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy { id: instance_id });
    }
    let settings = state.snapshot_settings()?;
    let reason = reason.unwrap_or_else(|| "manual".to_string());

    tokio::task::spawn_blocking(move || {
        sereno_core::backup::snapshot::create(
            &settings,
            &instance_id,
            &reason,
            pinned.unwrap_or(false),
        )
    })
    .await
    .map_err(|error| sereno_core::LauncherError::Internal {
        message: format!("创建快照任务异常: {error}"),
    })?
}

#[tauri::command]
#[specta::specta]
pub fn list_content_snapshots(
    instance_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContentSnapshot>> {
    let settings = state.snapshot_settings()?;
    sereno_core::backup::snapshot::list(&settings, &instance_id)
}

/// 回滚到某个快照。回滚会**替换**当前 mods / config，属于破坏性操作。
#[tauri::command]
#[specta::specta]
pub async fn restore_content_snapshot(
    instance_id: String,
    snapshot_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy { id: instance_id });
    }
    let settings = state.snapshot_settings()?;

    tokio::task::spawn_blocking(move || {
        sereno_core::backup::snapshot::restore(&settings, &instance_id, &snapshot_id)
    })
    .await
    .map_err(|error| sereno_core::LauncherError::Internal {
        message: format!("回滚任务异常: {error}"),
    })?
}

#[tauri::command]
#[specta::specta]
pub async fn delete_content_snapshot(
    instance_id: String,
    snapshot_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy { id: instance_id });
    }
    let settings = state.snapshot_settings()?;
    tokio::task::spawn_blocking(move || {
        sereno_core::backup::snapshot::delete(&settings, &instance_id, &snapshot_id)
    })
    .await
    .map_err(|error| sereno_core::LauncherError::Internal {
        message: format!("删除快照任务异常: {error}"),
    })?
}

#[tauri::command]
#[specta::specta]
pub fn pin_content_snapshot(
    instance_id: String,
    snapshot_id: String,
    pinned: bool,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let settings = state.snapshot_settings()?;
    sereno_core::backup::snapshot::set_pinned(&settings, &instance_id, &snapshot_id, pinned)
}
