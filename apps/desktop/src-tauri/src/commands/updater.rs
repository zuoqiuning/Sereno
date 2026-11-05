//! 增量更新相关的 Tauri 命令（薄壳）。
//!
//! 分工：这里只做「清单获取 + 验签」（公钥与更新源属于宿主配置），
//! 安全链、下载合并、替换脚本全部在 `sereno_core::updater` 里，
//! 那一层有完整的集成测试覆盖（见 `crates/sereno-core/tests/delta_flow.rs`）。

use serde::{Deserialize, Serialize};
use sereno_core::updater::{UpdateManifest, try_apply_delta_update, verify_manifest_signature};
use sereno_core::utils::http::build_client;
use sereno_core::{LauncherError, Result};

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

#[tauri::command]
#[specta::specta]
pub async fn check_and_apply_delta(
    current_version: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<DeltaOutcome> {
    let settings = state.snapshot_settings()?;

    // 前端一般不传版本号，以应用自身的版本为准（防伪造/防漂移）；
    // 参数保留 Option 形态是为了兼容启动脚本等特殊调用方。
    let current_version = current_version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| app.package_info().version.to_string());

    // 复用全局的客户端构建：代理、UA、超时与主下载链路保持一致
    let client = build_client(&settings)?;

    // 1. 下载增量清单。拉不到清单不算错误——增量通道不可用时静默走全量，
    //    全量通道（tauri-plugin-updater）有自己的备用端点与验签。
    let manifest_url = format!("{MANIFEST_BASE_URL}/latest.json");
    let manifest_bytes = match client.get(&manifest_url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("读取增量清单失败，回退全量: {e}");
                return Ok(delta_fallback("增量清单不可用，已回退全量更新"));
            }
        },
        Ok(resp) => {
            tracing::warn!("增量清单 HTTP {}，回退全量", resp.status());
            return Ok(delta_fallback("增量清单不可用，已回退全量更新"));
        }
        Err(e) => {
            tracing::warn!("下载增量清单失败，回退全量: {e}");
            return Ok(delta_fallback("增量清单不可用，已回退全量更新"));
        }
    };

    // 2. 验签（硬门槛，fail-closed）：签名下载不到 / 验签不过 → 拒绝增量。
    //    这里绝不能因为「签名拉不到」就放行未验签的清单。
    let sig_url = format!("{MANIFEST_BASE_URL}/latest.json.sig");
    let signature = match client.get(&sig_url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!("读取增量签名失败，拒绝增量并回退全量: {e}");
                return Ok(delta_fallback("增量更新签名不可用，已回退全量更新"));
            }
        },
        Ok(resp) => {
            tracing::warn!("增量签名 HTTP {}，拒绝增量并回退全量", resp.status());
            return Ok(delta_fallback("增量更新签名不可用，已回退全量更新"));
        }
        Err(e) => {
            tracing::warn!("下载增量签名失败，拒绝增量并回退全量: {e}");
            return Ok(delta_fallback("增量更新签名不可用，已回退全量更新"));
        }
    };
    if let Err(e) = verify_manifest_signature(&manifest_bytes, &signature, MANIFEST_PUBKEY) {
        tracing::error!("增量清单验签失败，拒绝增量并回退全量: {e}");
        return Ok(delta_fallback("增量更新验签失败，已回退全量更新"));
    }

    // 3. 验签通过后才解析清单——此时 patches 数组是可信数据
    let manifest: UpdateManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| LauncherError::Parse {
            message: format!("解析增量清单失败: {e}"),
        })?;

    if manifest.version == current_version {
        return Ok(DeltaOutcome {
            has_update: false,
            applied: false,
            message: "已是最新版本".into(),
        });
    }

    // 4. 走 core 调度器：安全模式 → 锁 → 磁盘预检 → 下载 → 双哈希合并 → 拉起
    let current_exe = std::env::current_exe().map_err(|e| LauncherError::Io {
        message: format!("定位当前可执行文件失败: {e}"),
    })?;
    let applied =
        try_apply_delta_update(&client, &current_exe, &current_version, &manifest).await?;

    if applied {
        // 给前端留出渲染「增量更新完成」toast 的时间再退出——
        // 替换脚本在等本进程退出，退出是更新能够继续的前提。
        tracing::info!("增量合并完成，1.5 秒后退出以启动替换脚本");
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            std::process::exit(0);
        });
        return Ok(DeltaOutcome {
            has_update: true,
            applied: true,
            message: "增量更新完成，应用即将重启".into(),
        });
    }

    Ok(delta_fallback("没有可用的增量补丁，已回退全量更新"))
}

#[tauri::command]
#[specta::specta]
pub fn confirm_update_success() -> Result<()> {
    sereno_core::updater::confirm_startup_success()
}
