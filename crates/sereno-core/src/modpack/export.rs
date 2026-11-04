//! 实例导出为 Modrinth 整合包（`.mrpack`）。
//!
//! ## 导出的是什么
//!
//! `.mrpack` 里有两类内容：
//!
//! - **索引条目**（`modrinth.index.json` 的 `files`）：能追溯到 Modrinth 的 Mod，
//!   只写「项目 + 版本 + 哈希 + 下载地址」，文件本体不打包——
//!   别人装的时候从 Modrinth CDN 拉；
//! - **覆盖文件**（`overrides/`）：追不到来源的东西——本地脚本、
//!   改过的配置、CurseForge 独占的 Mod——直接打进包里。
//!
//! 这与《实例生命周期与布局》第 8 节的约定一致：
//! **不导出** `saves/`、`logs/`、`crash-reports/`——它们是个人数据与诊断材料，
//! 不是实例内容的一部分。

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::models::instance::InstanceMetadata;
use crate::models::settings::AppSettings;
use crate::{LauncherError, Result};

/// Modrinth 的版本文件查询接口。
const MODRINTH_API: &str = "https://api.modrinth.com/v2";

/// 批量查询文件时的分片大小。
const HASH_BATCH: usize = 100;

/// **不导出**的目录：个人数据与诊断材料。
const EXCLUDED_DIRS: &[&str] = &[
    "saves",
    "logs",
    "crash-reports",
    "screenshots",
    "assets",
    "natives",
    "library",
];

/// 导出结果。
#[derive(Debug, Clone)]
pub struct ModpackExport {
    /// `modrinth.index.json` 的内容。
    pub index: MrpackIndex,
    /// 覆盖文件：`(包内相对路径, 源文件绝对路径)`。
    pub overrides: Vec<(String, PathBuf)>,
    /// 有多少个 Mod 追溯到了 Modrinth（其余进了覆盖目录）。
    pub linked_mods: u32,
    /// 有多少个 Mod 追不到来源（进了覆盖目录）。
    pub embedded_mods: u32,
}

/// `.mrpack` 的索引结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrpackIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub dependencies: MrpackDependencies,
    #[serde(default)]
    pub files: Vec<MrpackFile>,
}

/// 索引里的依赖声明。
///
/// **键名是 kebab-case**（`fabric-loader` / `quilt-loader`），
/// 与 `.mrpack` 规范一致——这里不能用 `rename_all = "camelCase"`，
/// 否则别的启动器读不出加载器版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrpackDependencies {
    pub minecraft: String,
    #[serde(
        rename = "fabric-loader",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fabric_loader: Option<String>,
    #[serde(
        rename = "quilt-loader",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub quilt_loader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neoforge: Option<String>,
}

/// 索引里的一个文件条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrpackFile {
    /// 相对游戏目录的路径，如 `mods/sodium.jar`。
    pub path: String,
    pub hashes: MrpackHashes,
    pub env: MrpackEnv,
    pub downloads: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
}

/// 条目的哈希。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrpackHashes {
    pub sha1: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
}

/// 条目的适用端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrpackEnv {
    pub client: String,
    pub server: String,
}

/// Modrinth 的版本文件查询结果。
#[derive(Debug, Clone, Deserialize)]
struct MrVersion {
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
    #[serde(default)]
    sha1: Option<String>,
    #[serde(default)]
    sha512: Option<String>,
}

/// 一个待导出的本地 Mod。
#[derive(Debug, Clone)]
struct LocalMod {
    file_name: String,
    path: PathBuf,
    sha1: String,
}

