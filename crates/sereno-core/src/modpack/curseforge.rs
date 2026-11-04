//! CurseForge 整合包（`.zip` + `manifest.json`）的解析与安装素材生成。
//!
//! ## 与 Modrinth 的本质差异
//!
//! `.mrpack` 的索引里**自带下载地址和哈希**，装它不需要查任何 API。
//! CurseForge 的 `manifest.json` 里只有 `projectID` / `fileID` 两个数字，
//! **下载地址必须回查 API 才知道**——这就是 CurseForge 整合包当年难装的原因。
//!
//! 本模块负责：
//!
//! 1. 解析 `manifest.json`（游戏版本、加载器、文件清单、覆盖目录）；
//! 2. 把 `projectID` / `fileID` 解析成真正的下载地址与 SHA-1；
//! 3. 按项目分类把文件落到正确的目录（`mods/` / `resourcepacks/` …）。
//!
//! **安全边界**：与 Modrinth 一样，路径来自网络，必须过归档路径校验。

use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::models::resource::{CfFile, CfModBrief};
use crate::utils::archive::validate_entry_path;
use crate::{LauncherError, Result};

/// CurseForge 整合包索引（`manifest.json`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CfManifest {
    /// 整合包名称。
    #[serde(default)]
    pub name: String,
    /// 整合包自身的版本号。
    #[serde(default)]
    pub version: String,
    /// 作者。
    #[serde(default)]
    pub author: String,
    /// 目标游戏版本与加载器。
    #[serde(default)]
    pub minecraft: CfMinecraft,
    /// 要安装的文件清单。
    #[serde(default)]
    pub files: Vec<CfManifestFile>,
    /// 覆盖目录名（相对压缩包根，通常是 `overrides`）。
    #[serde(default)]
    pub overrides: String,
    /// 仅客户端生效的覆盖目录名。
    #[serde(default)]
    pub client_overrides: String,
    /// 服务端覆盖目录名。**客户端不装**。
    #[serde(default)]
    pub server_overrides: String,
}

/// `manifest.json` 的 `minecraft` 段。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CfMinecraft {
    /// 要求的 Minecraft 版本。
    #[serde(default)]
    pub version: String,
    /// 加载器列表，取 `primary: true` 的那条。
    ///
    /// 字段名是 `modLoaders`（驼峰），不是 Rust 惯例的 `mod_loaders`。
    #[serde(rename = "modLoaders", default)]
    pub mod_loaders: Vec<CfModLoader>,
}

/// 加载器声明。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CfModLoader {
    /// 形如 `forge-47.2.0` / `neoforge-20.4.100` / `fabric-0.15.0`。
    #[serde(default)]
    pub id: String,
    /// 是否为主加载器。
    #[serde(default)]
    pub primary: bool,
}

/// 清单里的一个文件引用。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CfManifestFile {
    /// CurseForge 项目 ID（`projectID`）。
    #[serde(rename = "projectID", default)]
    pub project_id: u32,
    /// CurseForge 文件 ID（`fileID`）。
    #[serde(rename = "fileID", default)]
    pub file_id: u32,
    /// 是否必需。`false` 的文件多为服务端专用或可选组件。
    // 缺省 true：官方导出每项都显式写 required，但手写 / 第三方工具生成的
    // manifest 可能省略——「文件清单默认全部要装」才是符合直觉的语义，
    // 缺省 false 会让整个整合包装完是空的。
    #[serde(default = "default_true")]
    pub required: bool,
}

