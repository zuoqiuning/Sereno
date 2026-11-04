//! 增量更新（tauri-delta-updater）。
//!
//! ## 定位
//!
//! **增量优先、全量兜底**：用户点「检查更新」时先尝试增量补丁
//! （qbsdiff 二进制差分，带宽省 50–95%）；没有补丁、安全模式、
//! 或合并失败时返回 `has_update = true`，前端无缝走原有 Tauri
//! 全量更新（`commands/update.rs`，验签安装包路径**原样保留**）。
//!
//! ## 安全设计（全部来自 tauri-delta-updater 引擎）
//!
//! - 补丁与合并结果双重 SHA-256 校验，任何不匹配即拒绝；
//! - 连续 3 次失败进入**安全模式**（强制走全量），启动成功后解除；
//! - `.update.lock` 文件锁防多开；替换脚本带 30 秒启动超时自动回滚；
//! - 启动成功后由前端调用 `confirm_update_success` 清理备份与失败记录。
//!
//! ## 为什么只挂在「手动检查更新」上
//!
//! 启动时的后台检查保持原样（只弹卡片）：自动退出主进程属于需要
//! 用户知情的动作，v1 不在无人值守路径上做这件事。

use reqwest::Client;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri_delta_updater::{
    PlatformUpdater, UpdateLock, UpdateManifest, apply_patch, cleanup_previous_updates,
    download_patch_with_progress, get_failed_attempts, record_failed_attempt,
};

use sereno_core::{LauncherError, Result};

/// 增量检查结果（前端据此决定走增量提示还是全量卡片）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DeltaOutcome {
    /// `true` = 没有可用增量（无补丁 / 安全模式 / 合并失败），走全量更新。
    pub has_update: bool,
    /// `true` = 增量已合并并拉起替换脚本，应用即将退出重启。
    pub applied: bool,
    /// 面向用户的一句话说明。
    pub message: String,
}

/// 连续失败多少次进入安全模式（与引擎默认一致）。
const SAFE_MODE_ATTEMPTS: u32 = 3;

/// 从 Tauri 配置里读 updater 插件的 endpoints（与全量更新同源）。
fn updater_endpoints(app: &tauri::AppHandle) -> Vec<String> {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|value| value.get("endpoints"))
        .and_then(|endpoints| endpoints.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 从任一 endpoint 拉 latest.json 并解析为扩展清单。
async fn fetch_manifest(
    client: &Client,
    endpoints: &[String],
) -> std::result::Result<UpdateManifest, String> {
    let mut last_error = String::from("没有可用的更新源");
    for endpoint in endpoints {
        match client.get(endpoint).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => {
                    return serde_json::from_slice::<UpdateManifest>(&bytes)
                        .map_err(|e| format!("解析 latest.json 失败: {e}"));
                }
                Err(e) => last_error = format!("读取更新清单失败: {e}"),
            },
            Err(e) => last_error = format!("请求更新清单失败: {e}"),
        }
    }
    Err(last_error)
}

/// 增量产物：合并完成的暂存 exe + 替换脚本路径。
pub struct DeltaStaged {
    pub staged_exe: PathBuf,
    pub bat_path: PathBuf,
}

/// 增量尝试的核心流程（**不**拉起替换脚本、**不**退出进程——由命令层做，
/// 整个流程可以在测试里安全驱动）。
///
/// 返回 `None` = 无可用增量（走全量）；`Some` = 合并完成，待替换。
/// 失败时在本函数内记录失败计数（安全模式累计）。
pub async fn try_apply_delta(
    exe: &Path,
    current_version: &str,
    endpoints: &[String],
    client: &reqwest::Client,
    delta_client: &reqwest_delta::Client,
) -> std::result::Result<Option<DeltaStaged>, String> {
    let inner = try_apply_delta_inner(exe, current_version, endpoints, client, delta_client).await;
    match inner {
        Ok(result) => Ok(result),
        Err(error) => {
            // 记失败计数（安全模式累计）；调用方转告前端走全量
            if let Some(dir) = exe.parent() {
                let _ = record_failed_attempt(dir).await;
            }
            Err(error)
        }
    }
}

