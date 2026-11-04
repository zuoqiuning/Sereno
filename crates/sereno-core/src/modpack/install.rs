//! 整合包安装流程：原版 → 加载器 → MOD → 覆盖文件。
//!
//! 放在 core 而不是命令层：命令层不直接依赖 `reqwest`，
//! 而这一步既要下载又要复用 core 的下载器与加载器安装。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::Result;
use crate::download::counter::SessionCounter;
use crate::download::queue::DownloadQueue;
use crate::download::session::{DownloadSession, spawn_summary_pusher};
use crate::download::sink::ProgressSink;
use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::models::download::DownloadProgress;
use crate::models::instance::InstanceMetadata;
use crate::models::settings::AppSettings;
use crate::{LauncherError, modpack};

/// 取得整合包文件：命中缓存直接用，否则从 Modrinth 下载并缓存。
pub async fn ensure_pack(
    client: &reqwest::Client,
    settings: &AppSettings,
    project_id: &str,
    version_id: &str,
) -> Result<PathBuf> {
    let path = modpack::cache_path(project_id, version_id)?;
    if path.exists()
        && let Ok(meta) = std::fs::metadata(&path)
        && meta.len() > 0
    {
        return Ok(path);
    }

    let versions = crate::resource::modrinth::get_versions(client, project_id, "", "").await?;
    let version = versions
        .iter()
        .find(|version| version.id == version_id)
        .ok_or_else(|| LauncherError::LoaderNotFound {
            loader: "modpack".into(),
            version: version_id.to_string(),
        })?;

    let url = version
        .files
        .first()
        .map(|file| file.url.clone())
        .ok_or_else(|| LauncherError::ResourceNotFound {
            project_id: project_id.to_string(),
        })?;

    if let Some(parent) = path.parent() {
        crate::utils::paths::ensure_dir(parent)?;
    }

    let queue = DownloadQueue::new(client.clone(), 2);
    let counter = Arc::new(SessionCounter::new("modpack-fetch", "modpack"));
    counter.add_total(1, 0);

    queue
        .execute_task(
            DownloadTask {
                id: "modpack".to_string(),
                display_name: "整合包".to_string(),
                sources: crate::download::mirror::build_sources_with(
                    &url,
                    settings.download.prefer_mirror,
                ),
                destination: path.clone(),
                checksum: Checksum::None,
                size_hint: None,
                priority: 6,
                retry_policy: RetryPolicy {
                    max_retries: 5,
                    ..RetryPolicy::default()
                },
            },
            CancellationToken::new(),
            None,
            "modpack-fetch",
            counter,
        )
        .await?;

    Ok(path)
}

/// 把整合包装进实例：原版 → 加载器 → MOD → 覆盖文件。
///
/// `metadata` 会被就地更新（加载器信息），调用方负责写回磁盘。
pub async fn install_pack(
    settings: &AppSettings,
    client: &reqwest::Client,
    inst_dir: &Path,
    metadata: &mut InstanceMetadata,
    index: &modpack::ModpackIndex,
    pack: &Path,
    sink: Arc<dyn ProgressSink>,
) -> Result<()> {
    // 1. 原版游戏
    crate::install::game::install_base(settings, inst_dir, &metadata.game_version, sink.clone())
        .await?;

    // 2. 加载器（整合包声明了才有）
    if let Some((loader, version)) = modpack::loader_of(index) {
        let info = crate::loader::install::install_loader_into(
            settings,
            inst_dir,
            metadata,
            &loader,
            Some(&version),
        )
        .await?;
        metadata.loader = info;
    }

    // 3. MOD 与其它资源文件
    let tasks = modpack::build_file_tasks(index, inst_dir, settings.download.prefer_mirror);
    if !tasks.is_empty() {
        download_pack_files(client, settings, &tasks, sink.clone()).await?;
    }

    // 4. 覆盖文件（配置、脚本等）
    let overrides = modpack::extract_overrides(pack, inst_dir)?;
    if overrides > 0 {
        tracing::info!("整合包覆盖文件已写入 {overrides} 个文件");
    }

    Ok(())
}