/// 读取 CurseForge 整合包里的 `manifest.json`。
pub fn read_manifest(pack_path: &Path) -> Result<CfManifest> {
    let file = std::fs::File::open(pack_path).map_err(|e| LauncherError::Io {
        message: format!("打开整合包失败 {}: {e}", pack_path.display()),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| LauncherError::Parse {
        message: format!("解析整合包失败 {}: {e}", pack_path.display()),
    })?;

    // 少数整合包把 manifest 放在子目录里（作者自制压缩），
    // 因此找不到根目录的 manifest 时再扫一遍条目名。
    let mut index = archive.by_name("manifest.json").ok().map(|_| 0usize);
    if index.is_none() {
        index = (0..archive.len()).find(|position| {
            archive
                .by_index(*position)
                .ok()
                .map(|entry| {
                    let name = entry.name().to_string();
                    name.ends_with("/manifest.json")
                })
                .unwrap_or(false)
        });
    }

    let Some(index) = index else {
        return Err(LauncherError::Parse {
            message: "整合包缺少 manifest.json，可能不是有效的 CurseForge 整合包".into(),
        });
    };

    let mut text = String::new();
    archive
        .by_index(index)
        .map_err(|e| LauncherError::Parse {
            message: format!("读取整合包索引失败: {e}"),
        })?
        .read_to_string(&mut text)
        .map_err(|e| LauncherError::Parse {
            message: format!("读取整合包索引失败: {e}"),
        })?;

    // 有些整合包的 manifest 带 BOM，serde_json 会直接报错
    if text.starts_with('\u{feff}') {
        text.remove(0);
    }

    serde_json::from_str(&text).map_err(|e| LauncherError::Parse {
        message: format!("解析整合包索引失败: {e}"),
    })
}

/// 整合包要求的加载器：`(类型, 版本)`；原版整合包返回 `None`。
///
/// 只认 `primary: true` 的那条。没有 primary 标记时取第一条——
/// 实践中 CF 导出时总是把主加载器放在第一位并标记 primary。
pub fn loader_of(manifest: &CfManifest) -> Option<(String, String)> {
    let loader = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|entry| entry.primary)
        .or_else(|| manifest.minecraft.mod_loaders.first())?;

    let id = loader.id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return None;
    }

    // 格式是 `<类型>-<版本>`，版本本身也可能含 `-`（如 `forge-47.2.0` 没有，
    // 但 `neoforge-20.4.100-beta` 有），所以只按第一个 `-` 切分。
    let (kind, version) = id.split_once('-')?;

    let kind = match kind.trim() {
        "forge" => "forge",
        "neoforge" => "neoforge",
        "fabric" => "fabric",
        "quilt" => "quilt",
        "optifine" => "optifine",
        _ => return None,
    };

    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    Some((kind.to_string(), version.to_string()))
}

/// 客户端需要下载的文件数（不含可选文件）。
fn default_true() -> bool {
    true
}

pub fn required_file_count(manifest: &CfManifest) -> u32 {
    manifest.files.iter().filter(|file| file.required).count() as u32
}

/// 清单里的全部文件 ID（去重）。
pub fn file_ids(manifest: &CfManifest) -> Vec<u32> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for file in &manifest.files {
        if seen.insert(file.file_id) {
            ids.push(file.file_id);
        }
    }
    ids
}

/// 清单里的全部项目 ID（去重）。
pub fn project_ids(manifest: &CfManifest) -> Vec<u32> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for file in &manifest.files {
        if seen.insert(file.project_id) {
            ids.push(file.project_id);
        }
    }
    ids
}

/// 根据项目分类决定文件落到实例的哪个目录。
///
/// CurseForge 的 `classId`：`6` = Mod，`12` = 资源包，
/// `6552` = 光影，`6945` = 数据包。其余（含未知）一律当 Mod。
pub fn target_folder(class_id: u32) -> &'static str {
    match class_id {
        12 => "resourcepacks",
        6552 => "shaderpacks",
        6945 => "datapacks",
        _ => "mods",
    }
}

/// 解析结果：一个文件应该去哪里、从哪里下载、用什么校验。
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    /// 落盘用的文件名（已去重加后缀）。
    pub file_name: String,
    /// 目标子目录（相对实例的 `minecraft/`）。
    pub folder: String,
    /// 下载地址。
    pub url: String,
    /// SHA-1 校验值；CF 的 `hashes` 里 `algo = 1` 是 SHA-1。
    pub sha1: Option<String>,
    /// 文件大小（字节）。
    pub size: Option<u64>,
}

