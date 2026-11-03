//! 整合包命令：Modrinth 整合包（`.mrpack`）一键安装。
//!
//! 本层只做三件事：取参数、调用 core、把错误交给前端。

use std::sync::Arc;

use tauri::ipc::Channel;

use sereno_core::Result;
use sereno_core::download::sink::ProgressSink;
use sereno_core::models::download::DownloadProgress;
use sereno_core::models::modpack::ModpackInfo;
use sereno_core::storage::instance::{read_metadata, write_metadata};

use crate::ipc::progress::ChannelSink;
use crate::state::AppState;

/// 整合包概览：安装前先让用户看清「什么版本、什么加载器、多少个文件」。
#[tauri::command]
#[specta::specta]
pub async fn get_modpack_info(
    project_id: String,
    version_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ModpackInfo> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let pack =
        sereno_core::modpack::install::ensure_pack(&client, &settings, &project_id, &version_id)
            .await?;
    let index = sereno_core::modpack::read_index(&pack)?;

    let (loader, loader_version) = sereno_core::modpack::loader_of(&index).unwrap_or_default();

    Ok(ModpackInfo {
        name: if index.name.is_empty() {
            "未命名整合包".to_string()
        } else {
            index.name.clone()
        },
        minecraft_version: index.dependencies.minecraft.clone(),
        loader,
        loader_version,
        file_count: sereno_core::modpack::file_count(&index),
    })
}

/// 安装整合包：原版 + 加载器 + MOD + 覆盖文件，一次做完。
///
/// 整合包不决定实例名，实例由前端先用 `create_instance` 建好，
/// 这里只负责把内容装进那个实例。
#[tauri::command]
#[specta::specta]
pub async fn install_modpack(
    instance_id: String,
    project_id: String,
    version_id: String,
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

    let client = sereno_core::utils::http::build_client(&settings)?;
    let pack =
        sereno_core::modpack::install::ensure_pack(&client, &settings, &project_id, &version_id)
            .await?;
    let index = sereno_core::modpack::read_index(&pack)?;

    // 整合包要求的游戏版本必须与实例一致，否则装出来的实例根本进不去
    let required = index.dependencies.minecraft.trim();
    if !required.is_empty() && required != metadata.game_version {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "minecraftVersion".into(),
            message: format!(
                "整合包要求 Minecraft {required}，当前实例是 {}",
                metadata.game_version
            ),
        });
    }

    let sink: Arc<dyn ProgressSink> = Arc::new(ChannelSink(channel));

    metadata.status = "installing".to_string();
    write_metadata(&inst_dir, &metadata)?;

    let result = sereno_core::modpack::install::install_pack(
        &settings,
        &client,
        &inst_dir,
        &mut metadata,
        &index,
        &pack,
        sink,
    )
    .await;

    metadata.status = if result.is_ok() {
        "ready".to_string()
    } else {
        "created".to_string()
    };
    write_metadata(&inst_dir, &metadata)?;

    result
}
