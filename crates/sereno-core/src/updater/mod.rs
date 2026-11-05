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

/// 「只检查、不更新」的检查结果。
#[derive(Debug, Clone)]
pub struct DeltaCheck {
    /// 补丁目标版本（即清单版本）。
    pub to_version: String,
    /// 清单里的更新说明。
    pub notes: String,
}

/// 只检查增量更新是否可用，**不下载、不应用**。
///
/// 与 [`try_apply_delta_update`] 的分工：UI 的「检查更新」应该先调这里，
/// 把「发现新版本 + 更新说明」展示给用户，等用户确认后再调
/// [`try_apply_delta_update`]。绝不能在检查阶段就把应用退掉——
/// 那是 0.2.0 闪退事故（用户点检查 → 应用直接退出）的根因。
///
/// - `Ok(Some(_))`：有可用增量补丁，展示给用户并征求同意；
/// - `Ok(None)`：没有可用增量（无补丁 / 跨版本），走全量检查；
/// - `Err`：清单不可达或验签失败（fail-closed），走全量检查。
pub async fn check_delta_update(
    client: &reqwest::Client,
    manifest_url: &str,
    public_key: &str,
    current_version: &str,
) -> Result<Option<DeltaCheck>> {
    use reqwest::StatusCode;

    // 拉清单（fail-closed：任何失败都不假装"没有更新"，而是把错误抛给
    // 调用方决定降级策略）
    let response =
        client
            .get(manifest_url)
            .send()
            .await
            .map_err(|e| crate::LauncherError::Internal {
                message: format!("拉取更新清单失败: {e}"),
            })?;
    if response.status() != StatusCode::OK {
        return Err(crate::LauncherError::Internal {
            message: format!("拉取更新清单失败: HTTP {}", response.status()),
        });
    }
    let manifest_bytes = response
        .bytes()
        .await
        .map_err(|e| crate::LauncherError::Internal {
            message: format!("读取更新清单失败: {e}"),
        })?;

    // 签名与清单同目录同名，后缀 .sig（COS 布局：latest.json / latest.json.sig）
    let signature_url = format!("{manifest_url}.sig");
    let signature = client
        .get(&signature_url)
        .send()
        .await
        .map_err(|e| crate::LauncherError::Internal {
            message: format!("拉取清单签名失败: {e}"),
        })?
        .error_for_status()
        .map_err(|e| crate::LauncherError::Internal {
            message: format!("拉取清单签名失败: {e}"),
        })?
        .text()
        .await
        .map_err(|e| crate::LauncherError::Internal {
            message: format!("读取清单签名失败: {e}"),
        })?;

    // fail-closed：签名不过就当这次检查失败，绝不把不可信清单当"有更新"
    verify_manifest_signature(&manifest_bytes, signature.trim(), public_key).map_err(|e| {
        crate::LauncherError::Internal {
            message: format!("清单验签失败: {e}"),
        }
    })?;

    let manifest: UpdateManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| crate::LauncherError::Internal {
            message: format!("解析更新清单失败: {e}"),
        })?;

    let has_patch = manifest
        .find_patch(current_version, current_platform())
        .is_some();
    if !has_patch {
        return Ok(None);
    }
    Ok(Some(DeltaCheck {
        to_version: manifest.version.clone(),
        notes: manifest.notes.clone(),
    }))
}

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
    // 引擎 update() 的 future 持有非 Send 的内部进度回调，无法直接放进
    // tauri 的多线程运行时；且 debug 构建下该 future 体量会超出默认线程栈。
    // 解法：spawn_blocking 让出 tokio 工作线程 → 在大栈独立线程里起
    // 临时单线程运行时执行整个更新流程 → 外层 await 线程结果。
    let manifest_url = manifest_url.to_string();
    let public_key = public_key.to_string();
    let current_version = current_version.to_string();

    let outcome = tokio::task::spawn_blocking(
        move || -> std::result::Result<UpdateOutcome, tauri_delta_updater::DeltaError> {
            let updater = DeltaUpdater::new(
                UpdateConfig::new(&manifest_url, &public_key).current_version(&current_version),
            )
            .map_err(|e| {
                tauri_delta_updater::DeltaError::PatchApplyFailed(format!(
                    "初始化增量更新器失败: {e}"
                ))
            })?;

            std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("创建更新运行时失败")
                        .block_on(updater.update())
                })
                .expect("创建更新线程失败")
                .join()
                .unwrap_or_else(|_| {
                    Err(tauri_delta_updater::DeltaError::PatchApplyFailed(
                        "增量更新线程异常终止".into(),
                    ))
                })
        },
    )
    .await
    .map_err(|e| crate::LauncherError::Internal {
        message: format!("增量更新任务调度失败: {e}"),
    })?
    .map_err(|e| crate::LauncherError::Internal {
        message: format!("增量更新失败: {e}"),
    })?;

    match outcome {
        UpdateOutcome::Patched { staged } => {
            // 引擎文档要求：脚本已启动（auto_launch 默认 true），
            // 宿主必须让进程退出，否则脚本 60 秒后判定失败并回滚
            let _ = staged.script_path();
            tracing::info!("增量合并完成，已拉起替换脚本，宿主应尽快退出");
            Ok(true)
        }
        UpdateOutcome::FallbackToFullDownload { full_download_url } => {
            tracing::info!("没有可用增量补丁（全量地址: {full_download_url:?}），走全量更新");
            Ok(false)
        }
        UpdateOutcome::SafeMode => {
            tracing::warn!("增量更新连续失败，已进入安全模式，走全量更新");
            Ok(false)
        }
        // 引擎枚举标记了 #[non_exhaustive]，未来新增状态一律按"回退全量"处理
        other => {
            tracing::info!("增量更新结果 {other:?}，走全量更新");
            Ok(false)
        }
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
