//! 文件名清理、原子写入、目录操作。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{LauncherError, Result};

/// 原子写入的临时文件序号。
///
/// **同一进程内并发写同一目标路径时，临时文件名必须互不相同**，
/// 否则两个线程会写到同一个临时文件、互相覆盖，最终落盘内容是其中一个的
/// 片段而不是完整内容。只用 PID 不够——PID 在进程内是常量。
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Windows 保留文件名（不区分大小写）。
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 把任意字符串清理成安全的文件名。
///
/// - 替换 Windows 非法字符 `< > : " / \ | ? *` 与控制字符
/// - 去掉首尾空白与结尾的点（Windows 不允许）
/// - 避开保留设备名
/// - 空结果回退为 `unnamed`
pub fn sanitize_file_name(raw: &str) -> String {
    let mut cleaned: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();

    cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();

    if cleaned.is_empty() {
        return "unnamed".to_string();
    }

    let stem_upper = cleaned
        .split('.')
        .next()
        .unwrap_or(&cleaned)
        .to_ascii_uppercase();
    if RESERVED_NAMES.contains(&stem_upper.as_str()) {
        cleaned.insert(0, '_');
    }

    cleaned
}

/// 原子写入：先写同目录临时文件，再 `rename` 覆盖目标。
///
/// 同目录是必要的——跨卷的 `rename` 会失败。
///
/// 临时文件名由 **PID + 进程内自增序号** 组成：
/// 只用 PID 的话，同进程内并发写同一目标路径的两个线程会拿到**完全相同**的
/// 临时文件名，互相覆盖，落盘结果是残缺内容。序号保证每次调用都唯一。
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| LauncherError::Io {
        message: format!("目标路径没有父目录: {}", path.display()),
    })?;

    if !dir.exists() {
        crate::utils::paths::ensure_dir(dir)?;
    }

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tmp".to_string());

    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path: PathBuf = dir.join(format!(".{file_name}.{}.{seq}.tmp", std::process::id()));

    std::fs::write(&tmp_path, contents).map_err(|e| LauncherError::Io {
        message: format!("写入临时文件失败 {}: {e}", tmp_path.display()),
    })?;

    // rename 在 Windows 上会覆盖已存在的目标，符合原子替换语义
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        // 清理临时文件，避免残留垃圾
        let _ = std::fs::remove_file(&tmp_path);
        return Err(LauncherError::Io {
            message: format!("原子替换失败 {}: {e}", path.display()),
        });
    }

    Ok(())
}

/// 原子写入 UTF-8 文本。
pub fn atomic_write_str(path: &Path, contents: &str) -> Result<()> {
    atomic_write(path, contents.as_bytes())
}

/// 递归删除目录（仅用于临时目录等自建路径，**不用于用户实例**）。
///
/// 实例删除必须走 `trash`，把文件移入回收站，而不是直接抹掉。
pub fn remove_dir_all(path: &Path) -> Result<()> {
    std::fs::remove_dir_all(path).map_err(|e| LauncherError::Io {
        message: format!("删除目录失败 {}: {e}", path.display()),
    })
}

/// 目录体积（字节）。用于配额检查与诊断包裁剪。
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            total += dir_size(&entry.path());
        } else {
            total += meta.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_illegal_chars() {
        assert_eq!(sanitize_file_name("a/b:c"), "a_b_c");
        assert_eq!(sanitize_file_name("  spaced  "), "spaced");
        assert_eq!(sanitize_file_name("trailing."), "trailing");
    }

    #[test]
    fn escapes_reserved_names() {
        assert_eq!(sanitize_file_name("CON"), "_CON");
        assert_eq!(sanitize_file_name("nul.txt"), "_nul.txt");
    }

    #[test]
    fn empty_falls_back() {
        assert_eq!(sanitize_file_name("   "), "unnamed");
        assert_eq!(sanitize_file_name("..."), "unnamed");
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = std::env::temp_dir().join(format!("sereno-fs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("a.json");

        atomic_write_str(&target, "{\"v\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"v\":1}");

        atomic_write_str(&target, "{\"v\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"v\":2}");

        // 不应残留临时文件
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "残留临时文件");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_atomic_writes_do_not_collide() {
        let dir = std::env::temp_dir().join(format!("sereno-fs-race-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("race.json");

        // 两个线程并发写同一目标：临时文件名必须不同，否则会互相覆盖，
        // 最终内容会是两段内容的拼接而不是其中之一。
        let payloads = [vec![b'a'; 64 * 1024], vec![b'b'; 64 * 1024]];
        let handles: Vec<_> = payloads
            .iter()
            .map(|payload| {
                let target = target.clone();
                let payload = payload.clone();
                std::thread::spawn(move || {
                    atomic_write(&target, &payload).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let final_content = std::fs::read(&target).unwrap();
        assert_eq!(
            final_content.len(),
            64 * 1024,
            "落盘内容长度不对，说明临时文件被互相覆盖了"
        );
        assert!(
            final_content.iter().all(|b| *b == b'a') || final_content.iter().all(|b| *b == b'b'),
            "落盘内容应由其中一次写入完整构成"
        );

        // 不应残留临时文件
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "残留临时文件");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
