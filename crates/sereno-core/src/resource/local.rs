//! 实例本地资源管理：扫描、启停、导入与更新检查。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::models::instance::InstanceMetadata;
use crate::models::resource::{
    CfFile, CfLogo, LocalResource, LocalResourceKind, LocalResourceMeta, LocalResourceScan,
    LocalResourceUpdate, ResourceSource,
};
use crate::models::settings::AppSettings;
use crate::utils::http::{MAX_METADATA_BYTES, build_client, read_json_limited};
use crate::utils::paths::{ensure_dir, instance_dir};
use crate::{LauncherError, Result};

const DISABLED_SUFFIX: &str = ".disabled";
const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const CF_API: &str = "https://api.curseforge.com/v1";
const HASH_BATCH: usize = 100;

pub fn folder_name(kind: LocalResourceKind) -> &'static str {
    match kind {
        LocalResourceKind::Mod => "mods",
        LocalResourceKind::Resourcepack => "resourcepacks",
        LocalResourceKind::Shader => "shaderpacks",
        LocalResourceKind::Datapack => "datapacks",
    }
}

pub fn resource_folder(
    settings: &AppSettings,
    instance_id: &str,
    kind: LocalResourceKind,
) -> Result<PathBuf> {
    Ok(instance_dir(settings, instance_id)?
        .join("minecraft")
        .join(folder_name(kind)))
}

fn validate_file_name(name: &str) -> Result<()> {
    if name.is_empty() || crate::utils::fs::sanitize_file_name(name) != name {
        return Err(LauncherError::InvalidConfig {
            key: "fileName".into(),
            message: "无效的资源文件名".into(),
        });
    }
    Ok(())
}

fn is_allowed(kind: LocalResourceKind, base_name: &str) -> bool {
    let lower = base_name.to_ascii_lowercase();
    match kind {
        LocalResourceKind::Mod => {
            lower.ends_with(".jar") || lower.ends_with(".zip") || lower.ends_with(".litemod")
        }
        LocalResourceKind::Resourcepack
        | LocalResourceKind::Shader
        | LocalResourceKind::Datapack => lower.ends_with(".zip"),
    }
}

fn split_name(raw: &str) -> (String, bool) {
    match raw.strip_suffix(DISABLED_SUFFIX) {
        Some(base) => (base.to_string(), false),
        None => (raw.to_string(), true),
    }
}

fn display_name(base: &str) -> String {
    let lower = base.to_ascii_lowercase();
    for extension in [".litemod", ".jar", ".zip"] {
        if lower.ends_with(extension) {
            return base[..base.len() - extension.len()].to_string();
        }
    }
    base.to_string()
}

