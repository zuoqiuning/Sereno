use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri_plugin_dialog::DialogExt;

use crate::ipc::progress::ChannelSink;
use crate::state::AppState;
use sereno_core::Result;
use sereno_core::download::sink::ProgressSink;
use sereno_core::models::download::DownloadProgress;
use sereno_core::models::resource::{
    LocalResource, LocalResourceKind, LocalResourceScan, ModVersion, ResourceDependencyInfo,
    ResourceKind, ResourceSearchPage, ResourceSearchQuery, ResourceSource,
};
use sereno_core::models::settings::AppSettings;
use sereno_core::storage::instance::read_metadata;

#[tauri::command]
#[specta::specta]
pub async fn search_resources(
    query: ResourceSearchQuery,
    state: tauri::State<'_, AppState>,
) -> Result<ResourceSearchPage> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let api_keys = sereno_core::resource::curseforge::api_keys();
    sereno_core::resource::search::search(&client, &api_keys, &query).await
}

/// Modrinth 获取版本列表。
///
/// `game_versions` 可以一次传多个游戏版本（Modrinth 的接口支持多值），
/// 这样「某个版本前缀下的全部小版本」只要一次请求。
#[tauri::command]
#[specta::specta]
pub async fn get_mod_versions(
    project_id: String,
    game_versions: Vec<String>,
    loader: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ModVersion>> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    sereno_core::resource::modrinth::get_versions_for(&client, &project_id, &game_versions, &loader)
        .await
}

/// 解析 MOD 版本声明的必需前置，返回名称与图标供详情页展示。
#[tauri::command]
#[specta::specta]
pub async fn get_resource_dependencies(
    source: String,
    ids: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ResourceDependencyInfo>> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let api_keys = sereno_core::resource::curseforge::api_keys();
    let source = match source.as_str() {
        "modrinth" => ResourceSource::Modrinth,
        "curseforge" => ResourceSource::Curseforge,
        _ => {
            return Err(sereno_core::LauncherError::InvalidConfig {
                key: "source".into(),
                message: format!("未知的资源来源: {source}"),
            });
        }
    };
    sereno_core::resource::dependency::resolve(&client, source, &api_keys, &ids).await
}

fn folder_for(kind: ResourceKind) -> Result<&'static str> {
    match kind {
        ResourceKind::Mod => Ok("mods"),
        ResourceKind::Resourcepack => Ok("resourcepacks"),
        ResourceKind::Shader => Ok("shaderpacks"),
        ResourceKind::Datapack => Ok("datapacks"),
        ResourceKind::Modpack => Err(sereno_core::LauncherError::InvalidConfig {
            key: "kind".into(),
            message: "整合包不能直接下载到实例目录".into(),
        }),
    }
}

/// 资源文件地址白名单。
///
/// 前端只能传 Modrinth / CurseForge / MCIM 的文件地址；否则一旦 WebView 被注入，
/// 启动器就会变成“任意 URL 下载器”，可以把文件写进实例目录。
fn ensure_resource_url(url: &str) -> Result<()> {
    const ALLOWED_HOST_SUFFIX: [&str; 3] = [".modrinth.com", ".forgecdn.net", ".mcimirror.top"];

    let rest =
        url.strip_prefix("https://")
            .ok_or_else(|| sereno_core::LauncherError::InvalidConfig {
                key: "url".into(),
                message: "下载地址必须使用 HTTPS".into(),
            })?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    if ALLOWED_HOST_SUFFIX
        .iter()
        .any(|suffix| host.ends_with(suffix))
    {
        Ok(())
    } else {
        Err(sereno_core::LauncherError::InvalidConfig {
            key: "url".into(),
            message: format!("不允许的下载地址: {host}"),
        })
    }
}

/// 一次资源下载的目标描述。
///
/// 收进结构体而不是八个位置参数：字段含义靠名字自解释，
/// 调用点也不会因为参数顺序写错而把 `sha1` 传成 `filename`。
struct DownloadTarget<'a> {
    /// 目标实例；`None` 表示下载到启动器的公共下载目录。
    instance_id: Option<&'a str>,
    /// 实例内的子目录（`mods` / `resourcepacks` / `shaderpacks`）。
    folder: &'a str,
    file_url: &'a str,
    filename: &'a str,
    sha1: Option<String>,
}

