//! 启动器自身的更新命令。
//!
//! ## 为什么用官方的 `tauri-plugin-updater`
//!
//! 上一版用的是自研的「下载 + 校验 `SHA256SUMS.txt`」方案。它能防住
//! 「传输途中被篡改」，但防不住**发布源本身被投毒**——
//! `SHA256SUMS.txt` 和安装包躺在同一个 Release 里，攻击者能改哈希表，
//! 也能改安装包，两边对上就通过了。
//!
//! 官方 updater 用的是 **minisign 数字签名**：发布端用私钥签更新产物，
//! 客户端用**内置的公钥**验签。私钥不出现在任何分发渠道里，
//! 因此即使镜像站、CDN、甚至 GitHub 账号全被拿下，
//! 攻击者也造不出一个能通过验签的更新包。
//!
//! 这补上了《发布与防投毒》里「分发信任」那一层的真正缺口。
//!
//! ## 分层
//!
//! 按《项目结构与模块边界》的四层原则：
//!
//! - **网络与验签**由 `tauri-plugin-updater` 在 Rust 侧完成，
//!   前端**不参与**、也**不能**指定下载地址；
//! - 本文件是**命令层**，只做「取参数 → 调插件 → 转可序列化结果」；
//! - 进度通过 `ipc::events::UpdateProgress` 事件回传。
//!
//! ## 安全边界
//!
//! - 更新地址只来自 `tauri.conf.json` 的 `plugins.updater.endpoints`，
//!   **不接受前端传入任何 URL**；
//! - 下载的产物必须通过 minisign 验签，否则插件直接拒绝安装；
//! - 开启了 `requireSignedVersion`：签名覆盖的版本号必须与清单一致，
//!   防止「用旧包的合法签名 + 伪造的高版本号」把用户降级。

use serde::{Deserialize, Serialize};
use sereno_core::{LauncherError, Result};
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;

use crate::ipc::events;
use crate::state::AppState;

/// 把插件的错误转成统一的 `LauncherError`。
fn update_error(context: &str, error: impl std::fmt::Display) -> LauncherError {
    LauncherError::UpdateFailed {
        message: format!("{context}：{error}"),
    }
}

/// 判断这个错误是不是「端点上没有可用版本」。
///
/// 仓库还没发过任何 Release 时，两个端点都会返回 404，
/// 插件把它归为 `ReleaseNotFound`。这**不是故障**——
/// 用户点「检查更新」应当看到「已是最新版本」，
/// 而不是一条红色的「检查更新失败」。
fn is_no_release(error: &tauri_plugin_updater::Error) -> bool {
    matches!(error, tauri_plugin_updater::Error::ReleaseNotFound)
}

/// 检查更新的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    /// 是否有新版本。
    pub available: bool,
    /// 最新版本号（无更新时等于当前版本）。
    pub latest_version: String,
    /// 当前版本号。
    pub current_version: String,
    /// 更新说明（可能为空）。
    pub notes: String,
    /// 发布日期（RFC 3339，可能为空）。
    pub pub_date: Option<String>,
}

/// 检查是否有新版本。
///
/// **不产生任何下载**——只是读一次清单。
///
/// 结果会写进设置（`update.latest_version` / `update.last_checked_at`）：
/// 只在检查那一刻弹一次 Toast 的话，错过就没了；
/// 记下来才能在用户下次打开「更新」页时仍然看得到「有新版本」。
#[tauri::command]
#[specta::specta]
pub async fn check_app_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<AppUpdateInfo> {
    let current = app.package_info().version.to_string();

    let updater = app
        .updater()
        .map_err(|e| update_error("初始化更新通道失败", e))?;

    let update = match updater.check().await {
        Ok(update) => update,
        // 端点上没有可用版本（仓库还没发过 Release）——
        // 这是正常状态，不是故障。
        Err(e) if is_no_release(&e) => None,
        Err(e) => return Err(update_error("检查更新失败", e)),
    };

    let info = match &update {
        Some(update) => AppUpdateInfo {
            available: true,
            latest_version: update.version.clone(),
            current_version: current,
            notes: update.body.clone().unwrap_or_default(),
            pub_date: update.date.map(|date| date.to_string()),
        },
        None => AppUpdateInfo {
            available: false,
            latest_version: current.clone(),
            current_version: current,
            notes: String::new(),
            pub_date: None,
        },
    };

    // 记录本次检查结果。写设置失败不该让「检查更新」整体失败——
    // 用户已经拿到了答案，存不存下来只是下次打开时看不看得见。
    let mut settings = state.snapshot_settings()?;
    settings.update.latest_version = Some(info.latest_version.clone());
    settings.update.last_checked_at = Some(now_seconds());
    let _ = state.update_settings(settings);

    Ok(info)
}

