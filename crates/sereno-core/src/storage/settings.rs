//! `settings.json` 的读写与版本迁移。

use crate::Result;
use crate::error::LauncherError;
use crate::models::settings::{AppSettings, SCHEMA_VERSION};
use crate::utils::fs::atomic_write_str;
use crate::utils::paths::settings_file;

pub fn load() -> Result<AppSettings> {
    let path = settings_file()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let raw = std::fs::read_to_string(&path).map_err(|e| LauncherError::Io {
        message: format!("读取设置失败 {}: {e}", path.display()),
    })?;

    let mut settings: AppSettings =
        serde_json::from_str(&raw).map_err(|e| LauncherError::Parse {
            message: format!("解析设置失败 {}: {e}", path.display()),
        })?;

    migrate(&mut settings)?;
    Ok(settings.normalized())
}

pub fn save(settings: &AppSettings) -> Result<()> {
    let path = settings_file()?;
    let mut json = serde_json::to_string_pretty(&settings.clone().normalized()).map_err(|e| {
        LauncherError::Parse {
            message: format!("序列化设置失败: {e}"),
        }
    })?;
    json.push('\n');
    atomic_write_str(&path, &json)
}

pub fn load_or_recover() -> AppSettings {
    match load() {
        Ok(settings) => settings,
        Err(LauncherError::Parse { message }) => {
            let path = settings_file().ok();
            let backup_note = path
                .as_ref()
                .and_then(|p| backup_corrupt(p))
                .map(|p| format!("，已备份为 {}", p.display()))
                .unwrap_or_else(|| "，备份失败".to_string());

            tracing::warn!(
                error = %message,
                "设置文件解析失败，已使用默认设置启动{backup_note}"
            );
            AppSettings::default()
        }
        Err(other) => {
            tracing::error!(error = %other, "读取设置失败，使用默认设置启动");
            AppSettings::default()
        }
    }
}

fn backup_corrupt(path: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let file_name = path.file_name()?.to_string_lossy().to_string();
    let backup = path.with_file_name(format!("{file_name}.corrupt-{stamp}"));

    std::fs::rename(path, &backup).ok()?;
    Some(backup)
}

fn migrate(settings: &mut AppSettings) -> Result<()> {
    let version = settings.schema_version;

    // 迁移链示例：每一级 +1
    // v1 -> v2
    // if version < 2 {
    //     // 补全新字段默认值（如果需要）
    //     version = 2;
    // }

    if version > SCHEMA_VERSION {
        tracing::warn!(
            schema_version = version,
            supported = SCHEMA_VERSION,
            "设置文件的 schema 版本高于当前程序，已按原样保留"
        );
        return Ok(());
    }

    settings.schema_version = SCHEMA_VERSION;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent() {
        let mut s = AppSettings {
            schema_version: SCHEMA_VERSION,
            ..Default::default()
        };
        migrate(&mut s).unwrap();
        let once = s.clone();
        migrate(&mut s).unwrap();
        assert_eq!(once, s);
    }

    #[test]
    fn future_schema_is_preserved() {
        let mut s = AppSettings {
            schema_version: SCHEMA_VERSION + 5,
            ..Default::default()
        };
        migrate(&mut s).unwrap();
        assert_eq!(s.schema_version, SCHEMA_VERSION + 5);
    }
}
