//! 世界（存档）备份：把 `minecraft/saves` 打成 zip，支持列出与恢复。

use std::fs::File;
use std::io::copy;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::models::backup::WorldBackup;
use crate::models::settings::AppSettings;
use crate::utils::archive::extract_zip_safely;
use crate::utils::paths::{ensure_dir, instance_dir, state_dir};
use crate::{LauncherError, Result};

fn saves_dir(settings: &AppSettings, instance_id: &str) -> Result<PathBuf> {
    Ok(instance_dir(settings, instance_id)?
        .join("minecraft")
        .join("saves"))
}

fn backup_dir(instance_id: &str) -> Result<PathBuf> {
    // `instance_id` 会直接拼进路径；不校验就等于把 `../../..` 交给调用方。
    crate::utils::paths::validate_instance_id(instance_id)?;
    Ok(state_dir()?
        .join("backups")
        .join("worlds")
        .join(instance_id))
}

/// 把当前存档打包成 zip；没有存档时给出明确提示。
pub fn create(settings: &AppSettings, instance_id: &str) -> Result<WorldBackup> {
    let saves = saves_dir(settings, instance_id)?;
    if !saves.is_dir() {
        return Err(LauncherError::InvalidConfig {
            key: "saves".into(),
            message: "该实例还没有存档，无法备份".into(),
        });
    }

    let dir = backup_dir(instance_id)?;
    ensure_dir(&dir)?;
    let created_at = now_secs();
    // 文件名只有秒级精度：同一秒内点两次备份会互相覆盖。
    // 存在时追加序号，绝不静默丢弃上一份。
    let mut name = format!("world-{created_at}.zip");
    let mut seq = 1;
    while dir.join(&name).exists() {
        name = format!("world-{created_at}-{seq}.zip");
        seq += 1;
        if seq > 99 {
            return Err(LauncherError::Internal {
                message: "同一秒内的备份过多，请稍后再试".into(),
            });
        }
    }
    // 先写临时文件再换名：中途失败不会在列表里留下一个打不开的 zip。
    let temp = dir.join(format!(".{name}.tmp"));
    let path = dir.join(&name);

    let file = File::create(&temp).map_err(|e| LauncherError::Io {
        message: format!("创建备份文件失败: {e}"),
    })?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    if let Err(error) = add_dir(&mut writer, &saves, &saves, options) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    // `finish()` 之后必须 drop，否则文件句柄还开着，Windows 上 rename 会失败
    let mut file = writer.finish().map_err(|e| LauncherError::Io {
        message: format!("写入备份失败: {e}"),
    })?;
    use std::io::Write;
    let _ = file.flush();
    drop(file);

    std::fs::rename(&temp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        LauncherError::Io {
            message: format!("保存备份失败: {e}"),
        }
    })?;

    let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    Ok(WorldBackup {
        name,
        size,
        created_at,
    })
}

/// 列出该实例的所有备份，新的在前。
pub fn list(instance_id: &str) -> Result<Vec<WorldBackup>> {
    let dir = backup_dir(instance_id)?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| LauncherError::Io {
            message: format!("读取备份目录失败: {e}"),
        })?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("zip") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let meta = entry.metadata().ok();
        let created_at = meta
            .as_ref()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        backups.push(WorldBackup {
            name: name.to_string(),
            size: meta.map(|meta| meta.len()).unwrap_or(0),
            created_at,
        });
    }

    backups.sort_by_key(|backup| std::cmp::Reverse(backup.created_at));
    Ok(backups)
}

/// 恢复备份：先把现有 saves 改名备份，解压成功后再删掉旧目录；
/// 解压失败会把旧目录原样移回来，避免把玩家存档弄丢。
pub fn restore(settings: &AppSettings, instance_id: &str, name: &str) -> Result<()> {
    if !is_valid_backup_name(name) {
        return Err(LauncherError::InvalidConfig {
            key: "name".into(),
            message: "无效的备份文件名".into(),
        });
    }

    let dir = backup_dir(instance_id)?;
    let archive_path = dir.join(name);
    if !archive_path.is_file() {
        return Err(LauncherError::Internal {
            message: format!("备份不存在: {name}"),
        });
    }

    let saves = saves_dir(settings, instance_id)?;
    let parent = saves.parent().ok_or_else(|| LauncherError::Internal {
        message: "存档目录缺少父目录".into(),
    })?;
    ensure_dir(parent)?;

    let stamp = now_secs();

    // 两段式恢复：先解压到暂存目录，成功后才与现有存档换名。
    //
    // 不能「先把 saves 改名再解压」：解压中途失败或断电，用户看到的就是
    // 「世界没了」；更不能在回滚时用 `let _ = rename(...)` 静默吞掉失败，
    // 那等于把数据丢失当成成功返回。
    let staging = saves.with_file_name(format!("saves.restore-staging-{stamp}"));
    let _ = std::fs::remove_dir_all(&staging);

    let file = File::open(&archive_path).map_err(|e| LauncherError::Io {
        message: format!("打开备份失败: {e}"),
    })?;
    if let Err(error) = extract_zip_safely(file, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    let old = saves.with_file_name(format!("saves.restore-old-{stamp}"));
    if saves.exists() {
        std::fs::rename(&saves, &old).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging);
            LauncherError::Io {
                message: format!("暂存现有存档失败: {e}"),
            }
        })?;
    }

    if let Err(e) = std::fs::rename(&staging, &saves) {
        // 换不进去就把旧存档换回来，并**如实报错**，不要留下
        // 「saves 不存在、旧内容还叫 saves.restore-old-*」的状态。
        if old.exists() {
            let _ = std::fs::rename(&old, &saves);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(LauncherError::Io {
            message: format!("替换存档目录失败: {e}"),
        });
    }

    if old.exists() {
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

fn is_valid_backup_name(name: &str) -> bool {
    !name.is_empty()
        && name.ends_with(".zip")
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn add_dir(
    writer: &mut ZipWriter<File>,
    root: &Path,
    current: &Path,
    options: SimpleFileOptions,
) -> Result<()> {
    let entries = std::fs::read_dir(current).map_err(|e| LauncherError::Io {
        message: format!("读取存档目录失败 {}: {e}", current.display()),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| LauncherError::Internal {
                message: format!("无法计算备份相对路径: {}", path.display()),
            })?;
        let name = relative.to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            writer
                .add_directory(format!("{name}/"), options)
                .map_err(|e| LauncherError::Io {
                    message: format!("写入备份目录失败 {name}: {e}"),
                })?;
            add_dir(writer, root, &path, options)?;
        } else if path.is_file() {
            writer
                .start_file(name.clone(), options)
                .map_err(|e| LauncherError::Io {
                    message: format!("写入备份条目失败 {name}: {e}"),
                })?;
            let mut source = File::open(&path).map_err(|e| LauncherError::Io {
                message: format!("读取存档文件失败 {}: {e}", path.display()),
            })?;
            copy(&mut source, writer).map_err(|e| LauncherError::Io {
                message: format!("写入备份失败 {name}: {e}"),
            })?;
        }
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