async fn download_to(
    settings: &AppSettings,
    target: DownloadTarget<'_>,
    sink: Option<Arc<dyn ProgressSink>>,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Result<()> {
    let DownloadTarget {
        instance_id,
        folder,
        file_url,
        filename,
        sha1,
    } = target;

    ensure_resource_url(file_url)?;
    let client = sereno_core::utils::http::build_client(settings)?;

    // 文件名来自远端 API / 前端参数，必须清理成安全文件名，
    // 否则 `../../evil.jar` 能直接写出实例目录。
    let filename = sereno_core::utils::fs::sanitize_file_name(filename);

    // 没有目标实例时下载到启动器的下载目录，用户随后可以手动导入到实例。
    let dest = match instance_id {
        Some(id) => {
            let inst_dir = sereno_core::utils::paths::instance_dir(settings, id)?;
            inst_dir.join("minecraft").join(folder).join(&filename)
        }
        None => sereno_core::utils::paths::state_dir()?
            .join("downloads")
            .join(folder)
            .join(&filename),
    };
    sereno_core::utils::paths::ensure_dir(dest.parent().unwrap())?;

    let task = sereno_core::download::task::DownloadTask {
        id: format!("{folder}-{filename}"),
        display_name: filename.clone(),
        sources: sereno_core::download::mirror::build_sources(file_url),
        destination: dest,
        checksum: match sha1 {
            Some(hash) => sereno_core::download::task::Checksum::Sha1(hash),
            None => sereno_core::download::task::Checksum::None,
        },
        size_hint: None,
        priority: 5,
        retry_policy: sereno_core::download::task::RetryPolicy::default(),
    };

    let queue = std::sync::Arc::new(sereno_core::download::queue::DownloadQueue::new(
        client,
        settings.download.max_concurrent as usize,
    ));

    // 有进度通道时走完整会话：前端任务页能拿到文件级进度与总字节数。
    if let Some(sink) = sink {
        let mut session = sereno_core::download::session::DownloadSession::new(
            "resource-download".to_string(),
            queue,
        );
        if let Some(token) = cancel {
            session.cancel_token = token;
        }
        return session.run(vec![task], sink).await;
    }

    let counter = std::sync::Arc::new(sereno_core::download::counter::SessionCounter::new(
        "resource-download",
        folder,
    ));
    counter.add_total(1, 0);

    queue
        .execute_task(
            task,
            tokio_util::sync::CancellationToken::new(),
            None,
            "resource-download",
            counter,
        )
        .await?;
    Ok(())
}

/// 下载资源文件。
///
/// `instance_id` 为空时下载到启动器的下载目录（`%APPDATA%\Sereno\downloads\<kind>`），
/// 用户可以先下载、再决定放进哪个实例。
//
// 参数扁平化是**有意为之**：这些参数就是 IPC 契约，前端调用点写一行即可读。
// 改成请求结构体会改变 `bindings.ts` 的生成形状（必须用 debug 构建重新生成并提交），
// 属于独立改动，已记录在 `docs/代码评审与PCL对比.md` 的 P2-1。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn download_resource(
    instance_id: Option<String>,
    kind: ResourceKind,
    file_url: String,
    filename: String,
    sha1: Option<String>,
    task_id: String,
    channel: Channel<DownloadProgress>,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let folder = folder_for(kind)?;
    let settings = state.snapshot_settings()?;

    // 任务页可以按 `task_id` 取消这次下载；没登记过就只是普通下载。
    let token = state.new_cancel_token(&task_id);
    let sink: Arc<dyn ProgressSink> = Arc::new(ChannelSink(channel));
    let result = download_to(
        &settings,
        DownloadTarget {
            instance_id: instance_id.as_deref(),
            folder,
            file_url: &file_url,
            filename: &filename,
            sha1,
        },
        Some(sink),
        Some(token),
    )
    .await;

    state.clear_cancel_token(&task_id);
    result
}

