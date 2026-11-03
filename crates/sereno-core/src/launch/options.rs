//! 启动前的 `options.txt` 语言修正。
//!
//! 只传 `--lang zh_cn` 是不够的：Minecraft 会优先读取 `options.txt` 里的
//! `lang:`，已有英文存档时命令行参数不会覆盖它（PCL 也是直接改 options.txt）。
//!
//! 版本差异（PCL 实测结论）：
//! - 1.1–1.10：必须 `zh_CN`（区域码大写），`zh_cn` 会被切成英文；
//! - 1.11+：必须 `zh_cn`，`zh_CN` 会被切成英文；
//! - 1.0 及更早没有语言选项，写入无害。

use std::path::{Path, PathBuf};

use crate::Result;

/// 把 `options.txt` 的 `lang` 改成简体中文。
///
/// 文件选择顺序与 PCL 一致：
/// 1. 实例的 `options.txt`；
/// 2. 不存在时，退到 Yosbr Mod 的 `config/yosbr/options.txt`；
/// 3. 两者都没有时，创建 `options.txt`。
pub fn ensure_chinese(game_dir: &Path, mc_version: &str) -> Result<()> {
    let options = game_dir.join("options.txt");
    let yosbr = game_dir.join("config").join("yosbr").join("options.txt");

    let target: PathBuf = if options.exists() {
        options
    } else if yosbr.exists() {
        yosbr
    } else {
        options
    };

    let target_lang = if requires_upper_region(mc_version) {
        "zh_CN"
    } else {
        "zh_cn"
    };

    let existing = std::fs::read_to_string(&target).unwrap_or_default();
    let updated = set_lang_line(&existing, target_lang);

    if existing == updated {
        return Ok(());
    }

    crate::utils::fs::atomic_write_str(&target, &updated)?;
    tracing::info!(
        target = %target.display(),
        %target_lang,
        "已设置游戏语言为简体中文"
    );
    Ok(())
}

/// 1.10 及以下（含 1.7.10 / 1.8.9）需要 `zh_CN`。
fn requires_upper_region(mc_version: &str) -> bool {
    // 只取开头的数字与点，兼容 `1.8.9-forge…`、`23w13a` 等版本 id
    let prefix: String = mc_version
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let prefix = prefix.trim_end_matches('.');

    let mut parts = prefix.split('.');
    let Some(major) = parts.next().and_then(|v| v.parse::<u32>().ok()) else {
        return false;
    };
    let minor = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);

    major == 1 && minor <= 10
}

/// 替换（或追加）`lang:` 行，保留其它设置与原有换行风格。
fn set_lang_line(content: &str, lang: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let mut replaced = false;

    for line in &mut lines {
        if line.starts_with("lang:") {
            *line = format!("lang:{lang}");
            replaced = true;
            break;
        }
    }

    if !replaced {
        lines.push(format!("lang:{lang}"));
    }

    let mut result = lines.join("\n");
    if content.ends_with('\n') || !replaced {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        std::env::temp_dir().join(format!("sereno-options-{stamp}"))
    }

    #[test]
    fn replaces_existing_lang_and_keeps_other_lines() {
        let input = "fov:0.0\nlang:en_us\nrenderDistance:12\n";
        let output = set_lang_line(input, "zh_cn");
        assert!(output.contains("lang:zh_cn"));
        assert!(output.contains("fov:0.0"));
        assert!(!output.contains("en_us"));
    }

    #[test]
    fn appends_lang_when_missing() {
        let output = set_lang_line("fov:0.0\n", "zh_cn");
        assert!(output.ends_with("lang:zh_cn\n"));
    }

    #[test]
    fn legacy_versions_use_upper_region() {
        assert!(requires_upper_region("1.8.9"));
        assert!(requires_upper_region("1.7.10"));
        assert!(requires_upper_region("1.10.2"));
        // 1.11–1.12 已经要求 zh_cn（小写区域码）
        assert!(!requires_upper_region("1.12.2"));
        assert!(!requires_upper_region("1.21.1"));
        assert!(!requires_upper_region("neoforge-21.1.265"));
    }

    #[test]
    fn writes_options_file_when_absent() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        ensure_chinese(&dir, "1.21.1").unwrap();

        let content = std::fs::read_to_string(dir.join("options.txt")).unwrap();
        assert!(content.contains("lang:zh_cn"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