fn scan_entries(
    folder: &Path,
    kind: LocalResourceKind,
    skip_disabled: bool,
) -> Result<Vec<LocalResource>> {
    if !folder.exists() {
        return Ok(Vec::new());
    }

    let mut seen: HashMap<String, LocalResource> = HashMap::new();
    let entries = std::fs::read_dir(folder).map_err(|e| LauncherError::Io {
        message: format!("读取资源目录失败 {}: {e}", folder.display()),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(raw_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let (base, enabled) = split_name(raw_name);
        if !is_allowed(kind, &base) || (skip_disabled && !enabled) {
            continue;
        }
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        let key = base.to_ascii_lowercase();
        let candidate = LocalResource {
            kind,
            file_name: raw_name.to_string(),
            display_name: display_name(&base),
            enabled,
            size,
        };

        match seen.get(&key) {
            Some(existing) if existing.enabled && !enabled => continue,
            Some(existing) if !existing.enabled && enabled => {
                seen.insert(key, candidate);
            }
            Some(_) => continue,
            None => {
                seen.insert(key, candidate);
            }
        }
    }

    let mut resources: Vec<LocalResource> = seen.into_values().collect();
    resources.sort_by(|a, b| {
        a.display_name
            .to_ascii_lowercase()
            .cmp(&b.display_name.to_ascii_lowercase())
    });
    Ok(resources)
}

pub fn list(
    settings: &AppSettings,
    instance_id: &str,
    kind: LocalResourceKind,
) -> Result<Vec<LocalResource>> {
    scan_entries(&resource_folder(settings, instance_id, kind)?, kind, false)
}

pub fn set_enabled(
    settings: &AppSettings,
    instance_id: &str,
    kind: LocalResourceKind,
    file_name: &str,
    enabled: bool,
) -> Result<()> {
    validate_file_name(file_name)?;
    let folder = resource_folder(settings, instance_id, kind)?;
    let current = folder.join(file_name);
    if !current.is_file() {
        return Err(LauncherError::ResourceNotFound {
            project_id: file_name.to_string(),
        });
    }

    let base = file_name.strip_suffix(DISABLED_SUFFIX).unwrap_or(file_name);
    let target_name = if enabled {
        base.to_string()
    } else {
        format!("{base}{DISABLED_SUFFIX}")
    };
    if target_name == file_name {
        return Ok(());
    }

    let target = folder.join(&target_name);
    if target.exists() {
        return Err(LauncherError::InvalidConfig {
            key: "fileName".into(),
            message: "同名资源已存在，无法切换启用状态".into(),
        });
    }

    std::fs::rename(&current, &target).map_err(|e| LauncherError::Io {
        message: format!("切换资源启用状态失败 {}: {e}", current.display()),
    })
}

pub fn remove(
    settings: &AppSettings,
    instance_id: &str,
    kind: LocalResourceKind,
    file_name: &str,
) -> Result<()> {
    validate_file_name(file_name)?;
    let path = resource_folder(settings, instance_id, kind)?.join(file_name);
    if !path.is_file() {
        return Err(LauncherError::ResourceNotFound {
            project_id: file_name.to_string(),
        });
    }
    std::fs::remove_file(&path).map_err(|e| LauncherError::Io {
        message: format!("删除旧资源失败 {}: {e}", path.display()),
    })
}

pub fn import(
    settings: &AppSettings,
    instance_id: &str,
    kind: LocalResourceKind,
    sources: &[PathBuf],
) -> Result<usize> {
    let folder = resource_folder(settings, instance_id, kind)?;
    ensure_dir(&folder)?;

    let mut imported = 0usize;
    for source in sources {
        if !source.is_file() {
            continue;
        }
        let Some(raw_name) = source.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let file_name = crate::utils::fs::sanitize_file_name(raw_name);
        let (base, _) = split_name(&file_name);
        if !is_allowed(kind, &base) {
            return Err(LauncherError::InvalidConfig {
                key: "file".into(),
                message: format!("不支持的文件类型：{raw_name}"),
            });
        }
        let dest = folder.join(&file_name);
        if dest.exists() {
            return Err(LauncherError::InvalidConfig {
                key: "file".into(),
                message: format!("同名资源已存在：{raw_name}"),
            });
        }
        std::fs::copy(source, &dest).map_err(|e| LauncherError::Io {
            message: format!("导入资源失败 {}: {e}", source.display()),
        })?;
        imported += 1;
    }

    Ok(imported)
}

#[derive(Debug, Clone)]
struct HashedFile {
    file_name: String,
    display_name: String,
    sha1: String,
    fingerprint: u32,
}

fn scan_hashed(folder: &Path, kind: LocalResourceKind) -> Result<Vec<HashedFile>> {
    let resources = scan_entries(folder, kind, false)?;
    let mut result = Vec::new();

    for resource in resources {
        let path = folder.join(&resource.file_name);
        let Ok(bytes) = std::fs::read(&path) else {
            tracing::warn!(path = %path.display(), "读取本地资源失败，跳过更新检查");
            continue;
        };
        let sha1 = format!("{:x}", Sha1::digest(&bytes));
        result.push(HashedFile {
            file_name: resource.file_name,
            display_name: resource.display_name,
            sha1,
            fingerprint: curseforge_fingerprint(&bytes),
        });
    }

    Ok(result)
}

/// CurseForge 使用的 MurmurHash2（忽略空白字节，种子为 1）。
fn curseforge_fingerprint(bytes: &[u8]) -> u32 {
    let filtered: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|byte| !matches!(*byte, 9 | 10 | 13 | 32))
        .collect();

    let length = filtered.len() as u32;
    let mut h = 1u32 ^ length;
    let mut i = 0usize;

    while i + 4 <= filtered.len() {
        let mut k = u32::from_le_bytes([
            filtered[i],
            filtered[i + 1],
            filtered[i + 2],
            filtered[i + 3],
        ]);
        k = k.wrapping_mul(0x5bd1_e995);
        k ^= k >> 24;
        k = k.wrapping_mul(0x5bd1_e995);
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= k;
        i += 4;
    }

    match filtered.len() - i {
        3 => {
            h ^= u32::from(filtered[i]) | (u32::from(filtered[i + 1]) << 8);
            h ^= u32::from(filtered[i + 2]) << 16;
            h = h.wrapping_mul(0x5bd1_e995);
        }
        2 => {
            h ^= u32::from(filtered[i]) | (u32::from(filtered[i + 1]) << 8);
            h = h.wrapping_mul(0x5bd1_e995);
        }
        1 => {
            h ^= u32::from(filtered[i]);
            h = h.wrapping_mul(0x5bd1_e995);
        }
        _ => {}
    }

    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    h
}

async fn post_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<T> {
    let resp =
        client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| LauncherError::DownloadFailed {
                message: format!("请求失败: {e}"),
                url: url.to_string(),
            })?;
    if !resp.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("接口返回 HTTP {}", resp.status()),
            url: url.to_string(),
        });
    }
    read_json_limited(resp, MAX_METADATA_BYTES).await
}