/// 构建导出内容。
///
/// **不写盘**：先算出「索引 + 覆盖文件清单」，让调用方确认后再落盘——
/// 导出可能涉及几百个文件，失败要能重来。
pub async fn build(
    settings: &AppSettings,
    client: &reqwest::Client,
    instance: &InstanceMetadata,
) -> Result<ModpackExport> {
    let inst_dir = crate::utils::paths::instance_dir(settings, &instance.id)?;
    let game_dir = inst_dir.join("minecraft");

    // 1. 收集本地 Mod（mods/ 下启用的文件）
    let mods = collect_mods(&game_dir.join("mods"))?;

    // 2. 用 SHA-1 反查 Modrinth，看哪些能追溯到来源
    let resolved = resolve_on_modrinth(client, &mods).await;

    // 3. 分类：能追溯的写进索引条目，追溯不到的进覆盖目录
    let mut files = Vec::new();
    let mut overrides: Vec<(String, PathBuf)> = Vec::new();
    let mut linked = 0u32;
    let mut embedded = 0u32;

    for mod_entry in &mods {
        let Some(version) = resolved.get(&mod_entry.sha1) else {
            overrides.push((
                format!("mods/{}", mod_entry.file_name),
                mod_entry.path.clone(),
            ));
            embedded += 1;
            continue;
        };

        // 优先选文件名对得上的文件，否则取 primary
        let file = version
            .files
            .iter()
            .find(|file| file.filename == mod_entry.file_name)
            .or_else(|| version.files.iter().find(|file| file.primary))
            .or_else(|| version.files.first());

        let Some(file) = file else {
            overrides.push((
                format!("mods/{}", mod_entry.file_name),
                mod_entry.path.clone(),
            ));
            embedded += 1;
            continue;
        };

        if file.url.is_empty() {
            overrides.push((
                format!("mods/{}", mod_entry.file_name),
                mod_entry.path.clone(),
            ));
            embedded += 1;
            continue;
        }

        files.push(MrpackFile {
            path: format!("mods/{}", mod_entry.file_name),
            hashes: MrpackHashes {
                sha1: file
                    .hashes
                    .sha1
                    .clone()
                    .unwrap_or_else(|| mod_entry.sha1.clone()),
                sha512: file.hashes.sha512.clone(),
            },
            env: MrpackEnv {
                client: "required".to_string(),
                server: "unsupported".to_string(),
            },
            downloads: vec![file.url.clone()],
            file_size: std::fs::metadata(&mod_entry.path).ok().map(|m| m.len()),
        });
        linked += 1;
    }

    // 4. 其余游戏目录内容进覆盖目录
    collect_overrides(&game_dir, &mut overrides)?;

    // 5. 组装索引
    let index = MrpackIndex {
        format_version: 1,
        game: "minecraft".to_string(),
        version_id: instance.id.clone(),
        name: instance.display_name.clone(),
        summary: Some(format!(
            "{} · {}",
            instance.game_version, instance.loader.r#type
        )),
        dependencies: dependencies_of(instance),
        files,
    };

    Ok(ModpackExport {
        index,
        overrides,
        linked_mods: linked,
        embedded_mods: embedded,
    })
}

/// 依赖声明：游戏版本 + 加载器。
fn dependencies_of(instance: &InstanceMetadata) -> MrpackDependencies {
    let version = instance.loader.version.clone();
    match instance.loader.r#type.as_str() {
        "fabric" => MrpackDependencies {
            minecraft: instance.game_version.clone(),
            fabric_loader: version,
            quilt_loader: None,
            forge: None,
            neoforge: None,
        },
        "quilt" => MrpackDependencies {
            minecraft: instance.game_version.clone(),
            fabric_loader: None,
            quilt_loader: version,
            forge: None,
            neoforge: None,
        },
        "forge" => MrpackDependencies {
            minecraft: instance.game_version.clone(),
            fabric_loader: None,
            quilt_loader: None,
            forge: version,
            neoforge: None,
        },
        "neoforge" => MrpackDependencies {
            minecraft: instance.game_version.clone(),
            fabric_loader: None,
            quilt_loader: None,
            forge: None,
            neoforge: version,
        },
        _ => MrpackDependencies {
            minecraft: instance.game_version.clone(),
            fabric_loader: None,
            quilt_loader: None,
            forge: None,
            neoforge: None,
        },
    }
}

/// 收集 `mods/` 下启用的 Mod。
fn collect_mods(folder: &Path) -> Result<Vec<LocalMod>> {
    let mut mods = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Ok(mods);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(raw_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // 禁用的 Mod（`.jar.disabled`）不导出
        if raw_name.to_ascii_lowercase().ends_with(".disabled") {
            continue;
        }
        let lower = raw_name.to_ascii_lowercase();
        if !lower.ends_with(".jar") && !lower.ends_with(".zip") && !lower.ends_with(".litemod") {
            continue;
        }

        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let sha1 = {
            use sha1::{Digest, Sha1};
            format!("{:x}", Sha1::digest(&bytes))
        };

        mods.push(LocalMod {
            file_name: raw_name.to_string(),
            path,
            sha1,
        });
    }

    mods.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(mods)
}

/// 用 SHA-1 批量反查 Modrinth。
async fn resolve_on_modrinth(
    client: &reqwest::Client,
    mods: &[LocalMod],
) -> std::collections::HashMap<String, MrVersion> {
    let mut result = std::collections::HashMap::new();
    if mods.is_empty() {
        return result;
    }

    for chunk in mods.chunks(HASH_BATCH) {
        let hashes: Vec<&str> = chunk.iter().map(|m| m.sha1.as_str()).collect();
        let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });

        let response = match client
            .post(format!("{MODRINTH_API}/version_files"))
            .json(&body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(%error, "Modrinth 版本文件查询失败，导出时相关 Mod 将打进覆盖目录");
                break;
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                status = %response.status(),
                "Modrinth 版本文件查询未成功，导出时相关 Mod 将打进覆盖目录"
            );
            break;
        }

        let data: std::collections::HashMap<String, MrVersion> = match response.json().await {
            Ok(data) => data,
            Err(error) => {
                tracing::warn!(%error, "解析 Modrinth 版本文件结果失败");
                break;
            }
        };
        result.extend(data);
    }

    result
}

