//! Modrinth 整合包（`.mrpack`）的解析与安装素材生成。
//!
//! `.mrpack` 是一个 zip，内含：
//!
//! - `modrinth.index.json`：整合包索引，声明游戏版本、加载器、要下载的文件；
//! - `overrides/`：直接覆盖到游戏目录的文件（配置、脚本等）；
//! - `client-overrides/`：仅客户端生效的覆盖文件。
//!
//! MOD 本体不在包里，而是由索引里的 `downloads` 指向 Modrinth CDN，
//! 由启动器自己下载并校验 SHA-1。

pub mod install;

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::LauncherError;
use crate::Result;
use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::utils::archive::validate_entry_path;

/// 整合包索引（`modrinth.index.json`）。
#[derive(Debug, Clone, Deserialize)]
pub struct ModpackIndex {
    /// 整合包名称。
    #[serde(default)]
    pub name: String,
    /// 整合包自身的版本号。
    #[serde(default)]
    pub version_id: String,
    /// 需要下载到游戏目录的文件。
    #[serde(default)]
    pub files: Vec<ModpackFile>,
    /// 覆盖文件（直接从包内解压）。
    #[serde(default)]
    pub overrides: Vec<ModpackFile>,
    /// 依赖（游戏版本与加载器）。
    #[serde(default)]
    pub dependencies: ModpackDependencies,
}

/// 索引里的一个文件条目。
#[derive(Debug, Clone, Deserialize)]
pub struct ModpackFile {
    /// 相对游戏目录的路径，例如 `mods/sodium.jar`。
    pub path: String,
    #[serde(default)]
    pub hashes: ModpackHashes,
    /// 下载地址（第一个为主源）。
    #[serde(default)]
    pub downloads: Vec<String>,
    /// 文件大小（字节），用于进度统计。
    #[serde(rename = "fileSize", default)]
    pub file_size: Option<u64>,
    /// 端侧要求：`required` / `optional` / `unsupported`。
    #[serde(default)]
    pub env: Option<ModpackEnv>,
}

/// 文件哈希。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModpackHashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

/// 端侧环境要求。
#[derive(Debug, Clone, Deserialize)]
pub struct ModpackEnv {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

/// 整合包依赖。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModpackDependencies {
    /// 要求的 Minecraft 版本。
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub forge: Option<String>,
    #[serde(default)]
    pub neoforge: Option<String>,
    #[serde(rename = "fabric-loader", default)]
    pub fabric_loader: Option<String>,
    #[serde(rename = "quilt-loader", default)]
    pub quilt_loader: Option<String>,
}

/// 读取 `.mrpack` 里的索引。
pub fn read_index(pack_path: &Path) -> Result<ModpackIndex> {
    let file = std::fs::File::open(pack_path).map_err(|e| LauncherError::Io {
        message: format!("打开整合包失败 {}: {e}", pack_path.display()),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| LauncherError::Parse {
        message: format!("解析整合包失败 {}: {e}", pack_path.display()),
    })?;

    let mut text = String::new();
    archive
        .by_name("modrinth.index.json")
        .map_err(|_| LauncherError::Parse {
            message: "整合包缺少 modrinth.index.json，可能不是有效的 .mrpack".into(),
        })?
        .read_to_string(&mut text)
        .map_err(|e| LauncherError::Parse {
            message: format!("读取整合包索引失败: {e}"),
        })?;

    serde_json::from_str(&text).map_err(|e| LauncherError::Parse {
        message: format!("解析整合包索引失败: {e}"),
    })
}

/// 整合包要求的加载器：`(类型, 版本)`；原版整合包返回 `None`。
pub fn loader_of(index: &ModpackIndex) -> Option<(String, String)> {
    let deps = &index.dependencies;
    if let Some(version) = deps.fabric_loader.as_ref().filter(|v| !v.is_empty()) {
        return Some(("fabric".to_string(), version.clone()));
    }
    if let Some(version) = deps.quilt_loader.as_ref().filter(|v| !v.is_empty()) {
        return Some(("quilt".to_string(), version.clone()));
    }
    if let Some(version) = deps.neoforge.as_ref().filter(|v| !v.is_empty()) {
        return Some(("neoforge".to_string(), version.clone()));
    }
    if let Some(version) = deps.forge.as_ref().filter(|v| !v.is_empty()) {
        return Some(("forge".to_string(), version.clone()));
    }
    None
}

/// 生成整合包文件的下载任务，落到实例的 `minecraft` 目录。
///
/// 路径来自网络，必须过一遍归档路径校验，否则 `../` 可以写到实例目录之外。
pub fn build_file_tasks(
    index: &ModpackIndex,
    instance_dir: &Path,
    prefer_mirror: bool,
) -> Vec<DownloadTask> {
    let mut tasks = Vec::new();

    for file in &index.files {
        if !client_supported(file) {
            continue;
        }
        let Some(url) = file.downloads.first() else {
            continue;
        };
        let Ok(relative) = validate_entry_path(&file.path) else {
            tracing::warn!("整合包文件路径非法，已跳过: {}", file.path);
            continue;
        };

        let checksum = match file.hashes.sha1.as_ref().filter(|s| !s.is_empty()) {
            Some(sha1) => Checksum::Sha1(sha1.clone()),
            None => Checksum::None,
        };

        tasks.push(DownloadTask {
            id: format!("modpack-{}", relative.to_string_lossy()),
            display_name: relative
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| relative.to_string_lossy().to_string()),
            sources: crate::download::mirror::build_sources_with(url, prefer_mirror),
            destination: instance_dir.join("minecraft").join(&relative),
            checksum,
            size_hint: file.file_size,
            priority: 4,
            retry_policy: RetryPolicy::default(),
        });
    }

    tasks
}

