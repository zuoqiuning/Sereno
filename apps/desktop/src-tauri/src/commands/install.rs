//! 安装命令：原版游戏安装。

use std::sync::Arc;

use tauri::ipc::Channel;

use sereno_core::Result;
use sereno_core::download::sink::ProgressSink;
use sereno_core::models::download::DownloadProgress;
use sereno_core::models::settings::AppSettings;
use sereno_core::storage::instance::{read_metadata, write_metadata};

use crate::ipc::progress::ChannelSink;
use crate::state::AppState;

/// 安装原版游戏，全程推送同一条整体进度。
///
/// **失败时把状态回退到 `created`**：否则实例会永久卡在 `installing`，
/// 界面上既不能启动也无法重试。
#[tauri::command]
#[specta::specta]
pub async fn install_game(
    instance_id: String,
    channel: Channel<DownloadProgress>,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy {
            id: instance_id.clone(),
        });
    }

    let settings = state.snapshot_settings()?;

    // 磁盘空间预检：安装至少需要 2 GB 可用空间（游戏本体 + 资源索引 +
    // 加载器库），不足时提前失败而不是下载到一半爆盘留下半成品
    {
        const MIN_INSTALL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
        let inst_root = sereno_core::utils::paths::instances_root(&settings)?;
        if let Some(available) = sereno_core::health::available_space(&inst_root)
            && available < MIN_INSTALL_BYTES
        {
            return Err(sereno_core::LauncherError::InvalidConfig {
                key: "disk".into(),
                message: format!(
                    "磁盘剩余空间不足：需要至少 2 GB，当前只有 {:.1} GB",
                    available as f64 / 1024.0 / 1024.0 / 1024.0
                ),
            });
        }
    }
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;

    let mut metadata = read_metadata(&inst_dir)?;
    if metadata.status == "ready" {
        return Ok(());
    }
    let game_version = metadata.game_version.clone();

    metadata.status = "installing".to_string();
    write_metadata(&inst_dir, &metadata)?;

    // 取消键与前端任务 ID 一致（任务页按实例 id 登记安装任务），
    // 这样「取消」按钮不需要额外参数就能命中。
    let cancel = state.new_cancel_token(&instance_id);
    let result = tokio::select! {
        _ = cancel.cancelled() => Err(sereno_core::LauncherError::Cancelled),
        result = run_install(&settings, &inst_dir, &game_version, channel) => result,
    };
    state.clear_cancel_token(&instance_id);

    metadata.status = if result.is_ok() {
        "ready".to_string()
    } else {
        "created".to_string()
    };
    write_metadata(&inst_dir, &metadata)?;

    result
}

/// 校验并修复实例：重新跑一遍安装计划，已存在且 SHA-1 正确的文件会被跳过。
///
/// 与 `install_game` 不同，这里不因 `status == "ready"` 提前返回；
/// 失败时保留原来的状态，避免把可用的实例降级成 `created`。
#[tauri::command]
#[specta::specta]
pub async fn repair_instance(
    instance_id: String,
    channel: Channel<DownloadProgress>,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy {
            id: instance_id.clone(),
        });
    }

    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let mut metadata = read_metadata(&inst_dir)?;
    let game_version = metadata.game_version.clone();
    let previous_status = metadata.status.clone();

    metadata.status = "installing".to_string();
    write_metadata(&inst_dir, &metadata)?;

    let cancel_key = format!("repair-{instance_id}");
    let cancel = state.new_cancel_token(&cancel_key);
    let result = tokio::select! {
        _ = cancel.cancelled() => Err(sereno_core::LauncherError::Cancelled),
        result = run_install(&settings, &inst_dir, &game_version, channel) => result,
    };
    state.clear_cancel_token(&cancel_key);

    metadata.status = if result.is_ok() {
        "ready".to_string()
    } else {
        previous_status
    };
    write_metadata(&inst_dir, &metadata)?;

    result
}

async fn run_install(
    settings: &AppSettings,
    inst_dir: &std::path::Path,
    game_version: &str,
    channel: Channel<DownloadProgress>,
) -> Result<()> {
    let sink: Arc<dyn ProgressSink> = Arc::new(ChannelSink(channel));
    sereno_core::install::game::install_base(settings, inst_dir, game_version, sink).await
}
