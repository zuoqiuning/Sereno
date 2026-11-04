//! 启动命令：解析版本（本地优先）→ 选择 Java → 组装参数 → 拉起进程。

use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tauri::Emitter;

use sereno_core::launch::args::LaunchOptions;
use sereno_core::storage::instance::{read_metadata, write_metadata};
use sereno_core::{LauncherError, Result};

use crate::ipc::events::GameExited;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn launch_game(
    instance_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let mut metadata = read_metadata(&inst_dir)?;

    if metadata.status != "ready" {
        return Err(LauncherError::InvalidConfig {
            key: "status".into(),
            message: "实例未安装完成，请先完成下载或重新安装".into(),
        });
    }

    // 加载器实例的实际版本目录名由安装器决定（Forge/NeoForge/Fabric 各不相同），
    // 以磁盘上的 JSON 为准，找不到就明确报错，而不是悄悄启动原版。
    let version_id = sereno_core::loader::resolve_installed_version(&inst_dir, &metadata)
        .ok_or_else(|| LauncherError::Internal {
            message: format!("实例 {instance_id} 的加载器文件缺失，请重新安装加载器"),
        })?;

    let client = sereno_core::utils::http::build_client(&settings)?;
    let cached_manifest = state.manifest_cache.get();
    let version = sereno_core::version::detail::resolve_for_instance(
        &client,
        cached_manifest.as_ref(),
        &inst_dir,
        &version_id,
    )
    .await?;

    // 实例设置优先，缺省时回落到全局设置
    let java_path = metadata
        .java_path
        .as_deref()
        .or(settings.java.custom_path.as_deref());
    let java =
        sereno_core::java::detector::resolve(version.required_java_major(), java_path).await?;

    // 账户：微软正版用真实令牌、UUID 与 XUID；离线账户用档案里固化的 UUID；
    // 第三方登录用 Yggdrasil 的令牌，并注入 authlib-injector。
    let (username, uuid, access_token, user_type, xuid, authlib_javaagent) =
        if settings.account.kind == sereno_core::models::account::kind::MICROSOFT {
            let session = sereno_core::account::microsoft::session(&client).await?;
            (
                session.name,
                session.uuid,
                session.access_token,
                "msa".to_string(),
                session.xuid,
                None,
            )
        } else if settings.account.kind == sereno_core::models::account::kind::AUTHLIB {
            let profile = sereno_core::account::authlib::active(&settings).ok_or_else(|| {
                LauncherError::InvalidConfig {
                    key: "account".into(),
                    message: "第三方账号已失效，请重新登录".into(),
                }
            })?;
            let session = sereno_core::account::authlib::session(&client, &profile).await?;

            // 注入器不存在时自动下载（带 SHA-256 校验）。
            // 失败就让启动失败——没有注入器的第三方登录进不去游戏，
            // 与其启动后一堆报错，不如直接说清楚。
            let jar = sereno_core::account::authlib::ensure_injector(
                &client,
                settings.download.prefer_mirror,
            )
            .await?;
            let agent = sereno_core::account::authlib::javaagent_arg(&jar, &profile.api_root);

            (
                session.name,
                session.uuid,
                session.access_token,
                "mojang".to_string(),
                String::new(),
                Some(agent),
            )
        } else {
            let profile = sereno_core::account::offline::active(&settings);
            (
                profile.name,
                profile.uuid,
                "0".to_string(),
                "legacy".to_string(),
                String::new(),
                None,
            )
        };

    let config = sereno_core::launch::LaunchConfig {
        java_exe: java.path.clone(),
        instance_dir: inst_dir.clone(),
        version,
        options: LaunchOptions {
            username,
            uuid,
            access_token,
            user_type,
            xuid,
            memory_mb: metadata
                .memory_mb
                .unwrap_or(settings.java.default_memory_mb.unwrap_or(2048))
                .clamp(512, 32768),
            auto_chinese: settings.logging.auto_language_chinese,
            extra_jvm_args: metadata.jvm_args.clone(),
            authlib_javaagent,
        },
    };

    let prepared = sereno_core::launch::prepare(&config)?;

    // 真正拉起进程前才占锁，避免解析/网络失败时把实例“锁死”
    if !state.mark_running(&instance_id) {
        return Err(LauncherError::InstanceBusy {
            id: instance_id.clone(),
        });
    }

    let running = state.running_registry();
    let app_handle = app.clone();
    let id_for_exit = instance_id.clone();
    let dir_for_exit = inst_dir.clone();
    let started = Instant::now();

    let on_exit = move |exit_code: Option<i32>| {
        if let Ok(mut set) = running.lock() {
            set.remove(&id_for_exit);
        }
        add_playtime(&dir_for_exit, started.elapsed().as_secs());

        let event = GameExited {
            instance_id: id_for_exit,
            exit_code,
        };
        let _ = app_handle.emit("game-exited", event);
    };

    // 同步等一小段时间：秒崩会把错误直接抛给前端
    let result = sereno_core::launch::process::launch_game(
        &prepared.java_exe,
        prepared.args,
        &prepared.instance_dir,
        on_exit,
    )
    .await;

    if result.is_err() {
        state.unmark_running(&instance_id);
    }
    result?;

    metadata.last_played_at = Some(now_secs());
    write_metadata(&inst_dir, &metadata)?;

    Ok(())
}

/// 进程退出后把本次时长累加进实例元数据。
fn add_playtime(inst_dir: &Path, seconds: u64) {
    let Ok(mut metadata) = read_metadata(inst_dir) else {
        return;
    };
    metadata.playtime_secs = metadata
        .playtime_secs
        .saturating_add(seconds.min(u32::MAX as u64) as u32);
    let _ = write_metadata(inst_dir, &metadata);
}

fn now_secs() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