pub(crate) async fn cf_post_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
    api_keys: &[String],
) -> Result<T> {
    if api_keys.is_empty() {
        return Err(LauncherError::InvalidConfig {
            key: "CURSEFORGE_API_KEY".into(),
            message: "未配置 CurseForge API Key".into(),
        });
    }

    // 与搜索共用同一套节流 / 在途上限 / Key 轮换 / 429 冷却规则。
    let resp = super::curseforge::request(client, url, api_keys, Some(body)).await?;
    if !resp.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("CurseForge 接口返回 HTTP {}", resp.status()),
            url: url.to_string(),
        });
    }
    read_json_limited(resp, MAX_METADATA_BYTES).await
}

#[derive(Debug, Clone, Deserialize)]
struct MrVersion {
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    date_published: Option<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    files: Vec<MrFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct MrFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: MrHashes,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MrHashes {
    sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MrProject {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon_url: Option<String>,
}

async fn modrinth_updates(
    client: &reqwest::Client,
    instance: &InstanceMetadata,
    files: &[HashedFile],
    metadata: &mut HashMap<String, LocalResourceMeta>,
    updates: &mut HashMap<String, LocalResourceUpdate>,
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let mut installed: HashMap<String, (MrVersion, &HashedFile)> = HashMap::new();
    for chunk in files.chunks(HASH_BATCH) {
        let hashes: Vec<&str> = chunk.iter().map(|file| file.sha1.as_str()).collect();
        let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
        let data: HashMap<String, MrVersion> =
            post_json(client, &format!("{MODRINTH_API}/version_files"), &body).await?;
        for file in chunk {
            if let Some(version) = data.get(&file.sha1) {
                installed.insert(file.sha1.clone(), (version.clone(), file));
            }
        }
    }

    let mut update_body = Vec::new();
    for (hash, (version, _)) in &installed {
        update_body.push(serde_json::json!({
            "hash": hash,
            "loaders": version.loaders,
            "game_versions": [instance.game_version],
        }));
    }
    if update_body.is_empty() {
        return Ok(());
    }

    let mut latest: HashMap<String, MrVersion> = HashMap::new();
    for chunk in update_body.chunks(HASH_BATCH) {
        let body = serde_json::json!({ "hashes": chunk, "algorithm": "sha1" });
        let data: HashMap<String, MrVersion> = post_json(
            client,
            &format!("{MODRINTH_API}/version_files/update_individual"),
            &body,
        )
        .await?;
        latest.extend(data);
    }

    // 所有匹配到的文件都需要项目信息（不只是有更新的），用于实例资源列表展示。
    let mut project_ids: Vec<String> = installed
        .values()
        .map(|(version, _)| version.project_id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    project_ids.sort();
    project_ids.dedup();

    for (hash, (installed_version, file)) in &installed {
        let Some(update) = latest.get(hash) else {
            continue;
        };
        if update.id == installed_version.id {
            continue;
        }
        let installed_date = installed_version.date_published.as_deref().unwrap_or("");
        let update_date = update.date_published.as_deref().unwrap_or("");
        if installed_date.is_empty() || update_date.is_empty() || update_date <= installed_date {
            continue;
        }
        let Some(download) = update
            .files
            .iter()
            .find(|entry| entry.primary)
            .or_else(|| update.files.first())
        else {
            continue;
        };
        if download.url.is_empty() {
            continue;
        }

        updates.insert(
            file.file_name.clone(),
            LocalResourceUpdate {
                file_name: file.file_name.clone(),
                title: update.version_number.clone(),
                icon_url: None,
                version_name: update.version_number.clone(),
                download_url: download.url.clone(),
                new_file_name: download.filename.clone(),
                sha1: download.hashes.sha1.clone(),
                source: ResourceSource::Modrinth,
            },
        );
    }

    let mut project_info: HashMap<String, MrProject> = HashMap::new();
    for chunk in project_ids.chunks(50) {
        let ids = serde_json::to_string(chunk).unwrap_or_else(|_| "[]".into());
        let url = format!("{MODRINTH_API}/projects?ids={}", urlencoding::encode(&ids));
        let Ok(projects) = client.get(&url).send().await else {
            continue;
        };
        if !projects.status().is_success() {
            continue;
        }
        if let Ok(list) = read_json_limited::<Vec<MrProject>>(projects, MAX_METADATA_BYTES).await {
            for project in list {
                project_info.insert(project.id.clone(), project);
            }
        }
    }

    for (installed_version, file) in installed.values() {
        let Some(project) = project_info.get(&installed_version.project_id) else {
            continue;
        };
        metadata
            .entry(file.file_name.clone())
            .or_insert(LocalResourceMeta {
                file_name: file.file_name.clone(),
                title: if project.title.is_empty() {
                    installed_version.version_number.clone()
                } else {
                    project.title.clone()
                },
                description: (!project.description.is_empty()).then(|| project.description.clone()),
                icon_url: project.icon_url.clone(),
                source: ResourceSource::Modrinth,
            });
    }

    for update in updates.values_mut() {
        if update.source != ResourceSource::Modrinth {
            continue;
        }
        if let Some(meta) = metadata.get(&update.file_name) {
            update.title = meta.title.clone();
            update.icon_url = meta.icon_url.clone();
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct CfFingerprintResponse {
    data: CfFingerprintData,
}

#[derive(Debug, Deserialize)]
struct CfFingerprintData {
    #[serde(rename = "exactMatches", default)]
    exact_matches: Vec<CfMatch>,
}

#[derive(Debug, Deserialize)]
struct CfMatch {
    id: u32,
    file: CfMatchFile,
}

#[derive(Debug, Clone, Deserialize)]
struct CfMatchFile {
    id: u32,
    #[serde(rename = "fileFingerprint", default)]
    fingerprint: u32,
    #[serde(rename = "fileDate", default)]
    file_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CfProject {
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    logo: Option<CfLogo>,
    #[serde(rename = "latestFilesIndexes", default)]
    latest_files_indexes: Vec<CfFileIndex>,
}

#[derive(Debug, Deserialize)]
struct CfFileIndex {
    #[serde(rename = "gameVersion", default)]
    game_version: String,
    #[serde(rename = "fileId", default)]
    file_id: u32,
    #[serde(rename = "modLoader", default)]
    mod_loader: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CfProjectList {
    #[serde(default)]
    data: Vec<CfProject>,
}

#[derive(Debug, Deserialize)]
struct CfFileList {
    #[serde(default)]
    data: Vec<CfFile>,
}

async fn curseforge_updates(
    client: &reqwest::Client,
    api_keys: &[String],
    instance: &InstanceMetadata,
    kind: LocalResourceKind,
    files: &[HashedFile],
    metadata: &mut HashMap<String, LocalResourceMeta>,
    updates: &mut HashMap<String, LocalResourceUpdate>,
) -> Result<()> {
    if files.is_empty() || api_keys.is_empty() {
        return Ok(());
    }

    let mut project_to_files: HashMap<u32, Vec<(&HashedFile, CfMatchFile)>> = HashMap::new();
    for chunk in files.chunks(HASH_BATCH) {
        let fingerprints: Vec<u32> = chunk.iter().map(|file| file.fingerprint).collect();
        let body = serde_json::json!({ "fingerprints": fingerprints });
        let response: CfFingerprintResponse = cf_post_json(
            client,
            &format!("{CF_API}/fingerprints/432"),
            &body,
            api_keys,
        )
        .await?;

        for exact in response.data.exact_matches {
            if let Some(file) = chunk
                .iter()
                .find(|candidate| candidate.fingerprint == exact.file.fingerprint)
            {
                project_to_files
                    .entry(exact.id)
                    .or_default()
                    .push((file, exact.file));
            }
        }
    }

    if project_to_files.is_empty() {
        return Ok(());
    }

    let mut project_ids: Vec<u32> = project_to_files.keys().copied().collect();
    project_ids.sort();
    project_ids.dedup();

    let mut projects: HashMap<u32, CfProject> = HashMap::new();
    for chunk in project_ids.chunks(50) {
        let body = serde_json::json!({ "modIds": chunk });
        let data: CfProjectList =
            cf_post_json(client, &format!("{CF_API}/mods"), &body, api_keys).await?;
        for project in data.data {
            projects.insert(project.id, project);
        }
    }

    for (project_id, locals) in &project_to_files {
        let Some(project) = projects.get(project_id) else {
            continue;
        };
        for (file, _) in locals {
            metadata
                .entry(file.file_name.clone())
                .or_insert(LocalResourceMeta {
                    file_name: file.file_name.clone(),
                    title: if project.name.is_empty() {
                        file.display_name.clone()
                    } else {
                        project.name.clone()
                    },
                    description: (!project.summary.is_empty()).then(|| project.summary.clone()),
                    icon_url: project.logo.as_ref().map(|logo| logo.thumbnail_url.clone()),
                    source: ResourceSource::Curseforge,
                });
        }
    }

    let target_loader = if kind == LocalResourceKind::Mod {
        cf_loader_type(&instance.loader.r#type)
    } else {
        None
    };

    let mut file_to_local: HashMap<u32, Vec<(&HashedFile, CfMatchFile)>> = HashMap::new();
    for (project_id, locals) in &project_to_files {
        let Some(project) = projects.get(project_id) else {
            continue;
        };
        for (file, installed) in locals {
            let candidate = project
                .latest_files_indexes
                .iter()
                .filter(|entry| entry.game_version == instance.game_version)
                .filter(|entry| {
                    if let Some(expected) = target_loader {
                        entry.mod_loader.is_none_or(|loader| loader == expected)
                    } else {
                        true
                    }
                })
                .max_by_key(|entry| entry.file_id);

            if let Some(entry) = candidate {
                file_to_local
                    .entry(entry.file_id)
                    .or_default()
                    .push((file, installed.clone()));
            }
        }
    }

    let mut file_ids: Vec<u32> = file_to_local.keys().copied().collect();
    file_ids.sort();
    file_ids.dedup();

    let mut remote_files: HashMap<u32, CfFile> = HashMap::new();
    for chunk in file_ids.chunks(50) {
        let body = serde_json::json!({ "fileIds": chunk });
        let data: CfFileList =
            cf_post_json(client, &format!("{CF_API}/mods/files"), &body, api_keys).await?;
        for file in data.data {
            remote_files.insert(file.id, file);
        }
    }

    for (file_id, locals) in file_to_local {
        let Some(remote) = remote_files.get(&file_id) else {
            continue;
        };
        let Some(download_url) = remote.download_url.clone().filter(|url| !url.is_empty()) else {
            continue;
        };

        for (local, installed) in locals {
            if updates.contains_key(&local.file_name) {
                continue;
            }
            let installed_date = installed.file_date.as_deref().unwrap_or("");
            let remote_date = remote.file_date.as_deref().unwrap_or("");
            if installed_date.is_empty()
                || remote_date.is_empty()
                || remote_date <= installed_date
                || remote.id == installed.id
            {
                continue;
            }

            let meta = metadata.get(&local.file_name);
            updates.insert(
                local.file_name.clone(),
                LocalResourceUpdate {
                    file_name: local.file_name.clone(),
                    title: meta
                        .map(|meta| meta.title.clone())
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| local.display_name.clone()),
                    icon_url: meta.and_then(|meta| meta.icon_url.clone()),
                    version_name: remote.display_name.clone(),
                    download_url: download_url.clone(),
                    new_file_name: remote.file_name.clone(),
                    sha1: None,
                    source: ResourceSource::Curseforge,
                },
            );
        }
    }

    Ok(())
}

fn cf_loader_type(loader: &str) -> Option<u32> {
    match loader {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

pub async fn check_updates(
    settings: &AppSettings,
    api_keys: &[String],
    instance: &InstanceMetadata,
    kind: LocalResourceKind,
) -> Result<LocalResourceScan> {
    let folder = resource_folder(settings, &instance.id, kind)?;
    let files = tokio::task::spawn_blocking(move || scan_hashed(&folder, kind))
        .await
        .map_err(|error| LauncherError::Internal {
            message: format!("本地资源扫描任务失败: {error}"),
        })??;

    if files.is_empty() {
        return Ok(LocalResourceScan {
            metadata: Vec::new(),
            updates: Vec::new(),
        });
    }

    let client = build_client(settings)?;
    let mut metadata: HashMap<String, LocalResourceMeta> = HashMap::new();
    let mut updates: HashMap<String, LocalResourceUpdate> = HashMap::new();

    if let Err(error) =
        modrinth_updates(&client, instance, &files, &mut metadata, &mut updates).await
    {
        tracing::warn!(%error, "Modrinth 元数据匹配失败");
    }
    if let Err(error) = curseforge_updates(
        &client,
        api_keys,
        instance,
        kind,
        &files,
        &mut metadata,
        &mut updates,
    )
    .await
    {
        tracing::warn!(%error, "CurseForge 元数据匹配失败");
    }

    Ok(LocalResourceScan {
        metadata: metadata.into_values().collect(),
        updates: updates.into_values().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curseforge_fingerprint_is_stable_and_ignores_whitespace() {
        let a = curseforge_fingerprint(b"hello world");
        let b = curseforge_fingerprint(b"hello \n\t world");
        assert_eq!(a, b);
    }
}
