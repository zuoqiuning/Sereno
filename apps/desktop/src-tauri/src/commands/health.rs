//! 实例健康检查命令。

use sereno_core::Result;
use sereno_core::models::health::HealthReport;

use crate::state::AppState;

/// 运行一次健康检查：启动预检 + Mod 依赖图 + 最近崩溃诊断。
#[tauri::command]
#[specta::specta]
pub async fn check_instance_health(
    instance_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<HealthReport> {
    let settings = state.snapshot_settings()?;
    let running = state.is_running(&instance_id);
    sereno_core::health::check(&settings, &instance_id, running).await
}
