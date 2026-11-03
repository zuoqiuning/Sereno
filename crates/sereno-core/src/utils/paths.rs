//! 平台感知路径：**状态目录与实例根目录的唯一出处**。
//!
//! 所有涉及「写到哪、读哪」的代码都必须经过这里，禁止在别处拼路径字符串。
//!
//! Windows 上 `dirs::config_dir()` 返回 `%APPDATA%`，因此状态目录固定为
//! `%APPDATA%\Sereno`。**状态目录不能放在安装目录里**——否则覆盖安装时
//! 用户数据会随卸载器一起丢失。

use std::path::{Path, PathBuf};

use crate::models::settings::AppSettings;
use crate::{LauncherError, Result};

/// 状态目录名（也是应用在用户目录下的命名空间）。
pub const APP_DIR_NAME: &str = "Sereno";

/// 状态根目录：`%APPDATA%\Sereno`。
///
/// - Windows：`%APPDATA%\Sereno`
/// - macOS：`~/Library/Application Support/Sereno`
/// - Linux：`~/.config/Sereno`
pub fn state_dir() -> Result<PathBuf> {
    // 便携模式：exe 同目录放 portable.txt（或设 SERENO_PORTABLE=1），
    // 所有数据写到 exe 旁的 data/，删除文件夹即可“卸载”。
    if std::env::var("SERENO_PORTABLE").ok().as_deref() == Some("1")
        && let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        return Ok(dir.join("data"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && dir.join("portable.txt").is_file()
    {
        return Ok(dir.join("data"));
    }

    let base = dirs::config_dir().ok_or_else(|| LauncherError::Internal {
        message: "无法定位系统配置目录（dirs::config_dir 返回 None）".into(),
    })?;
    Ok(base.join(APP_DIR_NAME))
}

/// 日志目录：`%APPDATA%\Sereno\logs`。
///
/// **不要**把日志写进实例目录：实例会被复制、导出、删除，
/// 日志跟着走会既泄露又混乱。
pub fn logs_dir() -> Result<PathBuf> {
    Ok(state_dir()?.join("logs"))
}

/// 诊断包目录：`%APPDATA%\Sereno\diagnostics`。
pub fn diagnostics_dir() -> Result<PathBuf> {
    Ok(state_dir()?.join("diagnostics"))
}

/// 元数据缓存目录：`%APPDATA%\Sereno\cache`。
pub fn cache_dir() -> Result<PathBuf> {
    Ok(state_dir()?.join("cache"))
}

/// 设置文件：`%APPDATA%\Sereno\settings.json`。
pub fn settings_file() -> Result<PathBuf> {
    Ok(state_dir()?.join("settings.json"))
}

/// **默认**实例根目录：`文档\Sereno\instances`。
///
/// 为什么放在「文档」而不是 AppData：
///
/// - 实例内容体量大（数百 MB 到数 GB），放在 `RoamingAppData` 下会触发
///   Windows 的漫游/同步机制，既慢又可能被企业策略限制；
/// - 备份、迁移、分享整合包时，「文档」是用户真正找得到的地方。
///
/// 见《实例生命周期与布局》。
pub fn default_instances_root() -> Result<PathBuf> {
    let docs = dirs::document_dir().ok_or_else(|| LauncherError::Internal {
        message: "无法定位系统文档目录（dirs::document_dir 返回 None）".into(),
    })?;
    Ok(docs.join(APP_DIR_NAME).join("instances"))
}

/// **实例根目录**：读设置，用户没配过就用默认目录。
///
/// ```no_run
/// # use sereno_core::models::settings::AppSettings;
/// # use sereno_core::utils::paths;
/// let root = paths::instances_root(&AppSettings::default())?;
/// # Ok::<(), sereno_core::LauncherError>(())
/// ```
pub fn instances_root(settings: &AppSettings) -> Result<PathBuf> {
    match settings.instances_root.as_deref().map(str::trim) {
        Some(configured) if !configured.is_empty() => Ok(PathBuf::from(configured)),
        _ => default_instances_root(),
    }
}

/// 单个实例目录：`<instances_root>\<id>`。
///
/// 实例 id 是前端可传入的参数，必须**先校验再拼路径**，
/// 否则 `../` 能让删除/启动/写文件跑到实例根目录之外。
pub fn instance_dir(settings: &AppSettings, instance_id: &str) -> Result<PathBuf> {
    validate_instance_id(instance_id)?;
    Ok(instances_root(settings)?.join(instance_id))
}

/// 只允许字母、数字、`-`、`_` 的实例 id。
pub fn validate_instance_id(id: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');

    if valid {
        Ok(())
    } else {
        Err(LauncherError::InvalidConfig {
            key: "instanceId".into(),
            message: format!("实例 ID 含非法字符: {id}"),
        })
    }
}

/// 确保目录存在（含全部父目录）。
pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| LauncherError::Io {
        message: format!("创建目录失败 {}: {e}", path.display()),
    })
}

