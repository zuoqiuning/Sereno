//! 增量更新调度器（基于 `tauri-delta-updater` 引擎的高阶 API [`DeltaUpdater`]）。
//!
//! 分工：清单的**获取、验签、补丁匹配、下载、合并、替换脚本拉起**
//! 全部由引擎一次调用完成（含安全模式、文件锁、清理残留、失败计数）；
//! 宿主只负责提供「清单地址 + 固化公钥 + 当前版本」，并把结果映射成
//! 全量回退语义。
//!
//! 返回语义：`Ok(true)` = 已合并并拉起替换脚本，宿主应退出主进程；
//! `Ok(false)` = 没有可用增量（无补丁 / 安全模式），走全量；
//! `Err` = 引擎级异常（引擎内部已记失败计数，连续失败自动进安全模式）。

use crate::Result;

// 宿主薄壳与发布工具只需要依赖 sereno-core，全部从这里 re-export，
// 避免它们直接依赖引擎 crate（保持依赖单向：core → 引擎，壳 → core）。
#[allow(unused_imports)]
pub use tauri_delta_updater::{
    DeltaUpdater, PatchInfo, UpdateConfig, UpdateManifest, UpdateOutcome, calculate_hash,
    create_patch, current_platform, verify_manifest_signature,
};

/// 尝试执行一次完整的增量更新。
///
/// - `Ok(true)`：已合并并拉起替换脚本，宿主应尽快退出主进程（交给脚本替换重启）；
/// - `Ok(false)`：没有可用增量（无补丁 / 跨版本 / 安全模式），走全量更新；
/// - `Err`：引擎级异常，调用方可自行决定提示或静默回退全量。
pub async fn try_apply_delta_update(
    manifest_url: &str,
    public_key: &str,
    current_version: &str,
) -> Result<bool> {
    let updater = DeltaUpdater::new(
        UpdateConfig::new(manifest_url, public_key).current_version(current_version),
    );

    match updater.update().await {
        Ok(UpdateOutcome::Patched { manifest, .. }) => {
            tracing::info!(
                "增量合并完成（新版本 {}），已拉起替换脚本",
                manifest.version
            );
            Ok(true)
        }
        Ok(UpdateOutcome::FallbackToFullDownload { full_download_url }) => {
            tracing::info!("没有可用增量补丁（全量地址: {full_download_url:?}），走全量更新");
            Ok(false)
        }
        Ok(UpdateOutcome::SafeMode) => {
            tracing::warn!("增量更新连续失败，已进入安全模式，走全量更新");
            Ok(false)
        }
        Err(e) => Err(crate::LauncherError::Internal {
            message: format!("增量更新失败: {e}"),
        }),
    }
}

/// 启动成功确认：清理失败计数与旧版备份，解除安全模式。
///
/// 前端在 `app.mount` 之后立刻调用——告诉替换脚本「新版界面已渲染成功」，
/// 脚本才会删除备份；没有这一步，替换脚本会在超时后自动回滚。
pub fn confirm_startup_success() -> Result<()> {
    tauri_delta_updater::PlatformUpdater::confirm_startup_success().map_err(|e| {
        crate::LauncherError::Internal {
            message: e.to_string(),
        }
    })
}