/// 下载整合包声明的文件。
async fn download_pack_files(
    client: &reqwest::Client,
    settings: &AppSettings,
    tasks: &[DownloadTask],
    sink: Arc<dyn ProgressSink>,
) -> Result<()> {
    let queue = Arc::new(DownloadQueue::new(
        client.clone(),
        settings.download.max_concurrent as usize,
    ));

    let session_id = "modpack".to_string();
    let counter = Arc::new(SessionCounter::new(session_id.clone(), "modpack"));
    // 阶段 3：原版与加载器已经装完，正在拉整合包文件
    counter.set_stage(3);
    counter.add_total(
        tasks.len() as u32,
        tasks.iter().filter_map(|task| task.size_hint).sum(),
    );

    let summary_cancel = CancellationToken::new();
    let summary_handle = spawn_summary_pusher(
        Arc::clone(&counter),
        Arc::clone(&sink),
        summary_cancel.clone(),
    );

    let session = DownloadSession::new(session_id, queue);
    let result = session
        .run_with_counter(tasks.to_vec(), Arc::clone(&sink), Arc::clone(&counter))
        .await;

    summary_cancel.cancel();
    summary_handle.abort();
    sink.publish(DownloadProgress::Session {
        summary: counter.snapshot(),
    });

    result
}

/// 取得 CurseForge 整合包文件：命中缓存直接用，否则从 CF 下载并缓存。
///
/// 整合包在 CF 上是一个普通文件（`projectID` + `fileID`），
/// 下载地址要查 API 才知道。
pub async fn ensure_cf_pack(
    client: &reqwest::Client,
    settings: &AppSettings,
    project_id: u32,
    file_id: u32,
) -> Result<PathBuf> {
    let path = modpack::curseforge::cache_path(project_id, file_id)?;
    if path.exists()
        && let Ok(meta) = std::fs::metadata(&path)
        && meta.len() > 0
    {
        return Ok(path);
    }

    let keys = crate::resource::curseforge::api_keys();
    let file = crate::resource::curseforge::get_file(client, project_id, file_id, &keys)
        .await?
        .ok_or_else(|| LauncherError::ResourceNotFound {
            project_id: format!("{project_id}:{file_id}"),
        })?;

    let Some(url) = file.download_url else {
        return Err(LauncherError::DownloadFailed {
            message: "该整合包不允许第三方下载，请到 CurseForge 页面手动下载".into(),
            url: format!("https://www.curseforge.com/project/{project_id}"),
        });
    };

    if let Some(parent) = path.parent() {
        crate::utils::paths::ensure_dir(parent)?;
    }

    let queue = DownloadQueue::new(client.clone(), 2);
    let counter = Arc::new(SessionCounter::new("cf-pack-fetch", "modpack"));
    counter.add_total(1, file.file_length);

    queue
        .execute_task(
            DownloadTask {
                id: "cf-pack".to_string(),
                display_name: "CurseForge 整合包".to_string(),
                sources: crate::download::mirror::build_sources_with(
                    &url,
                    settings.download.prefer_mirror,
                ),
                destination: path.clone(),
                checksum: Checksum::None,
                size_hint: if file.file_length > 0 {
                    Some(file.file_length)
                } else {
                    None
                },
                priority: 6,
                retry_policy: RetryPolicy {
                    max_retries: 5,
                    ..RetryPolicy::default()
                },
            },
            CancellationToken::new(),
            None,
            "cf-pack-fetch",
            counter,
        )
        .await?;

    Ok(path)
}

