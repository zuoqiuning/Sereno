//! 增量更新调度器（基于 `tauri-delta-updater` 引擎）。
//!
//! 职责：安全链（安全模式 → 文件锁 → 磁盘预检）+ 补丁下载与合并 +
//! 替换脚本生成与拉起。清单的**获取与验签**由宿主工程负责
//! （公钥与更新源都在 Tauri 配置里），验签通过后才进到这里——
//! 这道分工保证了 `patches` 数组在进入调度前已经是可信数据。
//!
//! 返回语义：`Ok(true)` = 已合并并拉起替换脚本，宿主应退出主进程；
//! `Ok(false)` = 没有可用增量（无补丁 / 安全模式 / 失败回退），走全量；
//! `Err` = 引擎级异常（已记失败计数，同样走全量）。
//!
//! 测试入口：[`prepare_delta_update`] 做完安全链与合并但**不拉起**
//! 替换脚本，集成测试可以校验暂存产物而不真的替换可执行文件。

use std::path::{Path, PathBuf};

use tauri_delta_updater::{
    PlatformUpdater, UpdateLock, apply_patch, calculate_hash, cleanup_previous_updates,
    download_patch_with_progress, get_failed_attempts, record_failed_attempt,
};

use crate::Result;

// 宿主薄壳只需要依赖 sereno-core：清单模型与验签函数从这里 re-export，
// 避免桌面壳直接依赖引擎 crate（保持依赖单向：core → 引擎，壳 → core）。
#[allow(unused_imports)]
pub use tauri_delta_updater::{PatchInfo, UpdateManifest, verify_manifest_signature};

/// 连续失败多少次进入安全模式（与引擎默认一致）。
const SAFE_MODE_ATTEMPTS: u32 = 3;

/// 增量合并完成后、真正拉起替换脚本之前的中间产物。
pub struct StagedDelta {
    /// 合并产物（新版可执行文件的暂存副本），哈希已与清单核对。
    pub staged_exe: PathBuf,
    /// 生成的替换脚本（等待当前进程退出后替换并重启）。
    pub bat_path: PathBuf,
    /// 更新锁：持有期间其他实例不能并发进入更新流程。
    ///
    /// 放在返回值里而不是提前释放，是为了覆盖「合并完成 → 拉起脚本 →
    /// 进程退出」这段窗口；测试中 drop 返回值即释放。
    pub _lock: UpdateLock,
}