/// 取消一个资源下载任务（任务页的“取消”按钮）；返回是否命中任务。
#[tauri::command]
#[specta::specta]
pub fn cancel_download(task_id: String, state: tauri::State<'_, AppState>) -> Result<bool> {
    Ok(state.cancel_task(&task_id))
}

/// 安装单个必需前置：按目标实例的游戏版本 / 加载器挑选文件后下载到 `mods`。
#[tauri::command]
#[specta::specta]
pub async fn install_resource_dependency(
    instance_id: String,
    source: String,
    project_id: String,
    version_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let settings = state.snapshot_settings()?;

    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let metadata = read_metadata(&inst_dir)?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let loader = if metadata.loader.r#type == "vanilla" {
        String::new()
    } else {
        metadata.loader.r#type.clone()
    };

    let (url, filename, sha1) = match source.as_str() {
        "modrinth" => {
            let file = sereno_core::resource::modrinth::dependency_file(
                &client,
                &project_id,
                version_id.as_deref(),
                &metadata.game_version,
                &loader,
            )
            .await?
            .ok_or_else(|| sereno_core::LauncherError::DownloadFailed {
                message: format!(
                    "前置 {project_id} 没有适配 {} / {} 的文件",
                    metadata.game_version,
                    if loader.is_empty() { "原版" } else { &loader }
                ),
                url: format!("https://api.modrinth.com/v2/project/{project_id}/version"),
            })?;
            (file.url, file.filename, file.hashes.sha1)
        }
        "curseforge" => {
            let mod_id: u32 =
                project_id
                    .parse()
                    .map_err(|_| sereno_core::LauncherError::InvalidConfig {
                        key: "projectId".into(),
                        message: format!("无效的 CurseForge 项目 ID: {project_id}"),
                    })?;
            let api_key = sereno_core::resource::curseforge::api_keys()
                .into_iter()
                .next()
                .unwrap_or_default();
            if api_key.is_empty() && !sereno_core::resource::curseforge::mirror_enabled() {
                return Err(sereno_core::LauncherError::InvalidConfig {
                    key: "CURSEFORGE_API_KEY".into(),
                    message: "镜像已关闭，且未在环境变量中设置 CurseForge API Key".into(),
                });
            }
            let file = sereno_core::resource::curseforge::dependency_file(
                &client,
                mod_id,
                &metadata.game_version,
                &loader,
                &api_key,
            )
            .await?
            .ok_or_else(|| sereno_core::LauncherError::DownloadFailed {
                message: format!("前置 {project_id} 没有可下载文件"),
                url: format!("https://api.curseforge.com/v1/mods/{mod_id}/files"),
            })?;
            let url =
                file.download_url
                    .ok_or_else(|| sereno_core::LauncherError::DownloadFailed {
                        message: format!("前置 {project_id} 的文件禁止第三方下载"),
                        url: format!("https://api.curseforge.com/v1/mods/{mod_id}/files"),
                    })?;
            (url, file.file_name, None)
        }
        _ => {
            return Err(sereno_core::LauncherError::InvalidConfig {
                key: "source".into(),
                message: format!("未知的资源来源: {source}"),
            });
        }
    };

    download_to(
        &settings,
        DownloadTarget {
            instance_id: Some(&instance_id),
            folder: "mods",
            file_url: &url,
            filename: &filename,
            sha1,
        },
        None,
        None,
    )
    .await?;
    Ok(filename)
}

