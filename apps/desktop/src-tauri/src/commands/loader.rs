use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sereno_core::Result;
use sereno_core::storage::instance::{read_metadata, write_metadata};
use sereno_core::utils::http::MAX_METADATA_BYTES;
use sereno_core::utils::ttl_cache::TtlCache;
use std::sync::OnceLock;
use std::time::Duration;

/// 加载器版本列表缓存。
///
/// 打开安装面板会把 Forge / NeoForge / Fabric 三份列表一起拉下来，
/// Fabric 的响应接近 400KB。同一游戏版本在 10 分钟内重复打开时直接命中缓存。
static VERSION_CACHE: OnceLock<TtlCache<String, Vec<String>>> = OnceLock::new();

fn version_cache() -> &'static TtlCache<String, Vec<String>> {
    VERSION_CACHE.get_or_init(|| TtlCache::new(Duration::from_secs(600), 32))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LoaderType {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
    OptiFine,
}

#[tauri::command]
#[specta::specta]
pub async fn get_loader_versions(
    mc_version: String,
    loader_type: LoaderType,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>> {
    if matches!(loader_type, LoaderType::Vanilla) {
        return Ok(Vec::new());
    }

    let settings = state.snapshot_settings()?;
    let prefer_mirror = settings.download.prefer_mirror;
    let cache_key = format!("{loader_type:?}:{mc_version}");

    if let Some(cached) = version_cache().get(&cache_key) {
        return Ok(cached);
    }

    let client = sereno_core::utils::http::build_client(&settings)?;
    let mut versions = Vec::new();

    match loader_type {
        LoaderType::Vanilla => unreachable!("原版在上方已经提前返回"),
        LoaderType::Fabric => {
            let official = format!("https://meta.fabricmc.net/v2/versions/loader/{mc_version}");
            let mirror = format!(
                "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{mc_version}"
            );
            let urls = if prefer_mirror {
                [mirror, official]
            } else {
                [official, mirror]
            };
            for url in urls {
                let Ok(resp) = client.get(&url).send().await else {
                    continue;
                };
                if !resp.status().is_success() {
                    continue;
                }
                let Ok(json) =
                    sereno_core::utils::http::read_json_limited::<Value>(resp, MAX_METADATA_BYTES)
                        .await
                else {
                    continue;
                };
                if let Some(arr) = json.as_array() {
                    for item in arr {
                        if let Some(v) = item
                            .get("loader")
                            .and_then(|l| l.get("version"))
                            .and_then(|v| v.as_str())
                        {
                            versions.push(v.to_string());
                        }
                    }
                }
                if !versions.is_empty() {
                    break;
                }
            }
        }
        LoaderType::Quilt => {
            let url = format!("https://meta.quiltmc.org/v3/versions/loader/{mc_version}");
            if let Ok(resp) = client.get(&url).send().await
                && resp.status().is_success()
                && let Ok(json) =
                    sereno_core::utils::http::read_json_limited::<Value>(resp, MAX_METADATA_BYTES)
                        .await
                && let Some(arr) = json.as_array()
            {
                for item in arr {
                    if let Some(version) = item
                        .get("loader")
                        .and_then(|loader| loader.get("version"))
                        .and_then(|value| value.as_str())
                    {
                        versions.push(version.to_string());
                    }
                }
            }
        }
        LoaderType::Forge => {
            let url = format!(
                "https://bmclapi2.bangbang93.com/forge/minecraft/{}",
                mc_version
            );
            if let Ok(resp) = client.get(&url).send().await
                && let Ok(json) =
                    sereno_core::utils::http::read_json_limited::<Value>(resp, MAX_METADATA_BYTES)
                        .await
                && let Some(arr) = json.as_array()
            {
                for item in arr {
                    if let Some(v) = item.get("version").and_then(|v| v.as_str()) {
                        let ver = v.split('-').nth(1).unwrap_or(v);
                        versions.push(ver.to_string());
                    }
                }
            }
            versions.reverse();
        }
        LoaderType::NeoForge => {
            versions =
                sereno_core::loader::neoforge::list_versions(&client, prefer_mirror, &mc_version)
                    .await?;
        }
        LoaderType::OptiFine => {
            // OptiFine 的版本标签形如 `1.20.1 HD_U I6`——
            // 安装时要用完整标签才能拼出下载地址，只给补丁号不够。
            versions = sereno_core::loader::optifine::list_versions(&client, &mc_version)
                .await?
                .into_iter()
                .map(|version| version.label)
                .collect();
        }
    }

    if !versions.is_empty() {
        version_cache().insert(cache_key, versions.clone());
    }
    Ok(versions)
}

#[tauri::command]
#[specta::specta]
pub async fn install_loader(
    instance_id: String,
    loader_type: String,
    loader_version: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let loader_type = loader_type.trim().to_lowercase();
    if !matches!(
        loader_type.as_str(),
        "vanilla" | "fabric" | "forge" | "neoforge" | "quilt" | "optifine"
    ) {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "loaderType".into(),
            message: format!("未知的加载器类型: {loader_type}"),
        });
    }

    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let mut metadata = read_metadata(&inst_dir)?;

    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy {
            id: instance_id.clone(),
        });
    }

    if loader_type == "vanilla" {
        return Ok(());
    }

    let spec_ver = loader_version.as_deref().filter(|v| *v != "latest");

    // OptiFine 走的是「下载安装器 → 无头跑一次 → 收集产物」的特殊流程，
    // 与其它加载器的安装器协议完全不同，单独处理。
    if loader_type == "optifine" {
        // 取消键与前端任务 ID 对齐（任务页按 `loader-<实例 id>` 登记）。
        let cancel_key = format!("loader-{instance_id}");
        let cancel = state.new_cancel_token(&cancel_key);
        let result = tokio::select! {
            _ = cancel.cancelled() => Err(sereno_core::LauncherError::Cancelled),
            result = sereno_core::loader::install::install_optifine_into(
                &settings, &inst_dir, &metadata, spec_ver,
            ) => result,
        };
        state.clear_cancel_token(&cancel_key);

        let info = result?;
        metadata.loader = info;
        write_metadata(&inst_dir, &metadata)?;

        if sereno_core::loader::resolve_installed_version(&inst_dir, &metadata).is_none() {
            return Err(sereno_core::LauncherError::LoaderInstallFailed {
                message: "OptiFine 安装器执行完成，但没有找到可启动的版本 JSON".into(),
            });
        }
        return Ok(());
    }

    // 取消键与前端任务 ID 对齐（任务页按 `loader-<实例 id>` 登记）。
    let cancel_key = format!("loader-{instance_id}");
    let cancel = state.new_cancel_token(&cancel_key);
    let result = tokio::select! {
        _ = cancel.cancelled() => Err(sereno_core::LauncherError::Cancelled),
        result = sereno_core::loader::install::install_loader_into(
            &settings,
            &inst_dir,
            &metadata,
            &loader_type,
            spec_ver,
        ) => result,
    };
    state.clear_cancel_token(&cancel_key);

    let loader_info = result?;

    metadata.loader = loader_info;
    write_metadata(&inst_dir, &metadata)?;

    // 安装器写出的版本目录名可能与我们推测的不同，以磁盘为准做一次校验
    if sereno_core::loader::resolve_installed_version(&inst_dir, &metadata).is_none() {
        return Err(sereno_core::LauncherError::LoaderInstallFailed {
            message: format!("{loader_type} 安装器执行完成，但没有找到可启动的版本 JSON"),
        });
    }

    Ok(())
}