/// 收集游戏目录里除 Mod 之外、值得打包的内容。
fn collect_overrides(game_dir: &Path, overrides: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(game_dir) else {
        return Ok(());
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // `mods/` 由索引条目负责，不重复进覆盖目录
        if name.eq_ignore_ascii_case("mods") {
            continue;
        }

        if path.is_dir() {
            if EXCLUDED_DIRS
                .iter()
                .any(|dir| dir.eq_ignore_ascii_case(name))
            {
                continue;
            }
            collect_dir(&path, name, overrides)?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        // 散装文件：只收配置类的
        let lower = name.to_ascii_lowercase();
        let interesting = lower.ends_with(".txt")
            || lower.ends_with(".json")
            || lower.ends_with(".properties")
            || lower.ends_with(".toml")
            || lower.ends_with(".dat");
        if !interesting {
            continue;
        }
        // `options.txt` 与 `servers.dat` 属于个人偏好，但也常被整合包携带，因此保留
        overrides.push((name.to_string(), path.clone()));
    }

    Ok(())
}

/// 递归收集目录内容。
fn collect_dir(dir: &Path, prefix: &str, overrides: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let relative = format!("{prefix}/{name}");

        if path.is_dir() {
            collect_dir(&path, &relative, overrides)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        overrides.push((relative, path.clone()));
    }

    Ok(())
}

/// 把导出内容写成 `.mrpack`（zip）。
///
/// **路径来源不可信**：覆盖文件的相对路径来自磁盘扫描，
/// 写入 zip 前统一过一遍路径校验，避免写出带 `..` 的条目。
pub fn write_mrpack(export: &ModpackExport, dest: &Path) -> Result<()> {
    use zip::write::SimpleFileOptions;

    if let Some(parent) = dest.parent() {
        crate::utils::paths::ensure_dir(parent)?;
    }

    let file = std::fs::File::create(dest).map_err(|e| LauncherError::Io {
        message: format!("创建整合包失败 {}: {e}", dest.display()),
    })?;
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    // 索引
    let index_json =
        serde_json::to_vec_pretty(&export.index).map_err(|e| LauncherError::Parse {
            message: format!("序列化整合包索引失败: {e}"),
        })?;
    writer
        .start_file("modrinth.index.json", options)
        .map_err(|e| LauncherError::Io {
            message: format!("写入整合包索引失败: {e}"),
        })?;
    writer
        .write_all(&index_json)
        .map_err(|e| LauncherError::Io {
            message: format!("写入整合包索引失败: {e}"),
        })?;

    // 覆盖文件
    for (relative, source) in &export.overrides {
        let safe = crate::utils::archive::validate_entry_path(relative)?;
        let name = safe.to_string_lossy().replace('\\', "/");

        let bytes = match std::fs::read(source) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(path = %source.display(), %error, "读取覆盖文件失败，已跳过");
                continue;
            }
        };

        writer
            .start_file(format!("overrides/{name}"), options)
            .map_err(|e| LauncherError::Io {
                message: format!("写入覆盖文件失败: {e}"),
            })?;
        writer.write_all(&bytes).map_err(|e| LauncherError::Io {
            message: format!("写入覆盖文件失败: {e}"),
        })?;
    }

    writer.finish().map_err(|e| LauncherError::Io {
        message: format!("完成整合包写入失败: {e}"),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance() -> InstanceMetadata {
        InstanceMetadata {
            schema_version: 1,
            id: "inst-test".to_string(),
            display_name: "测试实例".to_string(),
            game_version: "1.21.1".to_string(),
            loader: crate::models::instance::LoaderInfo {
                r#type: "fabric".to_string(),
                version: Some("0.16.9".to_string()),
            },
            status: "ready".to_string(),
            created_at: "0".to_string(),
            last_played_at: None,
            playtime_secs: 0,
            java_path: None,
            memory_mb: None,
            jvm_args: Vec::new(),
            favorite: false,
            group: None,
        }
    }

    #[test]
    fn dependencies_reflect_loader() {
        let deps = dependencies_of(&instance());
        assert_eq!(deps.minecraft, "1.21.1");
        assert_eq!(deps.fabric_loader.as_deref(), Some("0.16.9"));
        assert!(deps.forge.is_none());

        let mut forge = instance();
        forge.loader.r#type = "forge".to_string();
        let deps = dependencies_of(&forge);
        assert_eq!(deps.forge.as_deref(), Some("0.16.9"));
        assert!(deps.fabric_loader.is_none());
    }

    #[test]
    fn index_serializes_with_camel_case_keys() {
        let export = ModpackExport {
            index: MrpackIndex {
                format_version: 1,
                game: "minecraft".to_string(),
                version_id: "v1".to_string(),
                name: "测试".to_string(),
                summary: None,
                dependencies: dependencies_of(&instance()),
                files: vec![MrpackFile {
                    path: "mods/a.jar".to_string(),
                    hashes: MrpackHashes {
                        sha1: "abc".to_string(),
                        sha512: None,
                    },
                    env: MrpackEnv {
                        client: "required".to_string(),
                        server: "unsupported".to_string(),
                    },
                    downloads: vec!["https://example.com/a.jar".to_string()],
                    file_size: Some(1),
                }],
            },
            overrides: Vec::new(),
            linked_mods: 1,
            embedded_mods: 0,
        };

        let json = serde_json::to_string(&export.index).unwrap();
        assert!(json.contains("\"formatVersion\":1"), "{json}");
        assert!(json.contains("\"fileSize\":1"), "{json}");
        assert!(json.contains("\"fabric-loader\""), "{json}");
        assert!(json.contains("\"mods/a.jar\""), "{json}");
    }

    #[test]
    fn collects_enabled_mods_only() {
        let dir = std::env::temp_dir().join(format!("sereno-export-mods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jar"), "jar").unwrap();
        std::fs::write(dir.join("b.jar.disabled"), "off").unwrap();
        std::fs::write(dir.join("readme.txt"), "x").unwrap();

        let mods = collect_mods(&dir).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].file_name, "a.jar");
        assert_eq!(mods[0].sha1.len(), 40);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn excludes_personal_data_from_overrides() {
        let dir = std::env::temp_dir().join(format!("sereno-export-over-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::create_dir_all(dir.join("saves").join("World")).unwrap();
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        std::fs::write(dir.join("config").join("a.toml"), "x").unwrap();
        std::fs::write(dir.join("saves").join("World").join("level.dat"), "x").unwrap();
        std::fs::write(dir.join("options.txt"), "x").unwrap();

        let mut overrides = Vec::new();
        collect_overrides(&dir, &mut overrides).unwrap();

        let names: Vec<&str> = overrides.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"config/a.toml"), "{names:?}");
        assert!(names.contains(&"options.txt"), "{names:?}");
        assert!(
            !names.iter().any(|name| name.starts_with("saves")),
            "存档不该被导出：{names:?}"
        );
        assert!(
            !names.iter().any(|name| name.starts_with("mods")),
            "Mod 由索引负责：{names:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 写出的 `.mrpack` 必须能被标准 zip 读回，且含索引与覆盖文件。
    #[test]
    fn writes_readable_mrpack() {
        let dir = std::env::temp_dir().join(format!("sereno-export-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let override_source = dir.join("a.toml");
        std::fs::write(&override_source, "key = 1").unwrap();

        let export = ModpackExport {
            index: MrpackIndex {
                format_version: 1,
                game: "minecraft".to_string(),
                version_id: "v1".to_string(),
                name: "测试".to_string(),
                summary: None,
                dependencies: dependencies_of(&instance()),
                files: Vec::new(),
            },
            overrides: vec![("config/a.toml".to_string(), override_source)],
            linked_mods: 0,
            embedded_mods: 0,
        };

        let dest = dir.join("out.mrpack");
        write_mrpack(&export, &dest).unwrap();

        let file = std::fs::File::open(&dest).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("modrinth.index.json").is_ok());
        assert!(archive.by_name("overrides/config/a.toml").is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 覆盖文件路径带 `..` 时必须被拒绝，不能写进包里。
    #[test]
    fn rejects_path_traversal_in_override_paths() {
        let dir = std::env::temp_dir().join(format!("sereno-export-esc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("a.toml");
        std::fs::write(&source, "x").unwrap();

        let export = ModpackExport {
            index: MrpackIndex {
                format_version: 1,
                game: "minecraft".to_string(),
                version_id: "v1".to_string(),
                name: "测试".to_string(),
                summary: None,
                dependencies: dependencies_of(&instance()),
                files: Vec::new(),
            },
            overrides: vec![("../evil.toml".to_string(), source)],
            linked_mods: 0,
            embedded_mods: 0,
        };

        let dest = dir.join("out.mrpack");
        assert!(write_mrpack(&export, &dest).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
