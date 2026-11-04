//! 实例相关命令。

use tauri_plugin_dialog::DialogExt;

use sereno_core::Result;
use sereno_core::instance::import::ImportScan;
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

/// 设置 / 清除实例分组。
///
/// 归一化：首尾空白去掉；空串 = 移出分组（`None`）；上限 32 字符；
/// 禁止控制字符。分组只是展示元数据，不参与目录结构，无需迁移。
#[tauri::command]
#[specta::specta]
pub fn set_instance_group(
    instance_id: String,
    group: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<InstanceMetadata> {
    let normalized = match group.map(|g| g.trim().to_string()) {
        None => None,
        Some(g) if g.is_empty() => None,
        Some(g) => {
            if g.chars().count() > 32 {
                return Err(sereno_core::LauncherError::InvalidConfig {
                    key: "group".into(),
                    message: "分组名最长 32 个字符".into(),
                });
            }
            if g.chars().any(|c| c.is_control()) {
                return Err(sereno_core::LauncherError::InvalidConfig {
                    key: "group".into(),
                    message: "分组名不能包含控制字符".into(),
                });
            }
            Some(g)
        }
    };

    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let mut metadata = read_metadata(&inst_dir)?;
    metadata.group = normalized;
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

/// 扫描外部目录，找出可导入的实例候选。
///
/// 支持官方启动器 / PCL / HMCL（有 `versions/`）、
/// MultiMC / Prism（有 `.minecraft/`）与裸游戏目录。
#[tauri::command]
#[specta::specta]
pub fn scan_import_candidates(source: String) -> Result<ImportScan> {
    let path = std::path::PathBuf::from(source.trim());
    if path.as_os_str().is_empty() {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "source".into(),
            message: "请先选择要导入的目录".into(),
        });
    }
    sereno_core::instance::import::scan(&path)
}

/// 打开系统原生的“选择文件夹”对话框，用来指定要导入的外部实例目录。
#[tauri::command]
#[specta::specta]
pub async fn select_import_directory(app: tauri::AppHandle) -> Result<Option<String>> {
    let path = app
        .dialog()
        .file()
        .set_title("选择要导入的实例目录")
        .blocking_pick_folder();

    Ok(path.map(|p| p.to_string()))
}

/// 导入外部实例：新建实例 + 搬运用户数据（Mod、配置、存档）。
///
/// 游戏本体与库文件**不复制**，而是走 Sereno 自己的安装流程重新下载——
/// 更快、更准，还能用上镜像与 SHA-1 校验。
#[tauri::command]
#[specta::specta]
pub async fn import_instance(
    source: String,
    candidate_label: String,
    game_version: String,
    loader_type: String,
    loader_version: Option<String>,
    display_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<InstanceMetadata> {
    let source_path = std::path::PathBuf::from(source.trim());
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "displayName".into(),
            message: "实例名称不能为空".into(),
        });
    }
    let display_name: String = display_name.chars().take(64).collect();

    let game_version = game_version.trim().to_string();
    if game_version.is_empty() {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "gameVersion".into(),
            message: "外部目录里没有识别出 Minecraft 版本，请手动指定".into(),
        });
    }

    let settings = state.snapshot_settings()?;

    // 先确认游戏版本在 Mojang 清单里存在，否则装出来的实例根本进不去
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

    // 扫描源目录，取出游戏目录位置
    let scan = sereno_core::instance::import::scan(&source_path)?;
    let candidate = scan
        .candidates
        .iter()
        .find(|item| item.label == candidate_label)
        .or_else(|| scan.candidates.first())
        .ok_or_else(|| sereno_core::LauncherError::InvalidConfig {
            key: "source".into(),
            message: "选中的目录里没有可导入的实例".into(),
        })?;

    let source_game_dir = std::path::PathBuf::from(&candidate.game_dir);

    // 新建实例
    let mut metadata =
        sereno_core::instance::lifecycle::create_instance(&settings, &game_version, &display_name)?;
    metadata.loader = sereno_core::models::instance::LoaderInfo {
        r#type: if loader_type.trim().is_empty() {
            "vanilla".to_string()
        } else {
            loader_type.trim().to_string()
        },
        version: loader_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };

    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &metadata.id)?;
    sereno_core::storage::instance::write_metadata(&inst_dir, &metadata)?;

    // 搬运用户数据：Mod、配置、存档
    let (files, bytes) = sereno_core::instance::import::copy_user_data(
        &source_game_dir,
        &inst_dir.join("minecraft"),
    )?;
    tracing::info!(
        instance = %metadata.id,
        files,
        bytes,
        "外部实例数据导入完成"
    );

    Ok(metadata)
}
