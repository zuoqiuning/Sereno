//! 世界（存档）备份命令。

use crate::state::AppState;
use sereno_core::Result;
use sereno_core::models::backup::WorldBackup;

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
