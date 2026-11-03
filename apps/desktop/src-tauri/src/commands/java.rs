//! Java 相关命令。

use crate::state::AppState;
use sereno_core::LauncherError;
use sereno_core::models::java::JavaCandidate;

/// 扫描并返回系统中所有可用的 Java 运行时。
#[tauri::command]
#[specta::specta]
pub async fn get_java_candidates() -> Result<Vec<JavaCandidate>, LauncherError> {
    sereno_core::java::detector::detect_all().await
}

/// 自动下载并安装 Zulu JRE（8 / 17 / 21）。
#[tauri::command]
#[specta::specta]
pub async fn install_java_runtime(
    major: u32,
    state: tauri::State<'_, AppState>,
) -> Result<JavaCandidate, LauncherError> {
    let settings = state.snapshot_settings()?;
    sereno_core::java::installer::install_jre(&settings, major).await
}