/// 把 CF API 返回的文件与项目信息，解析成可下载的任务。
///
/// `files` 是 `POST /v1/mods/files` 的结果，`projects` 是 `POST /v1/mods` 的结果。
/// 同一个整合包里可能出现多个同名文件（例如不同项目都叫 `config.jar`），
/// 因此这里按「目录 + 文件名」去重，冲突时加 `-2`、`-3` 后缀。
pub fn resolve_files(
    manifest: &CfManifest,
    files: &[CfFile],
    projects: &[CfModBrief],
) -> Vec<ResolvedFile> {
    let class_by_project: std::collections::HashMap<u32, u32> = projects
        .iter()
        .map(|project| (project.id, project.class_id))
        .collect();
    let file_by_id: std::collections::HashMap<u32, &CfFile> =
        files.iter().map(|file| (file.id, file)).collect();

    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut resolved = Vec::new();

    for entry in &manifest.files {
        // 可选文件（`required = false`）多为服务端专用或可选组件，
        // 客户端默认不装——与 CurseForge 官方客户端的行为一致。
        if !entry.required {
            continue;
        }

        let Some(file) = file_by_id.get(&entry.file_id) else {
            // 文件已被作者下架：跳过而不是让整个整合包失败。
            tracing::warn!(
                file_id = entry.file_id,
                project_id = entry.project_id,
                "整合包引用的文件在 CurseForge 上不存在，已跳过"
            );
            continue;
        };
        let Some(url) = file.download_url.clone() else {
            tracing::warn!(
                file_id = entry.file_id,
                "整合包引用的文件没有下载地址（作者禁止第三方下载），已跳过"
            );
            continue;
        };

        let class_id = class_by_project
            .get(&entry.project_id)
            .copied()
            .unwrap_or(6);
        let folder = target_folder(class_id).to_string();

        let base = sanitize_file_name(&file.file_name);
        let file_name = unique_name(&mut used, &folder, &base);

        let sha1 = file
            .hashes
            .iter()
            .find(|hash| hash.algo == 1)
            .map(|hash| hash.value.clone())
            .filter(|value| !value.is_empty());

        resolved.push(ResolvedFile {
            file_name,
            folder,
            url,
            sha1,
            size: if file.file_length > 0 {
                Some(file.file_length)
            } else {
                None
            },
        });
    }

    resolved
}

/// 生成下载任务，落到实例的 `minecraft/<folder>/` 下。
pub fn build_file_tasks(
    resolved: &[ResolvedFile],
    instance_dir: &Path,
    prefer_mirror: bool,
) -> Vec<DownloadTask> {
    resolved
        .iter()
        .map(|item| {
            let checksum = match &item.sha1 {
                Some(sha1) => Checksum::Sha1(sha1.clone()),
                None => Checksum::None,
            };
            DownloadTask {
                id: format!("cf-{}", item.file_name),
                display_name: item.file_name.clone(),
                sources: crate::download::mirror::build_sources_with(&item.url, prefer_mirror),
                destination: instance_dir
                    .join("minecraft")
                    .join(&item.folder)
                    .join(&item.file_name),
                checksum,
                size_hint: item.size,
                priority: 4,
                retry_policy: RetryPolicy::default(),
            }
        })
        .collect()
}

/// 清理文件名：CF 的文件名来自网络，可能带路径分隔符。
///
/// 分三步：
///
/// 1. 先按路径分隔符切，只保留最后一段（`../../evil.jar` → `evil.jar`）；
/// 2. 再走通用的文件名清理（去掉非法字符、避开 Windows 保留名）；
/// 3. 最后把残留的点号序列压掉——Windows 上 `..` 虽不是保留名，
///    但 `.._.._evil.jar` 这种名字既难看又容易让人误以为是路径穿越。
fn sanitize_file_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let last = trimmed
        .rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .unwrap_or("")
        .trim_start_matches('.');

    let cleaned = crate::utils::fs::sanitize_file_name(last);

    // 把连续的点压成一个：扩展名分隔点保留，多余的点换成 `_`。
    // 这样 `jei.jar` 原样保留，而 `a..b.jar` 变成 `a.b.jar`，
    // 不会出现 `..` 这种看起来像路径穿越的片段。
    let mut out = String::with_capacity(cleaned.len());
    let mut previous_dot = false;
    for ch in cleaned.chars() {
        if ch == '.' {
            if previous_dot {
                out.push('_');
            } else {
                out.push('.');
            }
            previous_dot = true;
            continue;
        }
        out.push(ch);
        previous_dot = false;
    }

    if out.is_empty() || out == "." {
        return "unnamed".to_string();
    }
    out
}