/// 确保状态目录体系完整存在（启动时调用一次）。
pub fn ensure_state_layout() -> Result<()> {
    ensure_dir(&state_dir()?)?;
    ensure_dir(&logs_dir()?)?;
    ensure_dir(&diagnostics_dir()?)?;
    ensure_dir(&cache_dir()?)?;
    Ok(())
}

/// 把绝对路径里的用户名替换为 `%USERPROFILE%`。
///
/// 日志与诊断包会被用户贴到群里、issue 里、论坛上，
/// **任何进入日志的路径都按「将要公开」对待**。
pub fn redact_path(path: &Path) -> String {
    let raw = path.display().to_string();

    // 1. 当前用户目录 → %USERPROFILE%
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if !home_str.is_empty() && raw.starts_with(&home_str) {
            return format!("%USERPROFILE%{}", &raw[home_str.len()..]);
        }
    }

    // 2. 任意盘符下的 Users\<用户名>\ → Users\<user>\
    redact_user_segment(&raw)
}

/// 把路径里的 `Users\<用户名>` 段替换掉，**不限盘符**。
///
/// 用户目录完全可能在 `D:\Users\alice`，只匹配 `C:\Users\` 会漏。
/// 同时处理 JSON 转义形式（反斜杠被翻倍），因为日志文件里是 JSON 文本。
pub fn redact_user_segment(raw: &str) -> String {
    let mut result = raw.to_string();

    for pattern in ["\\Users\\", "\\\\Users\\\\"] {
        let mut search_from = 0usize;
        while let Some(index) = result[search_from..].find(pattern) {
            let name_start = search_from + index + pattern.len();
            let rest = &result[name_start..];

            // 用户名段到下一个分隔符为止
            let name_len = rest.find(['\\', '/']).unwrap_or(rest.len());
            if name_len == 0 {
                // 没有用户名（形如 `C:\Users\`），换个位置继续找
                search_from = name_start;
                continue;
            }

            let mut updated = String::with_capacity(result.len());
            updated.push_str(&result[..name_start]);
            updated.push_str("<user>");
            updated.push_str(&rest[name_len..]);
            result = updated;
            search_from = name_start + "<user>".len();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_is_namespaced() {
        let dir = state_dir().unwrap();
        assert_eq!(dir.file_name().unwrap(), APP_DIR_NAME);
    }

    #[test]
    fn subdirs_live_under_state_dir() {
        let root = state_dir().unwrap();
        for child in [
            logs_dir().unwrap(),
            diagnostics_dir().unwrap(),
            cache_dir().unwrap(),
            settings_file().unwrap(),
        ] {
            assert!(child.starts_with(&root));
        }
    }

    #[test]
    fn instances_root_follows_settings() {
        let mut settings = AppSettings::default();

        // 未配置 → 默认目录（文档\Sereno\instances）
        let default_root = instances_root(&settings).unwrap();
        assert_eq!(default_root, default_instances_root().unwrap());
        assert!(
            default_root.ends_with("Sereno\\instances"),
            "默认实例根目录应在文档下: {}",
            default_root.display()
        );

        // 配置了 → 用配置的
        settings.instances_root = Some("D:\\Games\\Sereno".into());
        assert_eq!(
            instances_root(&settings).unwrap(),
            PathBuf::from("D:\\Games\\Sereno")
        );

        // 空字符串 → 回到默认
        settings.instances_root = Some("   ".into());
        assert_eq!(instances_root(&settings).unwrap(), default_root);
    }

    #[test]
    fn redact_path_removes_user_name_on_any_drive() {
        for secret in [
            r"C:\Users\alice\AppData\Roaming\secret.txt",
            r"D:\Users\bob\Games\secret.txt",
            r"E:\Users\carol\secret.txt",
        ] {
            let redacted = redact_path(Path::new(secret));
            assert!(!redacted.contains("alice"), "未脱敏: {redacted}");
            assert!(!redacted.contains("bob"), "未脱敏: {redacted}");
            assert!(!redacted.contains("carol"), "未脱敏: {redacted}");
            assert!(redacted.contains("<user>"), "未替换为占位符: {redacted}");
        }
    }

    #[test]
    fn redact_path_maps_own_home_to_placeholder() {
        let home = dirs::home_dir().unwrap();
        let secret = home.join("AppData").join("Roaming").join("secret.txt");
        let redacted = redact_path(&secret);
        assert!(
            redacted.contains("%USERPROFILE%"),
            "未替换为占位符: {redacted}"
        );
    }
}
