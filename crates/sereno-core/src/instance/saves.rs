//! 世界（存档）列表管理：浏览 / 删除 / 打开目录。
//!
//! 备份与恢复由 `backup::world` 独立负责；本模块只做「看」与「删」。
//! `level.dat` 是 NBT 格式，v1 不解析——展示目录名 / 修改时间 / 体积 /
//! icon.png 缩略图已经覆盖 PCL 存档列表的主要信息；游戏模式与种子
//! 等留待 NBT 解析器落地后补充。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::models::settings::AppSettings;
use crate::utils::paths::{instance_dir, validate_instance_id};
use crate::{LauncherError, Result};

/// 单个存档的摘要信息。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveSummary {
    /// 存档名 = `saves/` 下的目录名（游戏内显示的名字来自 level.dat，
    /// v1 不解析 NBT，用目录名代替——绝大多数玩家两者一致）。
    pub name: String,
    /// `icon.png` 的 base64 data URL（缩略图）；不存在为 None。
    pub icon_data_url: Option<String>,
    /// 最后游玩时间（level.dat 的修改时间，秒级时间戳字符串——
    /// specta 禁止导出 i64，全项目时间戳统一走字符串）。
    pub last_played_secs: String,
    /// 存档目录总大小（字节，字符串——specta 禁止导出 u64）。
    pub size_bytes: String,
}

fn saves_root(settings: &AppSettings, instance_id: &str) -> Result<PathBuf> {
    validate_instance_id(instance_id)?;
    Ok(instance_dir(settings, instance_id)?
        .join("minecraft")
        .join("saves"))
}

/// 存档名校验：它会被拼进路径，不校验等于把 `../../..` 交给调用方。
///
/// 与 `validate_instance_id` 不同：存档名允许中文、空格（玩家命名自由），
/// 但绝不允许路径分隔符与 `..`。
fn validate_save_name(name: &str) -> Result<()> {
    if name.is_empty() || name.chars().count() > 128 {
        return Err(LauncherError::InvalidConfig {
            key: "save".into(),
            message: "存档名不能为空且最长 128 个字符".into(),
        });
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(LauncherError::InvalidConfig {
            key: "save".into(),
            message: format!("非法的存档名：{name}"),
        });
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(LauncherError::InvalidConfig {
            key: "save".into(),
            message: "存档名不能包含控制字符".into(),
        });
    }
    Ok(())
}

/// 校验后返回具体存档目录的路径（打开目录等命令共用）。
pub fn save_path(settings: &AppSettings, instance_id: &str, name: &str) -> Result<PathBuf> {
    validate_save_name(name)?;
    let path = saves_root(settings, instance_id)?.join(name);
    if !path.is_dir() {
        return Err(LauncherError::InvalidConfig {
            key: "save".into(),
            message: format!("存档不存在：{name}"),
        });
    }
    Ok(path)
}

/// 列出实例的全部存档，按最后游玩时间倒序。
pub fn list(settings: &AppSettings, instance_id: &str) -> Result<Vec<SaveSummary>> {
    let root = saves_root(settings, instance_id)?;
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut saves = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|e| LauncherError::Io {
        message: format!("读取存档目录失败 {}: {e}", root.display()),
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        // 没有 level.dat 的目录不是存档（可能是残缺的半成品），跳过
        let level_dat = path.join("level.dat");
        if !level_dat.is_file() {
            continue;
        }
        let last_played_secs = std::fs::metadata(&level_dat)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|| "0".into());
        let icon_data_url = read_icon_data_url(&path);
        let size_bytes = dir_size(&path, 0).to_string();
        saves.push(SaveSummary {
            name,
            icon_data_url,
            last_played_secs,
            size_bytes,
        });
    }

    saves.sort_by(|a, b| {
        b.last_played_secs
            .parse::<u64>()
            .unwrap_or(0)
            .cmp(&a.last_played_secs.parse::<u64>().unwrap_or(0))
    });
    Ok(saves)
}

/// 删除存档：回收站优先，失败重试一次，再失败回退物理删除。
///
/// 与 `lifecycle::delete_instance` 的三级策略一致——用户已明确确认
/// 删除意图，卡死在「删不掉」比不可恢复更糟。
pub fn delete(settings: &AppSettings, instance_id: &str, name: &str) -> Result<()> {
    let path = save_path(settings, instance_id, name)?;
    match trash::delete(&path) {
        Ok(_) => Ok(()),
        Err(first) => {
            // 下载/游戏进程可能仍短暂持有句柄；等待后重试一次
            std::thread::sleep(std::time::Duration::from_millis(600));
            match trash::delete(&path) {
                Ok(_) => Ok(()),
                Err(second) => {
                    tracing::warn!(
                        save = %path.display(),
                        first = %first,
                        second = %second,
                        "存档移入回收站两次失败，回退物理删除"
                    );
                    crate::utils::fs::remove_dir_all(&path).map_err(|e| LauncherError::Io {
                        message: format!(
                            "移入回收站失败（{first} / {second}），物理删除也失败: {e}"
                        ),
                    })
                }
            }
        }
    }
}

/// 读取存档图标并转 base64 data URL。
///
/// 图标本身只有几 KB，转 data URL 让前端免于配置 asset 协议范围。
fn read_icon_data_url(save_dir: &Path) -> Option<String> {
    const MAX_ICON_BYTES: usize = 64 * 1024;
    let bytes = std::fs::read(save_dir.join("icon.png")).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
        return None;
    }
    use base64::Engine as _;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

/// 递归统计目录大小；符号链接不跟随，深度封顶防止异常结构拖垮列表。
fn dir_size(path: &Path, depth: u8) -> u64 {
    if depth > 16 {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            total += dir_size(&entry.path(), depth + 1);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_name_validation_rejects_traversal() {
        assert!(validate_save_name("新世界").is_ok());
        assert!(validate_save_name("My World (1)").is_ok());
        assert!(validate_save_name("../etc").is_err());
        assert!(validate_save_name("a/b").is_err());
        assert!(validate_save_name("a\\b").is_err());
        assert!(validate_save_name("").is_err());
        assert!(validate_save_name(".").is_err());
        assert!(validate_save_name("..").is_err());
    }

    #[test]
    fn save_path_rejects_missing_dir() {
        let settings = crate::models::settings::AppSettings::default();
        let err = save_path(&settings, "inst", "不存在");
        assert!(err.is_err());
    }
}