/// 执行增量更新的安全链与合并，产出暂存文件与替换脚本，但**不拉起**。
///
/// - `Ok(Some(staged))`：合并成功，随时可以拉起 `staged.bat_path`；
/// - `Ok(None)`：没有可用增量（安全模式 / 无补丁 / 下载失败），走全量；
/// - `Err`：合并产物哈希与清单不符（已记失败计数），走全量。
///
/// `manifest` 必须已经过验签（见宿主 `commands/updater.rs`）。
pub async fn prepare_delta_update(
    client: &reqwest::Client,
    current_exe_path: &Path,
    current_version: &str,
    manifest: &UpdateManifest,
) -> Result<Option<StagedDelta>> {
    let base_dir = current_exe_path
        .parent()
        .ok_or_else(|| crate::LauncherError::Internal {
            message: "无法定位安装目录".into(),
        })?;
    let exe_name = current_exe_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| crate::LauncherError::Internal {
            message: "无法定位当前可执行文件".into(),
        })?;

    // 1. 安全模式：连续 3 次失败后强制全量（防崩溃循环）
    let attempts = get_failed_attempts(base_dir).await;
    if attempts >= SAFE_MODE_ATTEMPTS {
        tracing::warn!("连续更新失败 {attempts} 次，进入安全模式，本次走全量更新");
        return Ok(None);
    }

    // 2. 已是最新 / 无可用补丁 → 全量路径
    if manifest.version == current_version {
        return Ok(None);
    }
    let Some(patch) = manifest.find_patch(current_version) else {
        return Ok(None);
    };

    // 3. 清理上次更新的残留（staged / patch / bat）
    cleanup_previous_updates(base_dir).await;

    // 4. 文件锁防多开（.update.lock，进程退出自动释放）
    let lock = UpdateLock::acquire(&base_dir.join(".update.lock")).map_err(|e| {
        crate::LauncherError::Internal {
            message: format!("获取更新锁失败: {e}"),
        }
    })?;

    // 5. 磁盘预检：补丁 + 合并产物 + 当前 exe 的余量
    let exe_len = std::fs::metadata(current_exe_path)
        .map(|m| m.len())
        .unwrap_or(0);
    tauri_delta_updater::check_disk_space(base_dir, exe_len.saturating_mul(2) + 32 * 1024 * 1024)
        .map_err(|e| crate::LauncherError::Internal {
        message: format!("磁盘空间不足: {e}"),
    })?;

    // 6. 下载补丁（断点续传 + 超时自动重连）
    let patch_path = base_dir.join("sereno_delta.patch");
    let _ = std::fs::remove_file(&patch_path);
    if let Err(e) =
        download_patch_with_progress(client, &patch.patch_url, &patch_path, |_, _| {}).await
    {
        tracing::error!("增量补丁下载失败: {e}");
        return Ok(None);
    }

    // 7. 合并：双 SHA-256 校验（补丁内容 + 合并产物），产出暂存 exe
    let staged_exe = base_dir.join(format!("sereno_staged_{exe_name}"));
    let _ = std::fs::remove_file(&staged_exe);
    if let Err(e) = apply_patch(
        current_exe_path,
        &patch_path,
        &staged_exe,
        &patch.patch_hash,
        &patch.target_hash,
    )
    .await
    {
        tracing::error!("增量补丁合并失败: {e}");
        let _ = record_failed_attempt(base_dir).await;
        return Err(crate::LauncherError::Internal {
            message: format!("增量补丁合并失败，已回退全量: {e}"),
        });
    }

    // 合并结果自证：暂存 exe 的哈希必须等于清单声明（防御引擎缺陷）
    let staged_hash =
        calculate_hash(&staged_exe)
            .await
            .map_err(|e| crate::LauncherError::Internal {
                message: format!("合并产物哈希计算失败: {e}"),
            })?;
    if staged_hash != patch.target_hash {
        let _ = record_failed_attempt(base_dir).await;
        let _ = std::fs::remove_file(&staged_exe);
        return Err(crate::LauncherError::Internal {
            message: "合并产物哈希与清单不符，已丢弃（防御性校验）".into(),
        });
    }

    // 8. 生成替换脚本（30 秒启动超时自动回滚；
    //    启动成功标记由 confirm_update_success 写入后清理备份）
    let bat_path =
        PlatformUpdater::generate_updater(current_exe_path, &staged_exe).map_err(|e| {
            crate::LauncherError::Internal {
                message: format!("生成替换脚本失败: {e}"),
            }
        })?;

    Ok(Some(StagedDelta {
        staged_exe,
        bat_path,
        _lock: lock,
    }))
}

/// 尝试应用增量更新：[`prepare_delta_update`] + 拉起替换脚本。
///
/// 拉起后宿主应退出主进程（交给脚本替换并重启）。
pub async fn try_apply_delta_update(
    client: &reqwest::Client,
    current_exe_path: &Path,
    current_version: &str,
    manifest: &UpdateManifest,
) -> Result<bool> {
    let Some(staged) =
        prepare_delta_update(client, current_exe_path, current_version, manifest).await?
    else {
        return Ok(false);
    };

    if let Err(e) = PlatformUpdater::launch_updater(&staged.bat_path) {
        tracing::error!("拉起替换脚本失败: {e}");
        return Ok(false);
    }

    Ok(true)
}

/// 启动成功确认：清理失败计数与旧版备份，解除安全模式。
pub fn confirm_startup_success() -> Result<()> {
    PlatformUpdater::confirm_startup_success().map_err(|e| crate::LauncherError::Internal {
        message: e.to_string(),
    })
}
