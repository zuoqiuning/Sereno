//! 从外部目录导入已有实例。
//!
//! ## 要解决的场景
//!
//! 用户已经有游戏在别的启动器里（PCL、HMCL、MultiMC、Prism、官方启动器），
//! 现在想转到 Sereno。**最值钱的不是游戏本体，而是那些 Mod、配置与存档**——
//! 游戏本体与库文件可以用 Sereno 自己的下载器重新拉一遍（更快、更准、
//! 还能用上镜像），用户真正搬不走的是 `mods/`、`config/`、`saves/`。
//!
//! 因此导入采取**「重建骨架 + 搬运用户数据」**的策略：
//!
//! 1. 扫描外部目录，找出候选的游戏版本与加载器；
//! 2. 用户选定一个后，新建 Sereno 实例（骨架由 Sereno 自己管理）；
//! 3. 把 `mods/` / `config/` / `saves/` 等**用户数据**复制过去；
//! 4. 游戏本体、库文件、资源对象走 Sereno 自己的安装流程重新下载。
//!
//! ## 支持的目录布局
//!
//! | 布局 | 特征 | 游戏目录 |
//! | :--- | :--- | :--- |
//! | 官方启动器 / PCL / HMCL | 有 `versions/` | 目录本身 |
//! | MultiMC / Prism | 有 `.minecraft/` | `.minecraft/` 子目录 |
//! | 裸游戏目录 | 有 `mods/` 或 `saves/` | 目录本身 |

use std::path::{Path, PathBuf};

use crate::models::instance::LoaderInfo;
use crate::{LauncherError, Result};

/// 扫描到的外部布局类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportLayout {
    /// 官方启动器 / PCL / HMCL：有 `versions/` 目录。
    Flat,
    /// MultiMC / Prism：游戏目录在 `.minecraft/` 子目录里。
    Nested,
    /// 裸游戏目录：只有 `mods/` / `saves/` 等，没有版本信息。
    GameDirOnly,
}

/// 一个可导入的候选实例。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    /// 展示名（来自目录名或版本 ID）。
    pub label: String,
    /// Minecraft 版本。
    pub game_version: String,
    /// 加载器信息。
    pub loader: LoaderInfo,
    /// 游戏目录（`--gameDir` 对应的那一层）。
    pub game_dir: String,
    /// 目录布局类型。
    pub layout: ImportLayout,
    /// `mods/` 里的文件数。
    pub mod_count: u32,
    /// `saves/` 里的世界数。
    pub save_count: u32,
}

/// 一次扫描的结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportScan {
    /// 扫描的根目录。
    pub source: String,
    /// 识别出的布局。
    pub layout: ImportLayout,
    /// 游戏目录。
    pub game_dir: String,
    /// 候选版本列表（按「有加载器优先、版本号新优先」排序）。
    pub candidates: Vec<ImportCandidate>,
}

/// 会被搬运的用户数据目录。
///
/// **刻意不包含** `libraries/`、`versions/`、`assets/`——它们由 Sereno
/// 自己重新下载，比复制更快也更可信。
const COPY_DIRS: &[&str] = &[
    "mods",
    "config",
    "resourcepacks",
    "shaderpacks",
    "datapacks",
    "saves",
    "screenshots",
    "defaultconfigs",
    "kubejs",
    "scripts",
    "schematics",
    "stats", // 玩家统计是目录（stats/*.json），此前误放在文件列表里永远不会被复制
];

/// 会被搬运的用户数据文件（精确匹配）。
const COPY_FILES: &[&str] = &[
    "options.txt",
    "servers.dat",
    "usercache.json",
    "banned-players.json",
    "banned-ips.json",
    "whitelist.json",
];

/// 会被搬运的用户数据文件（按扩展名匹配）。
const COPY_EXTENSIONS: &[&str] = &["properties", "mcdump"];