/// 读取 CurseForge 整合包的 `manifest.json`。
///
/// **优先走按需读取**：用 HTTP Range 只取 ZIP 里的那一个条目，
/// 不必为了几 KB 的索引把几十 MB 的包整个下下来。
///
/// 按需读取失败（服务端不支持 Range、ZIP 结构异常）时回落到
/// 「整包下载后本地解析」——慢一点，但一定能work。
pub async fn read_cf_manifest(
    client: &reqwest::Client,
    settings: &AppSettings,
    project_id: u32,
    file_id: u32,
) -> Result<(modpack::curseforge::CfManifest, Option<PathBuf>)> {
    let keys = crate::resource::curseforge::api_keys();

    if let Some(file) =
        crate::resource::curseforge::get_file(client, project_id, file_id, &keys).await?
        && let Some(url) = file.download_url.clone()
    {
        match crate::utils::remote_zip::read_entry(client, &url, "manifest.json").await {
            Ok(bytes) => {
                let manifest: modpack::curseforge::CfManifest = serde_json::from_slice(&bytes)
                    .map_err(|e| LauncherError::Parse {
                        message: format!("解析整合包索引失败: {e}"),
                    })?;
                return Ok((manifest, None));
            }
            Err(error) => {
                tracing::debug!(%error, "整合包索引按需读取失败，回落到整包下载");
            }
        }
    }

    // 回落：整包下载后再解析
    let pack = ensure_cf_pack(client, settings, project_id, file_id).await?;
    let manifest = modpack::curseforge::read_manifest(&pack)?;
    Ok((manifest, Some(pack)))
}

/// 把 CurseForge 整合包装进实例。
///
/// 与 Modrinth 的差别在第 3 步：CF 的 `manifest.json` 里没有下载地址，
/// 必须先用 `POST /v1/mods/files` 把 `fileID` 解析成下载地址与 SHA-1，
/// 再用 `POST /v1/mods` 拿项目分类（决定文件落到 `mods/` 还是 `resourcepacks/`）。
pub async fn install_cf_pack(
    settings: &AppSettings,
    client: &reqwest::Client,
    inst_dir: &Path,
    metadata: &mut InstanceMetadata,
    manifest: &modpack::curseforge::CfManifest,
    pack_path: &Path,
    sink: Arc<dyn ProgressSink>,
) -> Result<()> {
    // 1. 原版游戏
    crate::install::game::install_base(settings, inst_dir, &metadata.game_version, sink.clone())
        .await?;

    // 2. 加载器（整合包声明了才有）
    if let Some((loader, version)) = modpack::curseforge::loader_of(manifest) {
        let info = crate::loader::install::install_loader_into(
            settings,
            inst_dir,
            metadata,
            &loader,
            Some(&version),
        )
        .await?;
        metadata.loader = info;
    }

    // 3. 解析文件清单 → 下载地址
    let file_ids = modpack::curseforge::file_ids(manifest);
    let project_ids = modpack::curseforge::project_ids(manifest);

    let keys = crate::resource::curseforge::api_keys();
    let (files, projects) = if file_ids.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let files = crate::resource::curseforge::get_files_by_ids(client, &file_ids, &keys).await?;
        let projects = if project_ids.is_empty() {
            Vec::new()
        } else {
            crate::resource::curseforge::get_mods_by_ids(client, &project_ids, &keys).await?
        };
        (files, projects)
    };

    let resolved = modpack::curseforge::resolve_files(manifest, &files, &projects);
    if !resolved.is_empty() {
        let tasks = modpack::curseforge::build_file_tasks(
            &resolved,
            inst_dir,
            settings.download.prefer_mirror,
        );
        download_pack_files(client, settings, &tasks, sink.clone()).await?;
    }

    // 4. 覆盖文件（配置、脚本等）
    let overrides = modpack::curseforge::extract_overrides(manifest, pack_path, inst_dir)?;
    if overrides > 0 {
        tracing::info!("CurseForge 整合包覆盖文件已写入 {overrides} 个文件");
    }

    Ok(())
}