/// 该文件是否需要装到客户端。
fn client_supported(file: &ModpackFile) -> bool {
    !matches!(
        file.env.as_ref().and_then(|env| env.client.as_deref()),
        Some("unsupported")
    )
}

/// 解压 `overrides/` 与 `client-overrides/` 到实例的 `minecraft` 目录。
pub fn extract_overrides(pack_path: &Path, instance_dir: &Path) -> Result<usize> {
    let file = std::fs::File::open(pack_path).map_err(|e| LauncherError::Io {
        message: format!("打开整合包失败 {}: {e}", pack_path.display()),
    })?;

    let report = crate::utils::archive::extract_zip_prefixes_safely(
        file,
        &instance_dir.join("minecraft"),
        &["overrides/", "client-overrides/"],
    )?;

    Ok(report.files)
}

/// 整合包在缓存目录里的存放路径。
pub fn cache_path(project_id: &str, version_id: &str) -> Result<PathBuf> {
    let stem = format!("{}-{}", safe_stem(project_id), safe_stem(version_id));
    Ok(crate::utils::paths::cache_dir()?
        .join("modpacks")
        .join(format!("{stem}.mrpack")))
}

/// 把网络来的 ID 收敛成安全的文件名片段。
fn safe_stem(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

/// 客户端需要下载的文件数。
pub fn file_count(index: &ModpackIndex) -> u32 {
    index
        .files
        .iter()
        .filter(|file| client_supported(file))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_json() -> &'static str {
        r#"{
            "name": "测试整合包",
            "versionId": "1.0.0",
            "files": [
                {
                    "path": "mods/example.jar",
                    "hashes": {"sha1": "abc", "sha512": "def"},
                    "downloads": ["https://cdn.modrinth.com/data/example/1/example.jar"],
                    "env": {"client": "required", "server": "unsupported"}
                },
                {
                    "path": "mods/server-only.jar",
                    "hashes": {"sha1": "123"},
                    "downloads": ["https://cdn.modrinth.com/data/x/1/x.jar"],
                    "env": {"client": "unsupported"}
                }
            ],
            "dependencies": {"minecraft": "1.21.1", "neoforge": "21.1.65"}
        }"#
    }

    #[test]
    fn parses_index_and_loader() {
        let index: ModpackIndex = serde_json::from_str(index_json()).unwrap();
        assert_eq!(index.name, "测试整合包");
        assert_eq!(index.dependencies.minecraft, "1.21.1");
        assert_eq!(
            loader_of(&index),
            Some(("neoforge".to_string(), "21.1.65".to_string()))
        );
    }

    #[test]
    fn client_unsupported_files_are_skipped() {
        let index: ModpackIndex = serde_json::from_str(index_json()).unwrap();
        assert_eq!(file_count(&index), 1);
    }

    #[test]
    fn rejects_paths_outside_game_dir() {
        let index = ModpackIndex {
            name: String::new(),
            version_id: String::new(),
            files: vec![ModpackFile {
                path: "../evil.jar".to_string(),
                hashes: ModpackHashes::default(),
                downloads: vec!["https://example.com/a.jar".to_string()],
                file_size: None,
                env: None,
            }],
            overrides: Vec::new(),
            dependencies: ModpackDependencies::default(),
        };

        let tasks = build_file_tasks(&index, Path::new("/tmp/instance"), false);
        assert!(tasks.is_empty(), "路径穿越必须被拒绝");
    }

    #[test]
    fn cache_path_is_sanitized() {
        let path = cache_path("../../evil", "a/b").unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains('/'), "文件名不能包含路径分隔符: {name}");
        assert!(!name.contains(".."), "文件名不能包含 ..: {name}");
    }

    /// 用真实的 zip 结构验证索引解析与覆盖文件落盘。
    #[test]
    fn extracts_overrides_into_game_dir() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let dir = std::env::temp_dir().join("sereno-modpack-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pack = dir.join("pack.mrpack");

        {
            let file = std::fs::File::create(&pack).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = SimpleFileOptions::default();

            writer.start_file("modrinth.index.json", options).unwrap();
            writer
                .write_all(br#"{"name":"t","versionId":"1","files":[],"dependencies":{"minecraft":"1.21.1"}}"#)
                .unwrap();
            writer
                .start_file("overrides/config/a.toml", options)
                .unwrap();
            writer.write_all(b"key = 1").unwrap();
            writer
                .start_file("client-overrides/config/b.toml", options)
                .unwrap();
            writer.write_all(b"key = 2").unwrap();
            // 服务端覆盖文件不应落到客户端
            writer
                .start_file("server-overrides/secret.toml", options)
                .unwrap();
            writer.write_all(b"nope").unwrap();
            writer.finish().unwrap();
        }

        let index = read_index(&pack).unwrap();
        assert_eq!(index.name, "t");
        assert_eq!(index.dependencies.minecraft, "1.21.1");

        let instance = dir.join("instance");
        let count = extract_overrides(&pack, &instance).unwrap();
        assert_eq!(count, 2, "只应写入 overrides 与 client-overrides");
        assert!(instance.join("minecraft/config/a.toml").exists());
        assert!(instance.join("minecraft/config/b.toml").exists());
        assert!(!instance.join("minecraft/secret.toml").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
