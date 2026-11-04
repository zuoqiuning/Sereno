//! 启动器自身的更新命令。
//!
//! **本层只做三件事**：取参数、调用 core、把错误交给前端。
//! 下载地址由 core 内部从官方 Release 解析，前端**不能**传任意 URL。

use std::path::PathBuf;

use sereno_core::Result;
use sereno_core::version::update::{UpdateDownload, UpdateInfo};

use crate::state::AppState;

/// 检查是否有新版本。
#[tauri::command]
#[specta::specta]
pub async fn check_app_update(state: tauri::State<'_, AppState>) -> Result<UpdateInfo> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    sereno_core::version::update::check(&client).await
}

/// 下载更新并校验 SHA-256。
///
/// 校验不通过时**不会**返回可执行的路径——下载的文件会被留在磁盘上供排查，
/// 但前端拿到的 `path` 只在校验通过时才存在。
#[tauri::command]
#[specta::specta]
pub async fn download_app_update(
    info: UpdateInfo,
    state: tauri::State<'_, AppState>,
) -> Result<UpdateDownload> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    sereno_core::version::update::download(&client, &info).await
}

/// 启动安装程序。
///
/// 只接受**已经校验过**的安装包路径——路径来自 `download_app_update` 的返回，
/// 前端无法凭空构造一个通过校验的路径。
#[tauri::command]
#[specta::specta]
pub fn launch_app_updater(path: String) -> Result<()> {
    let path = PathBuf::from(path.trim());
    sereno_core::version::update::launch_installer(&path)
}
