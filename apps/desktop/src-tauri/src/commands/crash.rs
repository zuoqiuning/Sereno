//! 诊断包导出命令。

use sereno_core::{LauncherError, Result};

use crate::state::AppState;

/// 导出诊断包（启动器日志 + 实例日志/崩溃报告），返回 zip 路径。
///
/// `instance_id` 为空时只导出启动器级日志。
#[tauri::command]
#[specta::specta]
pub async fn export_diagnostic(
    instance_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let settings = state.snapshot_settings()?;
    let state_dir = sereno_core::utils::paths::state_dir()?;
    let instance_dir = match instance_id {
        Some(id) => Some(sereno_core::utils::paths::instance_dir(&settings, &id)?),
        None => None,
    };

    let output_dir = state_dir.join("diagnostics");
    let state_for_task = state_dir.clone();
    let settings_for_task = settings.clone();

    // Java 信息用于诊断包里的 java.txt / manifest.json
    let mut java = None;
    if let Some(custom) = settings.java.custom_path.as_deref() {
        java = sereno_core::java::detector::probe(custom).await.ok();
    }
    if java.is_none() {
        java = sereno_core::java::detector::detect_all()
            .await
            .ok()
            .and_then(|list| list.into_iter().next());
    }

    let report = tokio::task::spawn_blocking(move || {
        sereno_core::crash::collector::export(
            &settings_for_task,
            instance_dir.as_deref(),
            &state_for_task,
            &output_dir,
            java.as_ref(),
        )
    })
    .await
    .map_err(|error| LauncherError::Internal {
        message: format!("诊断包生成任务异常: {error}"),
    })??;

    let path = report.path;
    // 导出后直接定位到文件，用户下一步就是把它发给开发者
    let _ = opener::reveal(&path);

    Ok(path.display().to_string())
}
