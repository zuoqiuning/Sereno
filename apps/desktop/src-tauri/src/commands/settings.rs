//! 设置与应用级命令。

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use sereno_core::Result;
use sereno_core::models::settings::{AppSettings, LogLevel};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub state_dir: String,
    pub logs_dir: String,
    pub arch: String,
    pub os: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_app_info() -> Result<AppInfo> {
    let state_dir = sereno_core::utils::paths::state_dir()?;
    let logs_dir = sereno_core::utils::paths::logs_dir()?;

    Ok(AppInfo {
        version: sereno_core::APP_VERSION.to_string(),
        state_dir: sereno_core::utils::paths::redact_path(&state_dir),
        logs_dir: sereno_core::utils::paths::redact_path(&logs_dir),
        arch: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn log_from_frontend(
    level: LogLevel,
    message: String,
    fields: Option<std::collections::HashMap<String, String>>,
) -> Result<()> {
    let message = sereno_core::utils::redact::redact_line(&message);

    match level {
        LogLevel::Error => tracing::error!(target: "sereno_ui", fields = ?fields, "{message}"),
        LogLevel::Warn => tracing::warn!(target: "sereno_ui", fields = ?fields, "{message}"),
        LogLevel::Info | LogLevel::Debug => {
            tracing::info!(target: "sereno_ui", fields = ?fields, "{message}")
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings> {
    state.snapshot_settings()
}

#[tauri::command]
#[specta::specta]
pub fn save_settings(settings: AppSettings, state: tauri::State<'_, AppState>) -> Result<()> {
    state.update_settings(settings)
}

/// 前端预加载结束后调用：显示主窗口并关闭启动画面。
#[tauri::command]
#[specta::specta]
pub fn finish_startup(app: tauri::AppHandle) -> Result<()> {
    if let Some(main) = app.get_webview_window("main") {
        main.show()
            .map_err(|e| sereno_core::LauncherError::Internal {
                message: format!("显示主窗口失败: {e}"),
            })?;
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
    Ok(())
}

/// 打开系统原生的“选择文件夹”对话框。
#[tauri::command]
#[specta::specta]
pub async fn select_directory(app: tauri::AppHandle) -> Result<Option<String>> {
    let path = app
        .dialog()
        .file()
        .set_title("选择实例根目录")
        .blocking_pick_folder();

    Ok(path.map(|p| p.to_string()))
}