/// 同目录同名冲突时加序号，避免互相覆盖。
fn unique_name(used: &mut std::collections::HashSet<String>, folder: &str, base: &str) -> String {
    let key = format!("{folder}/{base}");
    if used.insert(key) {
        return base.to_string();
    }

    let (stem, ext) = match base.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (base, String::new()),
    };

    for index in 2..1000u32 {
        let candidate = format!("{stem}-{index}{ext}");
        if used.insert(format!("{folder}/{candidate}")) {
            return candidate;
        }
    }

    // 理论上到不了这里；真到了就带上时间戳保证不冲突。
    let fallback = format!("{stem}-{}{ext}", std::process::id());
    used.insert(format!("{folder}/{fallback}"));
    fallback
}

/// 解压覆盖目录到实例的 `minecraft/`。
///
/// CurseForge 的 `overrides` 目录名是**可自定义**的（大多数是 `overrides`，
/// 但也有整合包用 `override` 或 `./`），因此这里不能写死前缀。
/// 服务端覆盖目录**不装**——客户端用不到。
pub fn extract_overrides(
    manifest: &CfManifest,
    pack_path: &Path,
    instance_dir: &Path,
) -> Result<usize> {
    let mut prefixes: Vec<String> = Vec::new();
    for dir in [&manifest.overrides, &manifest.client_overrides] {
        let dir = dir.trim().trim_end_matches('/');
        if dir.is_empty() || dir == "." {
            continue;
        }
        prefixes.push(format!("{dir}/"));
    }

    if prefixes.is_empty() {
        return Ok(0);
    }

    let file = std::fs::File::open(pack_path).map_err(|e| LauncherError::Io {
        message: format!("打开整合包失败 {}: {e}", pack_path.display()),
    })?;

    let borrowed: Vec<&str> = prefixes.iter().map(|prefix| prefix.as_str()).collect();
    let report = crate::utils::archive::extract_zip_prefixes_safely(
        file,
        &instance_dir.join("minecraft"),
        &borrowed,
    )?;

    Ok(report.files)
}

/// 校验一个整合包引用的相对路径是否安全（对外暴露给上层做预检）。
pub fn validate_pack_path(path: &str) -> Result<()> {
    validate_entry_path(path).map(|_| ())
}

/// CurseForge 整合包在缓存目录里的存放路径。
///
/// 整合包是 zip，直接用扩展名 `.zip`；与 Modrinth 的 `.mrpack` 分开存放，
/// 避免同名缓存互相覆盖。
pub fn cache_path(project_id: u32, file_id: u32) -> Result<std::path::PathBuf> {
    Ok(crate::utils::paths::cache_dir()?
        .join("modpacks")
        .join(format!("cf-{project_id}-{file_id}.zip")))
}

#[cfg(test)]
mod tests {