/// 当前的 Unix 时间戳（秒）。
fn now_seconds() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// 下载更新、验签并安装。
///
/// **Windows 上插件会在安装器启动后自动退出应用**，因此这个命令
/// 大概率不会正常返回到前端——这是预期行为，不是错误。
///
/// 进度通过 `UpdateProgress` 事件广播（而不是 Channel）：
/// 安装完成后进程可能立刻退出，Channel 的发送端会随命令一起被 drop，
/// 最后一个进度事件可能来不及送达。事件是广播式的，不受此影响。
#[tauri::command]
#[specta::specta]
pub async fn install_app_update(app: tauri::AppHandle) -> Result<()> {
    let updater = app
        .updater()
        .map_err(|e| update_error("初始化更新通道失败", e))?;

    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => {
            return Err(LauncherError::UpdateFailed {
                message: "当前已是最新版本".into(),
            });
        }
        Err(e) if is_no_release(&e) => {
            return Err(LauncherError::UpdateFailed {
                message: "当前已是最新版本".into(),
            });
        }
        Err(e) => return Err(update_error("检查更新失败", e)),
    };

    // 已下载字节数在闭包之间共享：插件的回调是 `FnMut`，要能累加。
    let downloaded = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));

    let sink = app.clone();
    let counter = std::sync::Arc::clone(&downloaded);

    update
        .download_and_install(
            move |chunk_length, content_length| {
                let total = counter
                    .fetch_add(chunk_length as u64, std::sync::atomic::Ordering::Relaxed)
                    + chunk_length as u64;

                let size = content_length.unwrap_or(0);
                let percent = if size > 0 {
                    ((total.min(size) as f64 / size as f64) * 100.0).round() as u8
                } else {
                    0
                };

                let _ = sink.emit(
                    "update-progress",
                    events::UpdateProgress {
                        downloaded: total,
                        total: size,
                        percent,
                    },
                );
            },
            || {
                tracing::info!("更新包下载完成，开始验签与安装");
            },
        )
        .await
        .map_err(|e| update_error("下载或安装更新失败", e))
}

/// 重启应用（安装完成后调用）。
///
/// 用的是 Tauri 核心的 `request_restart()`——
/// 它会在退出时把「需要重启」标记置上，由运行循环在退出后重新拉起进程。
#[tauri::command]
#[specta::specta]
pub fn relaunch_app(app: tauri::AppHandle) -> Result<()> {
    app.request_restart();
    Ok(())
}

/// 更新页需要的完整状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// 当前版本号。
    pub current_version: String,
    /// 是否在启动时自动检查。
    pub auto_check: bool,
    /// 上次检查到的新版本号（没有则为空）。
    pub latest_version: Option<String>,
    /// 上次检查时间戳（秒）。
    #[specta(type = specta_typescript::Number)]
    pub last_checked_at: Option<u64>,
}

/// 读取更新页的状态。
///
/// **不发起网络请求**——只是把设置里的记录读出来。
/// 页面据此显示「上次检查于……」与已知的最新版本。
#[tauri::command]
#[specta::specta]
pub fn get_update_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<UpdateStatus> {
    let settings = state.snapshot_settings()?;
    Ok(UpdateStatus {
        current_version: app.package_info().version.to_string(),
        auto_check: settings.update.auto_check,
        latest_version: settings.update.latest_version.clone(),
        last_checked_at: settings.update.last_checked_at,
    })
}

/// 设置「启动时自动检查更新」。
///
/// 关闭后启动器不再自动发起检查请求，用户只能手动检查。
#[tauri::command]
#[specta::specta]
pub fn set_update_auto_check(auto_check: bool, state: tauri::State<'_, AppState>) -> Result<()> {
    let mut settings = state.snapshot_settings()?;
    settings.update.auto_check = auto_check;
    state.update_settings(settings)
}