#[tauri::command]
#[specta::specta]
pub async fn update_local_resource(
    instance_id: String,
    kind: LocalResourceKind,
    old_file_name: String,
    new_file_name: String,
    file_url: String,
    sha1: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    if sereno_core::utils::fs::sanitize_file_name(&old_file_name) != old_file_name {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "oldFileName".into(),
            message: "无效的旧资源文件名".into(),
        });
    }

    let settings = state.snapshot_settings()?;
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy {
            id: instance_id.clone(),
        });
    }

    let folder = sereno_core::resource::local::folder_name(kind);
    let folder_path = sereno_core::resource::local::resource_folder(&settings, &instance_id, kind)?;
    let old_path = folder_path.join(&old_file_name);
    if !old_path.is_file() {
        return Err(sereno_core::LauncherError::ResourceNotFound {
            project_id: old_file_name.clone(),
        });
    }

    let enabled = !old_file_name.ends_with(".disabled");
    let new_name = sereno_core::utils::fs::sanitize_file_name(&new_file_name);
    let target_name = if enabled {
        new_name
    } else {
        format!("{new_name}.disabled")
    };
    let target_path = folder_path.join(&target_name);
    let same_name = target_name == old_file_name;

    if !same_name && target_path.exists() {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "newFileName".into(),
            message: format!("目标文件已存在：{target_name}"),
        });
    }

    // 同名更新时必须先把旧文件移走：下载器会跳过「已存在且校验通过」的文件，
    // 而 CurseForge 文件没有 SHA-1，否则会被误判为已完成。
    let backup_path = folder_path.join(format!("{target_name}.backup"));
    if same_name {
        if backup_path.exists() {
            let _ = std::fs::remove_file(&backup_path);
        }
        std::fs::rename(&old_path, &backup_path).map_err(|error| {
            sereno_core::LauncherError::Io {
                message: format!("备份旧资源失败: {error}"),
            }
        })?;
    }

    let download_result = download_to(
        &settings,
        DownloadTarget {
            instance_id: Some(&instance_id),
            folder,
            file_url: &file_url,
            filename: &target_name,
            sha1,
        },
        None,
        None,
    )
    .await;

    match download_result {
        Ok(()) => {
            if same_name && backup_path.exists() {
                let _ = std::fs::remove_file(&backup_path);
            }
            if !same_name
                && let Err(error) = sereno_core::resource::local::remove(
                    &settings,
                    &instance_id,
                    kind,
                    &old_file_name,
                )
            {
                tracing::warn!(%error, "更新后删除旧资源失败");
            }
            Ok(())
        }
        Err(error) => {
            if same_name && backup_path.exists() {
                let _ = std::fs::rename(&backup_path, &old_path);
            }
            Err(error)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn list_local_resources(
    instance_id: String,
    kind: LocalResourceKind,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<LocalResource>> {
    let settings = state.snapshot_settings()?;
    sereno_core::resource::local::list(&settings, &instance_id, kind)
}

#[tauri::command]
#[specta::specta]
pub fn set_local_resource_enabled(
    instance_id: String,
    kind: LocalResourceKind,
    file_name: String,
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy {
            id: instance_id.clone(),
        });
    }
    let settings = state.snapshot_settings()?;
    sereno_core::resource::local::set_enabled(&settings, &instance_id, kind, &file_name, enabled)
}

#[tauri::command]
#[specta::specta]
pub fn open_local_resource_folder(
    instance_id: String,
    kind: LocalResourceKind,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let settings = state.snapshot_settings()?;
    let folder = sereno_core::resource::local::resource_folder(&settings, &instance_id, kind)?;
    sereno_core::utils::paths::ensure_dir(&folder)?;
    opener::open(folder).map_err(|error| sereno_core::LauncherError::Io {
        message: format!("打开资源文件夹失败: {error}"),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn import_local_resources(
    instance_id: String,
    kind: LocalResourceKind,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<u32> {
    if state.is_running(&instance_id) {
        return Err(sereno_core::LauncherError::InstanceBusy {
            id: instance_id.clone(),
        });
    }
    let settings = state.snapshot_settings()?;
    let (label, extensions): (&str, &[&str]) = match kind {
        LocalResourceKind::Mod => ("Mod 文件", &["jar", "zip", "litemod"]),
        LocalResourceKind::Resourcepack => ("资源包", &["zip"]),
        LocalResourceKind::Shader => ("光影包", &["zip"]),
        LocalResourceKind::Datapack => ("数据包", &["zip"]),
    };

    let Some(files) = app
        .dialog()
        .file()
        .add_filter(label, extensions)
        .blocking_pick_files()
    else {
        return Ok(0);
    };

    let paths: Vec<PathBuf> = files
        .iter()
        .map(|file| PathBuf::from(file.to_string()))
        .collect();
    let imported = tokio::task::spawn_blocking(move || {
        sereno_core::resource::local::import(&settings, &instance_id, kind, &paths)
    })
    .await
    .map_err(|error| sereno_core::LauncherError::Internal {
        message: format!("导入资源任务失败: {error}"),
    })??;

    Ok(imported as u32)
}

#[tauri::command]
#[specta::specta]
pub async fn check_local_resource_updates(
    instance_id: String,
    kind: LocalResourceKind,
    state: tauri::State<'_, AppState>,
) -> Result<LocalResourceScan> {
    let settings = state.snapshot_settings()?;
    let inst_dir = sereno_core::utils::paths::instance_dir(&settings, &instance_id)?;
    let metadata = sereno_core::storage::instance::read_metadata(&inst_dir)?;
    let api_keys = sereno_core::resource::curseforge::api_keys();
    sereno_core::resource::local::check_updates(&settings, &api_keys, &metadata, kind).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_cf_mod_versions(
    mod_id: String,
    mc_version: String,
    loader: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<sereno_core::models::resource::CfFile>> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let api_key = sereno_core::resource::curseforge::api_keys()
        .into_iter()
        .next()
        .ok_or_else(|| sereno_core::LauncherError::InvalidConfig {
            key: "CURSEFORGE_API_KEY".into(),
            message: "未在环境变量中设置 CurseForge API Key".into(),
        })?;

    let mod_id_num: u32 =
        mod_id
            .parse()
            .map_err(|_| sereno_core::LauncherError::InvalidConfig {
                key: "mod_id".into(),
                message: "无效的 CurseForge Mod ID".into(),
            })?;

    sereno_core::resource::curseforge::get_versions(
        &client,
        mod_id_num,
        &mc_version,
        &loader,
        &api_key,
        sereno_core::resource::curseforge::VERSION_FIRST_PAGE_FILES,
    )
    .await
}

/// 批量补全 CurseForge 的版本分支：一次 IPC 拿回多个游戏版本的文件。
///
/// CurseForge 的文件列表按上传时间排序，只查一次会漏掉较早发布的分支
/// （例如 JEI 的 1.21.4 / 1.21.8）。这里按具体游戏版本逐个查询，
/// 查询之间并发执行，由后端统一限速。
#[tauri::command]
#[specta::specta]
pub async fn get_cf_mod_versions_batch(
    mod_id: String,
    versions: Vec<String>,
    loader: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<sereno_core::models::resource::CfFile>> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let api_key = sereno_core::resource::curseforge::api_keys()
        .into_iter()
        .next()
        .ok_or_else(|| sereno_core::LauncherError::InvalidConfig {
            key: "CURSEFORGE_API_KEY".into(),
            message: "未在环境变量中设置 CurseForge API Key".into(),
        })?;

    let mod_id_num: u32 =
        mod_id
            .parse()
            .map_err(|_| sereno_core::LauncherError::InvalidConfig {
                key: "mod_id".into(),
                message: "无效的 CurseForge Mod ID".into(),
            })?;

    sereno_core::resource::curseforge::get_versions_for(
        &client, mod_id_num, &versions, &loader, &api_key,
    )
    .await
}

/// 在系统浏览器打开资源项目页。slug 只允许字母数字与 `-_`，
/// 因此 URL 只能是两个站点的项目页，不接受前端传任意地址。
#[tauri::command]
#[specta::specta]
pub fn open_resource_page(source: String, slug: String, kind: String) -> Result<()> {
    let slug_valid = !slug.is_empty()
        && slug.len() <= 128
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !slug_valid {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "slug".into(),
            message: "无效的资源标识".into(),
        });
    }

    let url = match (source.as_str(), kind.as_str()) {
        ("curseforge", "modpack") => {
            format!("https://www.curseforge.com/minecraft/modpacks/{slug}")
        }
        ("curseforge", _) => format!("https://www.curseforge.com/minecraft/mc-mods/{slug}"),
        (_, "modpack") => format!("https://modrinth.com/modpack/{slug}"),
        (_, "datapack") => format!("https://modrinth.com/datapack/{slug}"),
        _ => format!("https://modrinth.com/mod/{slug}"),
    };

    opener::open(&url).map_err(|e| sereno_core::LauncherError::Io {
        message: format!("打开项目页失败: {e}"),
    })
}