/// 单次导入复制的字节上限（8 GB）。
///
/// 存档可能很大，但超过 8 GB 说明用户可能选错了目录
/// （比如整个 `.minecraft` 里塞了多个版本的资源对象）。
/// 超限时明确报错，而不是把磁盘写满。
const MAX_COPY_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// 扫描外部目录，找出可导入的候选。
pub fn scan(source: &Path) -> Result<ImportScan> {
    if !source.is_dir() {
        return Err(LauncherError::Io {
            message: format!("不是有效的目录: {}", source.display()),
        });
    }

    let (layout, game_dir) = detect_layout(source);

    let mut candidates = Vec::new();
    match layout {
        ImportLayout::Flat => {
            collect_from_versions(&game_dir, source, &mut candidates);
        }
        ImportLayout::Nested => {
            // MultiMC / Prism：版本信息在实例根的 `mmc-pack.json` 里，
            // 游戏目录在 `.minecraft/` 下。
            collect_from_mmc(source, &game_dir, &mut candidates);
        }
        ImportLayout::GameDirOnly => {}
    }

    // 没有版本信息时，仍然可以导入「纯用户数据」——
    // 用户可能只想把存档搬过来，游戏版本另建。
    if candidates.is_empty() {
        candidates.push(ImportCandidate {
            label: source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "导入的实例".to_string()),
            game_version: String::new(),
            loader: LoaderInfo {
                r#type: "vanilla".to_string(),
                version: None,
            },
            layout,
            game_dir: game_dir.to_string_lossy().to_string(),
            mod_count: count_files(&game_dir.join("mods")),
            save_count: count_dirs(&game_dir.join("saves")),
        });
    }

    sort_candidates(&mut candidates);

    Ok(ImportScan {
        source: source.to_string_lossy().to_string(),
        layout,
        game_dir: game_dir.to_string_lossy().to_string(),
        candidates,
    })
}

/// 判断目录布局，并返回游戏目录。
fn detect_layout(source: &Path) -> (ImportLayout, PathBuf) {
    // MultiMC / Prism：实例目录下有 `.minecraft/`
    let nested = source.join(".minecraft");
    if nested.is_dir() {
        return (ImportLayout::Nested, nested);
    }

    // 官方启动器 / PCL / HMCL：`versions/` 与游戏数据同级
    if source.join("versions").is_dir() {
        return (ImportLayout::Flat, source.to_path_buf());
    }

    // 裸游戏目录：没有任何版本信息
    (ImportLayout::GameDirOnly, source.to_path_buf())
}

/// 从 `versions/` 目录里收集候选。
fn collect_from_versions(game_dir: &Path, source: &Path, out: &mut Vec<ImportCandidate>) {
    let versions_dir = source.join("versions");
    let Ok(entries) = std::fs::read_dir(&versions_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }

        // 版本 JSON 通常是 `<版本目录>/<版本 ID>.json`，
        // 但也见过直接叫 `version.json` 的写法，因此扫一遍所有 json。
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };

            let id = json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let inherits = json
                .get("inheritsFrom")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let Some((game_version, loader)) = classify(&id, &inherits) else {
                continue;
            };

            out.push(ImportCandidate {
                label: if id.is_empty() {
                    dir.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "导入的实例".to_string())
                } else {
                    id.clone()
                },
                game_version,
                loader,
                layout: ImportLayout::Flat,
                game_dir: game_dir.to_string_lossy().to_string(),
                mod_count: count_files(&game_dir.join("mods")),
                save_count: count_dirs(&game_dir.join("saves")),
            });
            break;
        }
    }
}