/// 增量尝试的主体（见 [`try_apply_delta`]）。
async fn try_apply_delta_inner(
    exe: &Path,
    current_version: &str,
    endpoints: &[String],
    client: &reqwest::Client,
    delta_client: &reqwest_delta::Client,
) -> std::result::Result<Option<DeltaStaged>, String> {
    let base_dir = exe.parent().ok_or_else(|| "无法定位安装目录".to_string())?;
    let exe_name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "无法定位当前可执行文件".to_string())?;

    // 1. 安全模式：连续 3 次失败后强制全量（防崩溃循环）
    let attempts = get_failed_attempts(base_dir).await;
    if attempts >= SAFE_MODE_ATTEMPTS {
        return Ok(None);
    }

    // 2. 拉取扩展清单（Tauri latest.json 超集）
    let manifest = fetch_manifest(client, endpoints).await?;
    if manifest.version == current_version {
        return Ok(None);
    }

    // 3. 找 from=当前版本 的补丁；没有就走全量
    let Some(patch) = manifest
        .patches
        .iter()
        .find(|p| p.from_version == current_version && p.to_version == manifest.version)
    else {
        return Ok(None);
    };

    // 4. 清理上次更新的残留文件（staged / patch / bat）
    cleanup_previous_updates(base_dir).await;

    // 5. 文件锁防多开（.update.lock，进程退出自动释放）
    let lock = UpdateLock::acquire(&base_dir.join(".update.lock"))
        .map_err(|e| format!("获取更新锁失败: {e}"))?;

    // 6. 磁盘预检：补丁 + 合并产物 + 当前 exe 的余量
    let exe_len = std::fs::metadata(exe).map(|m| m.len()).unwrap_or(0);
    tauri_delta_updater::check_disk_space(base_dir, exe_len.saturating_mul(2) + 32 * 1024 * 1024)
        .map_err(|e| format!("磁盘空间不足: {e}"))?;

    // 7. 下载补丁（带断点续传与超时重试）
    let patch_path = base_dir.join("sereno_delta.patch");
    let _ = std::fs::remove_file(&patch_path);
    download_patch_with_progress(delta_client, &patch.patch_url, &patch_path, |_, _| {})
        .await
        .map_err(|e| format!("下载增量补丁失败: {e}"))?;

    // 8. 合并：双 SHA-256 校验（补丁内容 + 合并产物），产出暂存 exe
    let staged_exe = base_dir.join(format!("sereno_staged_{exe_name}"));
    let _ = std::fs::remove_file(&staged_exe);
    apply_patch(
        exe,
        &patch_path,
        &staged_exe,
        &patch.patch_hash,
        &patch.target_hash,
    )
    .await
    .map_err(|e| format!("合并增量补丁失败: {e}"))?;

    // 9. 生成替换脚本（30 秒启动超时自动回滚；启动成功标记由
    //    confirm_update_success 写入后清理备份）
    let bat_path = PlatformUpdater::generate_updater(exe, &staged_exe)
        .map_err(|e| format!("生成替换脚本失败: {e}"))?;

    drop(lock);
    Ok(Some(DeltaStaged {
        staged_exe,
        bat_path,
    }))
}

fn http_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("构建 HTTP 客户端失败")
}

/// 检查并尝试应用增量更新。
///
/// 返回给前端的语义：
/// - `applied = true`：增量已合并、替换脚本已拉起，**应用马上退出**；
/// - `has_update = true`：没有可用增量（含安全模式与失败回退），
///   前端继续走 Tauri 官方全量更新；
/// - 两者皆 false：当前已是最新版本。
#[tauri::command]
#[specta::specta]
pub async fn check_and_apply_delta(
    current_version: Option<String>,
    app: tauri::AppHandle,
) -> Result<DeltaOutcome> {
    let current_version = current_version.unwrap_or_else(|| app.package_info().version.to_string());
    let endpoints = updater_endpoints(&app);
    let client = http_client();

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            return Ok(DeltaOutcome {
                has_update: true,
                applied: false,
                message: format!("无法定位当前程序: {e}"),
            });
        }
    };

    // 增量引擎的下载器基于 reqwest 0.11，与其类型系统对齐
    let delta_client = reqwest_delta::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("构建增量下载客户端失败");

    match try_apply_delta(&exe, &current_version, &endpoints, &client, &delta_client).await {
        Ok(Some(staged)) => {
            // 拉起替换脚本（等当前进程退出后改名 + 重启 + 等启动成功标记）
            if let Err(e) = PlatformUpdater::launch_updater(&staged.bat_path) {
                return Ok(DeltaOutcome {
                    has_update: true,
                    applied: false,
                    message: format!("拉起替换脚本失败: {e}"),
                });
            }
            // 强制退出主进程，把文件让给替换脚本
            app.exit(0);
            Ok(DeltaOutcome {
                has_update: false,
                applied: true,
                message: "增量更新已合并，应用即将重启".into(),
            })
        }
        Ok(None) => Ok(DeltaOutcome {
            has_update: true,
            applied: false,
            message: "没有可用的增量补丁".into(),
        }),
        Err(error) => {
            // 失败计数已在 try_apply_delta 内记录（安全模式累计）
            Ok(DeltaOutcome {
                has_update: true,
                applied: false,
                message: format!("增量更新失败，已回退全量：{error}"),
            })
        }
    }
}

/// 启动成功确认：清理失败计数与旧版备份，解除安全模式。
///
/// 前端在 `app.mount` 之后立刻调用——它告诉替换脚本
/// 「新版本界面已渲染成功」，脚本才会安心删除备份。
#[tauri::command]
#[specta::specta]
pub fn confirm_update_success() -> Result<()> {
    PlatformUpdater::confirm_startup_success().map_err(|e| LauncherError::Internal {
        message: e.to_string(),
    })
}