    #[test]
    fn missing_required_field_means_install() {
        let json = r#"{"files":[{"projectID":1,"fileID":2}]}"#;
        let manifest: CfManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.files[0].required, "缺省 required 应视为需要安装");
    }

    use super::*;

    const SAMPLE: &str = r#"{
        "minecraft": {
            "version": "1.20.1",
            "modLoaders": [{"id": "forge-47.2.0", "primary": true}]
        },
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": "示例整合包",
        "version": "1.0.0",
        "author": "someone",
        "files": [
            {"projectID": 238222, "fileID": 5246076, "required": true},
            {"projectID": 999999, "fileID": 111111, "required": false}
        ],
        "overrides": "overrides"
    }"#;

    #[test]
    fn parses_manifest_and_loader() {
        let manifest: CfManifest = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(manifest.name, "示例整合包");
        assert_eq!(manifest.minecraft.version, "1.20.1");
        assert_eq!(
            loader_of(&manifest),
            Some(("forge".to_string(), "47.2.0".to_string()))
        );
        assert_eq!(required_file_count(&manifest), 1);
        assert_eq!(file_ids(&manifest), vec![5246076, 111111]);
        assert_eq!(project_ids(&manifest), vec![238222, 999999]);
    }

    #[test]
    fn vanilla_pack_has_no_loader() {
        let manifest: CfManifest = serde_json::from_str(
            r#"{"minecraft":{"version":"1.21.1","modLoaders":[]},"files":[]}"#,
        )
        .unwrap();
        assert_eq!(loader_of(&manifest), None);
    }

    #[test]
    fn loader_id_with_extra_dash_is_split_once() {
        let manifest: CfManifest = serde_json::from_str(
            r#"{"minecraft":{"version":"1.21.1","modLoaders":[{"id":"neoforge-21.1.65-beta"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            loader_of(&manifest),
            Some(("neoforge".to_string(), "21.1.65-beta".to_string()))
        );
    }

    #[test]
    fn target_folder_follows_class_id() {
        assert_eq!(target_folder(6), "mods");
        assert_eq!(target_folder(12), "resourcepacks");
        assert_eq!(target_folder(6552), "shaderpacks");
        assert_eq!(target_folder(6945), "datapacks");
        assert_eq!(target_folder(0), "mods");
    }

    #[test]
    fn optional_files_are_skipped_and_missing_files_do_not_fail() {
        let manifest: CfManifest = serde_json::from_str(SAMPLE).unwrap();
        let files = vec![CfFile {
            id: 5246076,
            display_name: "JEI".into(),
            file_name: "jei.jar".into(),
            download_url: Some("https://example.com/jei.jar".into()),
            download_count: 0,
            game_versions: vec![],
            release_type: 1,
            file_date: None,
            dependencies: vec![],
            hashes: vec![crate::models::resource::CfHash {
                value: "abc123".into(),
                algo: 1,
            }],
            file_length: 1024,
            modules: vec![],
        }];

        let resolved = resolve_files(&manifest, &files, &[]);
        assert_eq!(resolved.len(), 1, "可选文件与不存在的文件都不该出现");
        assert_eq!(resolved[0].file_name, "jei.jar");
        assert_eq!(resolved[0].folder, "mods");
        assert_eq!(resolved[0].sha1.as_deref(), Some("abc123"));
        assert_eq!(resolved[0].size, Some(1024));
    }

    #[test]
    fn duplicate_file_names_get_suffix() {
        let manifest = CfManifest {
            files: vec![
                CfManifestFile {
                    project_id: 1,
                    file_id: 10,
                    required: true,
                },
                CfManifestFile {
                    project_id: 2,
                    file_id: 11,
                    required: true,
                },
            ],
            ..Default::default()
        };
        let mk = |id: u32| CfFile {
            id,
            display_name: "x".into(),
            file_name: "same.jar".into(),
            download_url: Some("https://example.com/a.jar".into()),
            download_count: 0,
            game_versions: vec![],
            release_type: 1,
            file_date: None,
            dependencies: vec![],
            hashes: vec![],
            file_length: 1,
            modules: vec![],
        };
        let files = vec![mk(10), mk(11)];
        let resolved = resolve_files(&manifest, &files, &[]);
        assert_eq!(resolved[0].file_name, "same.jar");
        assert_eq!(resolved[1].file_name, "same-2.jar");
    }

    #[test]
    fn network_file_names_cannot_escape_directory() {
        let manifest = CfManifest {
            files: vec![CfManifestFile {
                project_id: 1,
                file_id: 10,
                required: true,
            }],
            ..Default::default()
        };
        let files = vec![CfFile {
            id: 10,
            display_name: "x".into(),
            file_name: "../../evil.jar".into(),
            download_url: Some("https://example.com/a.jar".into()),
            download_count: 0,
            game_versions: vec![],
            release_type: 1,
            file_date: None,
            dependencies: vec![],
            hashes: vec![],
            file_length: 1,
            modules: vec![],
        }];
        let resolved = resolve_files(&manifest, &files, &[]);
        let name = &resolved[0].file_name;
        assert!(!name.contains('/'), "文件名不能含路径分隔符: {name}");
        assert!(!name.contains(".."), "文件名不能含 ..: {name}");
    }
}