/// 从 MultiMC / Prism 的 `mmc-pack.json` 收集候选。
fn collect_from_mmc(source: &Path, game_dir: &Path, out: &mut Vec<ImportCandidate>) {
    let Ok(raw) = std::fs::read_to_string(source.join("mmc-pack.json")) else {
        return;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };

    let components = json
        .get("components")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut game_version = String::new();
    let mut loader = LoaderInfo {
        r#type: "vanilla".to_string(),
        version: None,
    };

    for component in &components {
        let Some(uid) = component.get("uid").and_then(|v| v.as_str()) else {
            continue;
        };
        let version = component
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        match uid {
            "net.minecraft" => game_version = version,
            "net.fabric" => {
                loader = LoaderInfo {
                    r#type: "fabric".to_string(),
                    version: Some(version),
                }
            }
            "net.quilt" => {
                loader = LoaderInfo {
                    r#type: "quilt".to_string(),
                    version: Some(version),
                }
            }
            "net.minecraftforge" => {
                loader = LoaderInfo {
                    r#type: "forge".to_string(),
                    version: Some(version),
                }
            }
            "net.neoforge" => {
                loader = LoaderInfo {
                    r#type: "neoforge".to_string(),
                    version: Some(version),
                }
            }
            _ => {}
        }
    }

    if game_version.is_empty() {
        return;
    }

    out.push(ImportCandidate {
        label: source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "导入的实例".to_string()),
        game_version,
        loader,
        layout: ImportLayout::Nested,
        game_dir: game_dir.to_string_lossy().to_string(),
        mod_count: count_files(&game_dir.join("mods")),
        save_count: count_dirs(&game_dir.join("saves")),
    });
}

/// 从版本 ID 与继承关系推断「游戏版本 + 加载器」。
///
/// 版本 ID 的写法在各家启动器里五花八门，常见的有：
///
/// - `1.20.1`（原版）
/// - `1.20.1-forge-47.2.0`（Forge）
/// - `1.20.1-neoforge-21.1.65`（NeoForge）
/// - `1.20.1-fabric-0.15.0`（Fabric）
/// - `fabric-loader-0.15.0-1.20.1`（Fabric，另一种写法）
/// - `1.20.1-quilt-0.17.0`（Quilt）
/// - `1.20.1-OptiFine_HD_U_G5`（OptiFine）
///
/// 另有 `inheritsFrom` 字段：加载器版本的 JSON 会声明继承自哪个原版版本，
/// 那是**最可靠**的来源，优先采用。
fn classify(id: &str, inherits: &str) -> Option<(String, LoaderInfo)> {
    let vanilla = LoaderInfo {
        r#type: "vanilla".to_string(),
        version: None,
    };

    // 没有任何加载器痕迹：这就是一个原版版本
    if inherits.is_empty() {
        let game_version = id.trim();
        if game_version.is_empty() {
            return None;
        }
        return Some((game_version.to_string(), vanilla));
    }

    // 有 inheritsFrom：父版本就是游戏版本
    let game_version = inherits.trim().to_string();

    let lowered = id.to_ascii_lowercase();
    let loader = if let Some(rest) = lowered.strip_prefix("fabric-loader-") {
        // `fabric-loader-0.15.0-1.20.1`：版本号是第二段
        let version = rest.split('-').next().unwrap_or(rest);
        ("fabric", version.to_string())
    } else if let Some(rest) = lowered.strip_prefix("quilt-loader-") {
        let version = rest.split('-').next().unwrap_or(rest);
        ("quilt", version.to_string())
    } else if let Some(index) = lowered.find("-forge-") {
        ("forge", lowered[index + 7..].to_string())
    } else if let Some(index) = lowered.find("-neoforge-") {
        ("neoforge", lowered[index + 10..].to_string())
    } else if let Some(index) = lowered.find("-quilt-") {
        ("quilt", lowered[index + 7..].to_string())
    } else if let Some(index) = lowered.find("-fabric-") {
        ("fabric", lowered[index + 8..].to_string())
    } else if lowered.contains("optifine") {
        let version = id
            .rsplit('-')
            .next()
            .unwrap_or_default()
            .trim_start_matches("optifine_")
            .to_string();
        ("optifine", version)
    } else {
        // 认不出加载器类型，但有继承关系——按原版处理，
        // 游戏版本仍然可用。
        return Some((game_version, vanilla));
    };

    Some((
        game_version,
        LoaderInfo {
            r#type: loader.0.to_string(),
            version: Some(loader.1),
        },
    ))
}

