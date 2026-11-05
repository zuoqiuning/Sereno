//! 增量更新相关的 Tauri 命令（薄壳）。
//!
//! 全部流程（拉清单 → minisign 验签 → 匹配补丁 → 下载 → 校验合并 →
//! 拉起替换脚本）由 `sereno_core::updater` 封装的引擎高阶 API 完成。
//! 这里只做：确定当前版本号、调用调度器、把结果映射成前端语义。
//! 增量通道的任何失败都不会阻塞用户——一律静默回退到全量更新。

use serde::{Deserialize, Serialize};
use sereno_core::Result;
use sereno_core::updater::{check_delta_update as check_delta, try_apply_delta_update};

use crate::state::AppState;

/// 清单验签公钥，与 `tauri.conf.json` 的 `plugins.updater.pubkey` 完全一致。
///
/// 注意：这是 minisign **公钥本体**（base64），不是 `.sig` 文件的内容——
/// `.sig` 是随清单下发的签名，公钥必须内置在代码里，绝不能从网络拉取。
const MANIFEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDAxRDZDM0IwQTI1Q0JGMDgKUldRSXYxeWlzTVBXQWRweXBvQ1B4eittUDd1OVExemFqZ29UQTdJeUpJOFlZNWgrYmNobFl4REMK";

/// 增量清单的更新源（与 `tauri.conf.json` `plugins.updater.endpoints` 的主端点一致）。
const MANIFEST_BASE_URL: &str = "https://sereno-updates-1316381240.cos.ap-beijing.myqcloud.com";

/// 增量更新检查的结果。
///
/// 前端语义：`applied = true` 时应用即将重启（后端已拉起替换脚本）；
/// 否则按 `has_update` 决定是否继续走全量更新流程。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeltaOutcome {
    /// `true` = 没有可用增量（无补丁 / 安全模式 / 合并失败），走全量更新。
    pub has_update: bool,
    /// `true` = 增量已合并并拉起替换脚本，应用即将退出重启。
    pub applied: bool,
    /// 面向用户的一句话说明。
    pub message: String,
}

/// 「增量不可用，回退全量」的统一出口。
fn delta_fallback(message: impl Into<String>) -> DeltaOutcome {
    DeltaOutcome {
        has_update: true,
        applied: false,
        message: message.into(),
    }
}

/// 「增量检查」结果：只报告有没有补丁，**不执行任何更新动作**。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeltaCheckOutcome {
    /// `true` = 有可用增量补丁（toVersion/notes 有效），应征求用户同意后再应用。
    pub available: bool,
    /// 补丁目标版本（available 时有效）。
    pub to_version: String,
    /// 清单里的更新说明（available 时有效）。
    pub notes: String,
}

/// 只检查增量更新是否可用（不下载、不应用）。
#[tauri::command]
#[specta::specta]
pub async fn check_delta_update(
    current_version: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<DeltaCheckOutcome> {
    let settings = state.snapshot_settings()?;
    let current_version = current_version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| app.package_info().version.to_string());

    let unavailable = DeltaCheckOutcome {
        available: false,
        to_version: String::new(),
        notes: String::new(),
    };

    // 任何失败（网络 / 验签 / 解析）都降级为"没有可用增量"，
    // 让调用方继续走全量检查通道，绝不阻塞用户。
    let client = match sereno_core::utils::http::build_client(&settings) {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("构建更新用 HTTP 客户端失败: {e}");
            return Ok(unavailable);
        }
    };
    let manifest_url = format!("{MANIFEST_BASE_URL}/latest.json");
    match check_delta(&client, &manifest_url, MANIFEST_PUBKEY, &current_version).await {
        Ok(Some(check)) => Ok(DeltaCheckOutcome {
            available: true,
            to_version: check.to_version,
            notes: check.notes,
        }),
        Ok(None) => Ok(unavailable),
        Err(e) => {
            tracing::warn!("增量检查失败，回退全量检查: {e}");
            Ok(unavailable)
        }
    }
}

/// 应用增量更新：下载补丁 → 校验合并 → 拉起替换脚本 → 退出重启。
///
/// **必须先经用户确认**（关于页的「检查更新」先用 check_delta_update
/// 探测并弹确认框），直接调用等于未经同意就把应用退掉——0.2.0 闪退事故的教训。
#[tauri::command]
#[specta::specta]
pub async fn apply_delta_update(
    current_version: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<DeltaOutcome> {
    let settings = state.snapshot_settings()?;
    let _ = &settings; // 读取即校验设置锁可用（安全检查项：更新命令不覆盖账户状态）

    let current_version = current_version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| app.package_info().version.to_string());

    let manifest_url = format!("{MANIFEST_BASE_URL}/latest.json");
    match try_apply_delta_update(&manifest_url, MANIFEST_PUBKEY, &current_version).await {
        Ok(true) => {
            // 给前端留出渲染「增量更新完成」toast 的时间再退出——
            // 替换脚本在等本进程退出，退出是更新能够继续的前提。
            tracing::info!("增量合并完成，1.5 秒后退出以启动替换脚本");
            tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                std::process::exit(0);
            });
            Ok(DeltaOutcome {
                has_update: true,
                applied: true,
                message: "增量更新完成，应用即将重启".into(),
            })
        }
        Ok(false) => Ok(delta_fallback("没有可用的增量补丁，已回退全量更新")),
        Err(e) => {
            // 增量通道异常（清单不可达 / 验签不过 / 磁盘不足…）不阻塞用户：
            // 记录日志后静默回退全量。引擎内部已记失败计数，
            // 连续失败会自动进入安全模式，之后连下载都不会再尝试。
            tracing::warn!("增量更新失败，回退全量: {e}");
            Ok(delta_fallback("增量通道不可用，已回退全量更新"))
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn confirm_update_success() -> Result<()> {
    sereno_core::updater::confirm_startup_success()
}
