//! `settings.json` 的读写与版本迁移。

use crate::Result;
use crate::error::LauncherError;
use crate::models::settings::{AppSettings, SCHEMA_VERSION};
use crate::utils::fs::atomic_write_str;
use crate::utils::paths::settings_file;

/// 读取设置。
///
/// 文件不存在时返回默认值（首次启动的正常路径，不是错误）。
/// 文件存在但损坏时返回 `Parse` 错误——**不静默覆盖**，
/// 否则用户改过的配置会被一次解析失败抹掉。
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

/// 写入设置（原子写）。
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

/// 读取设置，**损坏时降级而不是让应用起不来**。
///
/// 启动路径专用。行为：
///
/// 1. 正常读 → 迁移 → 返回；
/// 2. 文件不存在 → 返回默认值；
/// 3. 文件**解析失败** → 把原文件改名备份为 `settings.json.corrupt-<时间戳>`，
///    记一条 warn 日志，返回默认值。
///
/// **不会静默覆盖用户的配置**：原文件被保留为 `.corrupt-*` 备份，
/// 用户可以自己检查或恢复。启动器因为一个坏掉的配置文件就打不开，
/// 比丢掉几个设置项更糟。
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
            // IO 错误等：状态目录不可写之类的问题，不能靠默认值掩盖
            tracing::error!(error = %other, "读取设置失败，使用默认设置启动");
            AppSettings::default()
        }
    }
}

/// 把损坏的设置文件改名备份，返回备份路径。
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

/// 按 `schemaVersion` 迁移设置。
///
/// 每加一个不兼容的字段，就在这里补一条分支，并把 `SCHEMA_VERSION` +1。
/// 迁移必须是**幂等**的：同一个文件反复读，结果一致。
fn migrate(settings: &mut AppSettings) -> Result<()> {
    let version = settings.schema_version;

    // --- 迁移分支示例（按版本从小到大依次执行） ---
    // if version < 2 {
    //     // 旧字段 `autoLaunch` → 新字段 `startup.launchOnBoot`
    //     settings.startup.launch_on_boot = settings.remove("autoLaunch");
    //     version = 2;
    // }

    if version > SCHEMA_VERSION {
        // 设置文件来自更新版本的启动器，保留原样但记录告警。
        // 降级启动时不能把未知字段抹掉，否则用户会丢配置。
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