/// 候选排序：带加载器的排前面，同类里版本号新的排前面。
fn sort_candidates(candidates: &mut [ImportCandidate]) {
    candidates.sort_by(|a, b| {
        let a_has_loader = (a.loader.r#type != "vanilla") as u8;
        let b_has_loader = (b.loader.r#type != "vanilla") as u8;
        b_has_loader
            .cmp(&a_has_loader)
            .then_with(|| compare_versions(&b.game_version, &a.game_version))
            .then_with(|| a.label.cmp(&b.label))
    });
}

/// 比较两个版本号：`1.21.1` > `1.20.1` > `1.8.9`。
///
/// 按数字段逐个比较，非数字段按字符串比。
/// `1.20.1-forge` 与 `1.20.1` 的游戏版本部分相同，因此只看数字段。
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |value: &str| -> Vec<String> {
        value
            .split(['.', '-', '_'])
            .map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .collect()
    };

    let left = split(a);
    let right = split(b);
    for index in 0..left.len().max(right.len()) {
        let l = left
            .get(index)
            .map(|s| s.parse::<u64>().unwrap_or(0))
            .unwrap_or(0);
        let r = right
            .get(index)
            .map(|s| s.parse::<u64>().unwrap_or(0))
            .unwrap_or(0);
        match l.cmp(&r) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// 统计目录里的文件数（不递归）。
fn count_files(dir: &Path) -> u32 {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter(|e| e.path().is_file()).count() as u32)
        .unwrap_or(0)
}

/// 统计目录里的子目录数（不递归）。
fn count_dirs(dir: &Path) -> u32 {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter(|e| e.path().is_dir()).count() as u32)
        .unwrap_or(0)
}

/// 把外部实例的**用户数据**复制到 Sereno 实例的 `minecraft/` 下。
///
/// 只搬 `mods/`、`config/`、`saves/` 这类内容；游戏本体与库文件由
/// Sereno 自己重新下载（更快，且能用上镜像与校验）。
///
/// 返回复制的文件数与字节数。
pub fn copy_user_data(source_game_dir: &Path, target_game_dir: &Path) -> Result<(u32, u64)> {
    if !source_game_dir.is_dir() {
        return Err(LauncherError::Io {
            message: format!("源游戏目录不存在: {}", source_game_dir.display()),
        });
    }

    crate::utils::paths::ensure_dir(target_game_dir)?;

    let mut files = 0u32;
    let mut bytes = 0u64;

    for dir_name in COPY_DIRS {
        let from = source_game_dir.join(dir_name);
        if !from.is_dir() {
            continue;
        }
        let to = target_game_dir.join(dir_name);
        let f = copy_dir(&from, &to, &mut bytes)?;
        files += f;
    }

    for file_name in COPY_FILES {
        let from = source_game_dir.join(file_name);
        if !from.is_file() {
            continue;
        }
        let to = target_game_dir.join(file_name);
        if to.exists() {
            continue;
        }
        let size = std::fs::metadata(&from).map(|m| m.len()).unwrap_or(0);
        if let Err(error) = std::fs::copy(&from, &to) {
            tracing::warn!(path = %from.display(), %error, "复制实例数据文件失败");
            continue;
        }
        bytes += size;
        files += 1;
    }

    // 按扩展名匹配的文件（config 目录之外的散装配置）
    let Ok(entries) = std::fs::read_dir(source_game_dir) else {
        return Ok((files, bytes));
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if !COPY_EXTENSIONS
            .iter()
            .any(|ext| lower.ends_with(&format!(".{ext}")))
        {
            continue;
        }
        let to = target_game_dir.join(name);
        if to.exists() {
            continue;
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if let Err(error) = std::fs::copy(&path, &to) {
            tracing::warn!(path = %path.display(), %error, "复制实例数据文件失败");
            continue;
        }
        bytes += size;
        files += 1;
    }

    Ok((files, bytes))
}

/// 递归复制目录。
///
/// **跳过符号链接**：外部目录里的链接可能指向目录外，
/// 跟着复制会把用户主目录里的东西拖进实例。
///
/// 字节数通过 `bytes` 累加返回（跨目录累加），这里只返回文件数。
fn copy_dir(from: &Path, to: &Path, bytes: &mut u64) -> Result<u32> {
    crate::utils::paths::ensure_dir(to)?;

    let mut files = 0u32;
    let entries = std::fs::read_dir(from).map_err(|e| LauncherError::Io {
        message: format!("读取目录失败 {}: {e}", from.display()),
    })?;

    for entry in entries.flatten() {
        let source = entry.path();
        let meta = match std::fs::symlink_metadata(&source) {
            Ok(meta) => meta,
            Err(_) => continue,
        };

        if meta.file_type().is_symlink() {
            tracing::debug!(path = %source.display(), "跳过符号链接");
            continue;
        }

        let Some(name) = source.file_name() else {
            continue;
        };
        let target = to.join(name);

        if meta.is_dir() {
            let f = copy_dir(&source, &target, bytes)?;
            files += f;
            continue;
        }

        if !meta.is_file() {
            continue;
        }

        if *bytes + meta.len() > MAX_COPY_BYTES {
            return Err(LauncherError::InvalidConfig {
                key: "importSize".into(),
                message: format!(
                    "导入的数据超过 {} GB 上限，请确认是否选错了目录",
                    MAX_COPY_BYTES / 1024 / 1024 / 1024
                ),
            });
        }

        // 目标已存在时不覆盖：导入是「补齐」而不是「替换」
        if target.exists() {
            continue;
        }

        if let Err(error) = std::fs::copy(&source, &target) {
            tracing::warn!(path = %source.display(), %error, "复制实例数据失败");
            continue;
        }

        *bytes += meta.len();
        files += 1;
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_vanilla_version() {
        let (version, loader) = classify("1.21.1", "").unwrap();
        assert_eq!(version, "1.21.1");
        assert_eq!(loader.r#type, "vanilla");
        assert!(loader.version.is_none());
    }

    #[test]
    fn classifies_forge_from_inherits() {
        let (version, loader) = classify("1.20.1-forge-47.2.0", "1.20.1").unwrap();
        assert_eq!(version, "1.20.1");
        assert_eq!(loader.r#type, "forge");
        assert_eq!(loader.version.as_deref(), Some("47.2.0"));
    }

    #[test]
    fn classifies_fabric_without_inherits() {
        let (version, loader) = classify("1.20.1-fabric-0.15.0", "1.20.1").unwrap();
        assert_eq!(version, "1.20.1");
        assert_eq!(loader.r#type, "fabric");
        assert_eq!(loader.version.as_deref(), Some("0.15.0"));
    }

    #[test]
    fn classifies_fabric_loader_prefix_style() {
        let (version, loader) = classify("fabric-loader-0.15.0-1.20.1", "1.20.1").unwrap();
        assert_eq!(version, "1.20.1");
        assert_eq!(loader.r#type, "fabric");
        assert_eq!(loader.version.as_deref(), Some("0.15.0"));
    }

    #[test]
    fn classifies_neoforge() {
        let (_, loader) = classify("1.21.1-neoforge-21.1.65", "1.21.1").unwrap();
        assert_eq!(loader.r#type, "neoforge");
        assert_eq!(loader.version.as_deref(), Some("21.1.65"));
    }

    #[test]
    fn compares_versions_numerically() {
        use std::cmp::Ordering;
        assert_eq!(compare_versions("1.21.1", "1.20.1"), Ordering::Greater);
        assert_eq!(compare_versions("1.8.9", "1.21.1"), Ordering::Less);
        assert_eq!(compare_versions("1.20.1", "1.20.1"), Ordering::Equal);
        // 1.20 与 1.20.1：前者少一段，视为更旧
        assert_eq!(compare_versions("1.20", "1.20.1"), Ordering::Less);
    }

    #[test]
    fn sorts_loader_versions_first_then_newest() {
        let mut candidates = vec![
            ImportCandidate {
                label: "1.8.9".into(),
                game_version: "1.8.9".into(),
                loader: LoaderInfo {
                    r#type: "vanilla".into(),
                    version: None,
                },
                layout: ImportLayout::Flat,
                game_dir: String::new(),
                mod_count: 0,
                save_count: 0,
            },
            ImportCandidate {
                label: "1.21.1-forge".into(),
                game_version: "1.21.1".into(),
                loader: LoaderInfo {
                    r#type: "forge".into(),
                    version: Some("52.0.1".into()),
                },
                layout: ImportLayout::Flat,
                game_dir: String::new(),
                mod_count: 0,
                save_count: 0,
            },
            ImportCandidate {
                label: "1.20.1-forge".into(),
                game_version: "1.20.1".into(),
                loader: LoaderInfo {
                    r#type: "forge".into(),
                    version: Some("47.2.0".into()),
                },
                layout: ImportLayout::Flat,
                game_dir: String::new(),
                mod_count: 0,
                save_count: 0,
            },
        ];
        sort_candidates(&mut candidates);
        assert_eq!(candidates[0].game_version, "1.21.1");
        assert_eq!(candidates[1].game_version, "1.20.1");
        assert_eq!(candidates[2].game_version, "1.8.9");
    }

    #[test]
    fn detects_flat_layout_and_collects_candidates() {
        let root = std::env::temp_dir().join(format!("sereno-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let versions = root.join("versions").join("1.20.1-forge-47.2.0");
        std::fs::create_dir_all(&versions).unwrap();
        std::fs::write(
            versions.join("1.20.1-forge-47.2.0.json"),
            r#"{"id":"1.20.1-forge-47.2.0","inheritsFrom":"1.20.1"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("mods")).unwrap();
        std::fs::write(root.join("mods").join("a.jar"), "x").unwrap();

        let scan = scan(&root).unwrap();
        assert_eq!(scan.layout, ImportLayout::Flat);
        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.candidates[0].game_version, "1.20.1");
        assert_eq!(scan.candidates[0].loader.r#type, "forge");
        assert_eq!(scan.candidates[0].mod_count, 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_nested_layout_for_multimc() {
        let root = std::env::temp_dir().join(format!("sereno-import-mmc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".minecraft").join("mods")).unwrap();
        std::fs::write(
            root.join("mmc-pack.json"),
            r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.fabric","version":"0.15.0"}]}"#,
        )
        .unwrap();

        let scan = scan(&root).unwrap();
        assert_eq!(scan.layout, ImportLayout::Nested);
        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.candidates[0].game_version, "1.20.1");
        assert_eq!(scan.candidates[0].loader.r#type, "fabric");
        assert!(scan.game_dir.ends_with(".minecraft"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copies_user_data_but_not_libraries() {
        let root = std::env::temp_dir().join(format!("sereno-import-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("source");
        let target = root.join("target");

        std::fs::create_dir_all(source.join("mods")).unwrap();
        std::fs::write(source.join("mods").join("a.jar"), "jar").unwrap();
        std::fs::create_dir_all(source.join("saves").join("World")).unwrap();
        std::fs::write(source.join("saves").join("World").join("level.dat"), "x").unwrap();
        std::fs::write(source.join("options.txt"), "lang:zh_cn").unwrap();
        std::fs::create_dir_all(source.join("libraries")).unwrap();
        std::fs::write(source.join("libraries").join("big.jar"), "no").unwrap();
        std::fs::create_dir_all(source.join("versions")).unwrap();

        let (files, bytes) = copy_user_data(&source, &target).unwrap();
        assert!(files >= 3);
        assert!(bytes > 0);
        assert!(target.join("mods").join("a.jar").exists());
        assert!(
            target
                .join("saves")
                .join("World")
                .join("level.dat")
                .exists()
        );
        assert!(target.join("options.txt").exists());
        assert!(
            !target.join("libraries").exists(),
            "库文件不该被复制：Sereno 自己会重新下载"
        );
        assert!(!target.join("versions").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
