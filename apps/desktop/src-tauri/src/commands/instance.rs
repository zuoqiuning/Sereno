//! 实例相关命令。

use tauri_plugin_dialog::DialogExt;

use sereno_core::Result;
use sereno_core::instance::lifecycle;
use sereno_core::models::crash::InstanceLogs;
use sereno_core::models::instance::InstanceMetadata;
use sereno_core::storage::instance::{read_metadata, write_metadata};

use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn read_instance_logs(
    instance_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<InstanceLogs> {
    let settings = state.snapshot_settings()?;
    sereno_core::instance::logs::read(&settings, &instance_id)
}

#[tauri::command]
#[specta::specta]
pub fn list_instances(state: tauri::State<'_, AppState>) -> Result<Vec<InstanceMetadata>> {
    let settings = state.snapshot_settings()?;
    lifecycle::list_instances(&settings)
}

#[tauri::command]
#[specta::specta]
pub async fn create_instance(
    game_version: String,
    display_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<InstanceMetadata> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "displayName".into(),
            message: "实例名称不能为空".into(),
        });
    }
    let display_name: String = display_name.chars().take(64).collect();

    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let manifest = match state.manifest_cache.get() {
        Some(manifest) => manifest,
        None => {
            let manifest = sereno_core::version::fetch_manifest(&client).await?;
            state.manifest_cache.set(manifest.clone());
            manifest
        }
    };

    if manifest.find(&game_version).is_none() {
        return Err(sereno_core::LauncherError::VersionNotFound {
            id: game_version.clone(),
        });
    }

    lifecycle::create_instance(&settings, &game_version, &display_name)
}

#[tauri::command]
#[specta::specta]
pub fn get_running_instances(state: tauri::State<'_, AppState>) -> Result<Vec<String>> {
    // 以实例目录里的运行锁为准：游戏退出（甚至启动器重启过）后，
    // 锁会被清掉，内存集合也要跟着收缩。
    let settings = state.snapshot_settings()?;
    let alive = sereno_core::instance::lock::running_instances(&settings);
    state.replace_running(alive.clone());
    Ok(alive)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_instance(instance_id: String, state: tauri::State<'_, AppState>) -> Result<()> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy { id: instance_id });
    }

    let settings = state.snapshot_settings()?;

    // 移入回收站可能耗时数秒，放到阻塞线程里执行，避免卡住 IPC 运行时
    tokio::task::spawn_blocking(move || {
        sereno_core::instance::lifecycle::delete_instance(&settings, &instance_id)
    })
    .await
    .map_err(|error| sereno_core::LauncherError::Internal {
        message: format!("删除实例任务异常: {error}"),
    })?
}
#[tauri::command]
#[specta::specta]
pub fn open_instance_folder(instance_id: String, state: tauri::State<'_, AppState>) -> Result<()> {
    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    // 打开 minecraft 目录，玩家最关心的都在这里
    let target = inst_dir.join("minecraft");
    opener::open(target).map_err(|e| sereno_core::LauncherError::Io {
        message: format!("打开文件夹失败: {e}"),
    })?;
    Ok(())
}

/// 打开系统原生的“选择文件”对话框，用来指定实例专用 Java。
#[tauri::command]
#[specta::specta]
pub async fn select_java_executable(app: tauri::AppHandle) -> Result<Option<String>> {
    let path = app
        .dialog()
        .file()
        .set_title("选择 Java 可执行文件")
        .blocking_pick_file();

    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
#[specta::specta]
pub fn set_instance_favorite(
    instance_id: String,
    favorite: bool,
    state: tauri::State<'_, AppState>,
) -> Result<InstanceMetadata> {
    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let mut metadata = read_metadata(&inst_dir)?;
    metadata.favorite = favorite;
    write_metadata(&inst_dir, &metadata)?;
    Ok(metadata)
}

/// 更新实例级启动配置：Java、内存与附加 JVM 参数。
///
/// 传 `null` / 空列表表示“回落到全局设置”。Java 路径会先探测一次，
/// 避免把一个不是 Java 的文件存进去、到启动时才报错。
#[tauri::command]
#[specta::specta]
pub async fn update_instance_options(
    instance_id: String,
    java_path: Option<String>,
    memory_mb: Option<u32>,
    jvm_args: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<InstanceMetadata> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy { id: instance_id });
    }

    let java_path = java_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());
    if let Some(path) = java_path.as_deref() {
        sereno_core::java::detector::probe(path)
            .await
            .map_err(|_| sereno_core::LauncherError::InvalidConfig {
                key: "javaPath".into(),
                message: format!("无法识别的 Java 可执行文件: {path}"),
            })?;
    }

    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let mut metadata = read_metadata(&inst_dir)?;

    metadata.java_path = java_path;
    metadata.memory_mb = memory_mb.map(|mb| mb.clamp(512, 32768));
    metadata.jvm_args = jvm_args
        .into_iter()
        .map(|arg| arg.trim().to_string())
        .filter(|arg| !arg.is_empty())
        .take(64)
        .collect();

    write_metadata(&inst_dir, &metadata)?;
    Ok(metadata)
}
