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

    // 3a. 增量更新：清理上一版有而本版没有的文件（差量下载由队列层
    //     对已存在文件的校验跳过自动完成）
    let declared: std::collections::HashSet<PathBuf> = index
        .files
        .iter()
        .filter_map(|file| crate::utils::archive::validate_entry_path(&file.path).ok())
        .collect();
    let previous = load_recorded_files(inst_dir);
    if !previous.is_empty() {
        let removed = cleanup_stale_pack_files(inst_dir, &previous, &declared);
        if removed > 0 {
            tracing::info!("整合包增量更新：清理旧版本残留 {removed} 个文件");
        }
    }

    if !tasks.is_empty() {
        download_pack_files(client, settings, &tasks, sink.clone()).await?;
    }

    // 4. 覆盖文件（配置、脚本等）
    let overrides = modpack::extract_overrides(pack, inst_dir)?;
    if overrides > 0 {
        tracing::info!("整合包覆盖文件已写入 {overrides} 个文件");
    }

    // 5. 记录本版文件清单，供下次增量更新清理（只在成功后写）
    record_files(inst_dir, declared.into_iter().collect());

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

    let file = crate::resource::curseforge::get_file(client, project_id, file_id)
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
    if let Some(file) = crate::resource::curseforge::get_file(client, project_id, file_id).await?
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

    let (files, projects) = if file_ids.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let files = crate::resource::curseforge::get_files_by_ids(client, &file_ids).await?;
        let projects = if project_ids.is_empty() {
            Vec::new()
        } else {
            crate::resource::curseforge::get_mods_by_ids(client, &project_ids).await?
        };
        (files, projects)
    };

    let resolved = modpack::curseforge::resolve_files(manifest, &files, &projects);

    // 3a. 增量更新：清理上一版残留（与 Modrinth 同款机制）
    let declared: std::collections::HashSet<PathBuf> = resolved
        .iter()
        .filter_map(|file| {
            crate::utils::archive::validate_entry_path(&format!(
                "{}/{}",
                file.folder, file.file_name
            ))
            .ok()
        })
        .collect();
    let previous = load_recorded_files(inst_dir);
    if !previous.is_empty() {
        let removed = cleanup_stale_pack_files(inst_dir, &previous, &declared);
        if removed > 0 {
            tracing::info!("整合包增量更新：清理旧版本残留 {removed} 个文件");
        }
    }

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

    // 5. 记录本版文件清单（只在成功后写）
    record_files(inst_dir, declared.into_iter().collect());

    Ok(())
}

// ============================================================
// 增量更新：文件清单记录 + 旧版残留清理
//
// 队列层已对「已存在且校验通过」的文件直接跳过（差量下载免费获得）；
// 这里补齐 HMCL 的另一半：**清理旧版本有而新版本没有的文件**——
// 否则换版本后残留的旧 mod 会与新版冲突。
// ============================================================

/// 记录文件：`<实例>/.launcher/modpack-files.json`。
const PACK_FILES_RECORD: &str = ".launcher/modpack-files.json";

/// 本版整合包声明的文件路径（相对游戏目录 `minecraft/`）。
fn record_path(inst_dir: &Path) -> PathBuf {
    inst_dir.join(PACK_FILES_RECORD)
}

/// 读取上次安装记录的文件清单；无记录 / 解析失败返回空（清理自动变空操作）。
fn load_recorded_files(inst_dir: &Path) -> Vec<PathBuf> {
    let Ok(bytes) = std::fs::read(record_path(inst_dir)) else {
        return Vec::new();
    };
    #[derive(serde::Deserialize)]
    struct Record {
        #[serde(default)]
        files: Vec<String>,
    }
    let Ok(record) = serde_json::from_slice::<Record>(&bytes) else {
        return Vec::new();
    };
    record
        .files
        .into_iter()
        // 只信任能通过安全校验的路径——记录文件本身也要防篡改
        .filter_map(|relative| crate::utils::archive::validate_entry_path(&relative).ok())
        .collect()
}

/// 写入本版文件清单（原子写；记录失败只降级为「下次全量」，不影响安装结果）。
fn record_files(inst_dir: &Path, mut files: Vec<PathBuf>) {
    files.sort();
    files.dedup();
    #[derive(serde::Serialize)]
    struct Record {
        schema_version: u32,
        files: Vec<String>,
    }
    let record = Record {
        schema_version: 1,
        files: files
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect(),
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&record) else {
        return;
    };
    if let Err(error) = crate::utils::fs::atomic_write(&record_path(inst_dir), &bytes) {
        tracing::warn!(%error, "整合包文件清单写入失败（下次更新将退化为全量）");
    }
}

/// 清理上一版残留：记录里**有**而新清单里**没有**的文件。
///
/// 只删上次记录过的文件——用户自己放进去的 mod（不在记录里）绝不动。
/// 返回删除的文件数。
fn cleanup_stale_pack_files(
    inst_dir: &Path,
    previous: &[PathBuf],
    declared: &std::collections::HashSet<PathBuf>,
) -> usize {
    let minecraft = inst_dir.join("minecraft");
    let mut removed = 0usize;
    for relative in previous {
        if declared.contains(relative) {
            continue;
        }
        let path = minecraft.join(relative);
        if !path.is_file() {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {
                removed += 1;
                tracing::debug!(file = %path.display(), "整合包更新：移除旧版本文件");
            }
            Err(error) => {
                tracing::warn!(file = %path.display(), %error, "旧版本文件删除失败，保留");
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn cleanup_removes_only_recorded_stale_files() {
        let tmp = tempfile::tempdir().unwrap();
        let inst = tmp.path();
        let mods = inst.join("minecraft").join("mods");
        touch(&mods.join("kept.jar")); // 新旧都在
        touch(&mods.join("stale.jar")); // 旧有新无（记录过）
        touch(&mods.join("user-mod.jar")); // 用户自装（从未记录）

        let previous: Vec<PathBuf> = ["mods/kept.jar", "mods/stale.jar"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let declared: std::collections::HashSet<PathBuf> =
            ["mods/kept.jar"].iter().map(PathBuf::from).collect();

        let removed = cleanup_stale_pack_files(inst, &previous, &declared);
        assert_eq!(removed, 1);
        assert!(mods.join("kept.jar").is_file());
        assert!(!mods.join("stale.jar").exists());
        // 用户自装文件必须毫发无损
        assert!(mods.join("user-mod.jar").is_file());
    }

    #[test]
    fn cleanup_ignores_unvalidated_recorded_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let inst = tmp.path();
        touch(&inst.join("evil.txt"));

        // 记录文件被篡改成穿越路径：load 阶段直接丢弃，不会删除任何东西
        let previous = load_recorded_files_with(inst, &["../evil.txt".to_string()]);
        let declared: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        let removed = cleanup_stale_pack_files(inst, &previous, &declared);
        assert_eq!(removed, 0);
        assert!(inst.join("evil.txt").is_file());
    }

    /// 测试辅助：绕过文件读取直接构造「已加载」的记录。
    fn load_recorded_files_with(_inst_dir: &Path, raw: &[String]) -> Vec<PathBuf> {
        raw.iter()
            .filter_map(|relative| crate::utils::archive::validate_entry_path(relative).ok())
            .collect()
    }
}
