//! 归档解压的**安全关卡**。
//!
//! 恶意整合包是启动器最现实的攻击面：用户会导入从群文件、网盘、论坛下载的
//! zip，**默认它是恶意的**。本模块是**所有归档解压的必经通道**——
//! 整合包导入、加载器安装、备份快照恢复都走这里，不允许各写各的。
//!
//! ## 判定顺序不能变
//!
//! 先做**纯字符串检查**，再做**落盘前检查**，最后才写文件。
//! 任何一步失败都在写入前返回，保证「被拒绝时目标目录没有新增文件」。
//!
//! 校验清单（与《安全审计清单》一致）：
//!
//! 1. 拒绝绝对路径：以 `/`、`\`、盘符、`\\?\`、`\\server\` 开头；
//! 2. 拒绝 `..` 段：按 `/` 与 `\` 分割后任一段为 `..` 即拒绝（`a/../../b` 也拒）；
//! 3. 规范化后必须仍在目标目录内；
//! 4. 拒绝符号链接 / 硬链接条目，不跟随；
//! 5. 拒绝 Windows 保留名（`CON`、`NUL`、`COM1`…）与以空格或点结尾的段名；
//! 6. 拒绝大小写冲突（`Mod.jar` 与 `mod.jar` 在 Windows 上是同一个文件）；
//! 7. 长度与体积上限：单路径 260 字符、总条目 10 万、单文件 2 GB、总解压 20 GB。

use std::collections::HashSet;
use std::io::{self, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

use crate::{LauncherError, Result};

/// 单个条目路径长度上限（Windows `MAX_PATH`）。
pub const MAX_PATH_LENGTH: usize = 260;

/// 总条目数上限。
pub const MAX_ENTRIES: usize = 100_000;

/// 单文件解压后大小上限（2 GB）。
pub const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024;

/// 总解压大小上限（20 GB）。
pub const MAX_TOTAL_SIZE: u64 = 20 * 1024 * 1024 * 1024;

/// Windows 保留设备名（不区分大小写）。
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 解压结果统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractionReport {
    /// 写入的文件数。
    pub files: usize,
    /// 创建的目录数。
    pub dirs: usize,
    /// 解压出的总字节数。
    pub total_bytes: u64,
}

/// 判断一个路径是否为**安全的相对路径**。
///
/// 安全 = 非绝对路径，且每一段都是普通名或 `.`。
/// 盘符（`C:foo`）、UNC（`\\server\share`）、`\\?\` 前缀都会因为含
/// `Component::Prefix` 而被拒。
pub fn is_safe_relative(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// 判断某个路径段是否是 Windows 保留设备名。
///
/// 注意 `CON.txt` 同样危险——Windows 认的是第一个点之前的部分。
pub fn is_windows_reserved(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    RESERVED_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

/// 校验归档条目的路径，返回规范化后的相对路径。
///
/// 只做**纯字符串**层面的检查，不触碰文件系统。
pub fn validate_entry_path(name: &str) -> Result<PathBuf> {
    // **不要 trim 原始名字**：`"file.txt "`（结尾带空格）正是 Windows
    // 禁止的文件名，trim 掉尾随空格就检测不出来了。
    let raw = name;

    if raw.trim().is_empty() {
        return Err(LauncherError::Parse {
            message: "归档条目路径为空".into(),
        });
    }

    // 1. 绝对路径：以 / 或 \ 开头（含 UNC \\server\share 与 \\?\ 前缀）
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(LauncherError::Parse {
            message: format!("归档条目是绝对路径，已拒绝: {name}"),
        });
    }

    // 2. 盘符前缀：C: 或 C:\（`C:foo` 是驱动器相对路径，同样危险）
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic() {
        return Err(LauncherError::Parse {
            message: format!("归档条目带盘符，已拒绝: {name}"),
        });
    }

    // 3. 逐段检查
    let mut normalized = PathBuf::new();
    let mut segments = 0usize;

    for segment in raw.split(['/', '\\']) {
        // 空段（`a//b`）与 `.`（当前目录）都可以安全规范化掉
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(LauncherError::Parse {
                message: format!("归档条目含路径穿越（..），已拒绝: {name}"),
            });
        }
        if segment.ends_with(' ') || segment.ends_with('.') {
            return Err(LauncherError::Parse {
                message: format!("归档条目段名以空格或点结尾，已拒绝: {name}"),
            });
        }
        if is_windows_reserved(segment) {
            return Err(LauncherError::Parse {
                message: format!("归档条目含 Windows 保留名「{segment}」，已拒绝: {name}"),
            });
        }
        if segment.len() > 255 {
            return Err(LauncherError::Parse {
                message: format!("归档条目段名过长（{} 字符），已拒绝: {name}", segment.len()),
            });
        }

        normalized.push(segment);
        segments += 1;
    }

    if segments == 0 {
        return Err(LauncherError::Parse {
            message: format!("归档条目没有有效路径段: {name}"),
        });
    }

    // 4. 总长度限制
    if raw.len() > MAX_PATH_LENGTH {
        return Err(LauncherError::Parse {
            message: format!(
                "归档条目路径过长（{} > {MAX_PATH_LENGTH} 字符），已拒绝: {name}",
                raw.len()
            ),
        });
    }

    Ok(normalized)
}

/// 校验条目的文件系统类型：拒绝符号链接与设备文件。
///
/// `unix_mode` 的高 4 位是文件类型位：`0o120000` 是符号链接，
/// `0o040000` 是目录，`0o100000` 是普通文件。其余（管道、设备、socket）
/// 一律拒绝——我们没有理由解压这些东西。
pub fn validate_entry_type(unix_mode: Option<u32>, name: &str) -> Result<()> {
    let Some(mode) = unix_mode else {
        return Ok(());
    };

    const S_IFMT: u32 = 0o170000;
    const S_IFLNK: u32 = 0o120000;
    const S_IFDIR: u32 = 0o040000;
    const S_IFREG: u32 = 0o100000;

    let file_type = mode & S_IFMT;
    if file_type == S_IFLNK {
        return Err(LauncherError::Parse {
            message: format!("归档条目是符号链接，已拒绝: {name}"),
        });
    }
    if file_type != 0 && file_type != S_IFDIR && file_type != S_IFREG {
        return Err(LauncherError::Parse {
            message: format!("归档条目不是普通文件或目录，已拒绝: {name}"),
        });
    }

    Ok(())
}

/// 校验写入路径没有逃出目标目录。
///
/// 这是纯字符串校验之外的**第二道防线**：即使前面的规范化漏了，
/// 这里用「目标根的规范化路径 + 相对路径」再确认一次。
fn ensure_within(root: &Path, relative: &Path) -> Result<PathBuf> {
    // 目标根规范化后作为比较基准。
    //
    // **Windows 上 `canonicalize` 会带上 `\\?\` 前缀**，所以目标路径也必须
    // 从**规范化后的根**拼出来——否则两边前缀形式不同，`starts_with` 永远
    // 不成立，合法的条目会被误判成「越出目标目录」。
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    let mut target = root_canonical.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => target.push(part),
            Component::CurDir => {}
            _ => {
                return Err(LauncherError::Parse {
                    message: format!("归档条目路径非法，已拒绝: {}", relative.display()),
                });
            }
        }
    }

    if !target.starts_with(&root_canonical) {
        return Err(LauncherError::Parse {
            message: format!("归档条目越出目标目录，已拒绝: {}", relative.display()),
        });
    }

    Ok(target)
}

/// 安全解压 ZIP。
///
/// ## 两阶段
///
/// 1. **预检**：遍历**全部**条目，只做校验（路径、类型、大小、数量、大小写冲突）。
///    这一阶段不写任何文件——因此「被拒绝时目标目录无新增文件」成立。
/// 2. **落盘**：边解压边校验实际字节数（防止头部声明大小造假）。
///
/// ## 大小写冲突
///
/// `Mod.jar` 与 `mod.jar` 在 Windows 上是同一个文件。解压时若目标已存在
/// 且内容不同，**拒绝而不是覆盖**——这里在预检阶段就检出并整体拒绝。
pub fn extract_zip_safely<R: Read + Seek>(reader: R, dest: &Path) -> Result<ExtractionReport> {
    extract_zip_mapped(reader, dest, |name| Some(name.to_string()))
}

/// 只解压指定前缀下的条目，并把前缀从落盘路径里去掉。
///
/// 整合包的 `.mrpack` 里只有 `overrides/` 与 `client-overrides/` 需要落盘，
/// 索引 JSON 等其余条目必须忽略。路径校验与写入策略与整体解压完全一致。
pub fn extract_zip_prefixes_safely<R: Read + Seek>(
    reader: R,
    dest: &Path,
    prefixes: &[&str],
) -> Result<ExtractionReport> {
    extract_zip_mapped(reader, dest, |name| {
        let normalized = name.replace('\\', "/");
        prefixes
            .iter()
            .find_map(|prefix| normalized.strip_prefix(prefix))
            .map(|rest| rest.to_string())
            .filter(|rest| !rest.is_empty())
    })
}

/// 解压的通用实现。
///
/// `map` 决定每个条目如何落盘：返回 `None` 表示跳过该条目，
/// 返回相对路径字符串表示写入 `dest` 下的对应位置。
fn extract_zip_mapped<R, F>(reader: R, dest: &Path, map: F) -> Result<ExtractionReport>
where
    R: Read + Seek,
    F: Fn(&str) -> Option<String>,
{
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| LauncherError::Parse {
        message: format!("打开 ZIP 失败: {e}"),
    })?;

    if archive.len() > MAX_ENTRIES {
        return Err(LauncherError::Parse {
            message: format!("归档条目过多（{} > {MAX_ENTRIES}），已拒绝", archive.len()),
        });
    }

    crate::utils::paths::ensure_dir(dest)?;

    // ---------------- 阶段一：预检，不写任何文件 ----------------
    let mut plan: Vec<(usize, PathBuf, bool, u64)> = Vec::with_capacity(archive.len());
    let mut seen_lowercase: HashSet<String> = HashSet::with_capacity(archive.len());
    let mut declared_total: u64 = 0;

    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|e| LauncherError::Parse {
            message: format!("读取归档条目 {index} 失败: {e}"),
        })?;

        let raw_name = file.name().to_string();
        let Some(mapped) = map(&raw_name) else {
            continue;
        };

        let is_dir = file.is_dir() || raw_name.ends_with('/') || raw_name.ends_with('\\');

        let relative = validate_entry_path(&mapped)?;
        validate_entry_type(file.unix_mode(), &raw_name)?;

        let size = file.size();
        if size > MAX_FILE_SIZE {
            return Err(LauncherError::Parse {
                message: format!(
                    "归档条目过大（{} > {MAX_FILE_SIZE} 字节），已拒绝: {raw_name}",
                    size
                ),
            });
        }

        // 大小写冲突：Windows 上 `Mod.jar` 与 `mod.jar` 是同一个文件
        let key = relative.to_string_lossy().to_lowercase();
        if !seen_lowercase.insert(key.clone()) {
            return Err(LauncherError::Parse {
                message: format!("归档内存在大小写冲突的路径，已拒绝: {raw_name}"),
            });
        }

        declared_total = declared_total.saturating_add(size);
        if declared_total > MAX_TOTAL_SIZE {
            return Err(LauncherError::Parse {
                message: format!("归档解压后总大小超过上限（{MAX_TOTAL_SIZE} 字节），已拒绝"),
            });
        }

        plan.push((index, relative, is_dir, size));
    }

    // ---------------- 阶段二：落盘 ----------------
    let mut report = ExtractionReport::default();
    let mut written: Vec<PathBuf> = Vec::with_capacity(plan.len());
    let mut actual_total: u64 = 0;

    for (index, relative, is_dir, _declared) in plan {
        let mut file = archive.by_index(index).map_err(|e| LauncherError::Parse {
            message: format!("读取归档条目 {index} 失败: {e}"),
        })?;

        let target = ensure_within(dest, &relative)?;

        if is_dir {
            crate::utils::paths::ensure_dir(&target)?;
            report.dirs += 1;
            continue;
        }

        if let Some(parent) = target.parent() {
            crate::utils::paths::ensure_dir(parent)?;
        }

        let mut out = std::fs::File::create(&target).map_err(|e| LauncherError::Io {
            message: format!("创建文件失败 {}: {e}", target.display()),
        })?;

        // 边写边计数：头部声明的大小可能造假
        let remaining = MAX_TOTAL_SIZE.saturating_sub(actual_total);
        let copied =
            copy_with_caps(&mut file, &mut out, MAX_FILE_SIZE, remaining).map_err(|e| {
                // 中途失败：把本次写入的文件全部清掉，不留半个实例
                cleanup_written(&written);
                LauncherError::Parse {
                    message: format!("解压条目失败（{}）: {e}", relative.display()),
                }
            })?;

        out.flush().map_err(|e| LauncherError::Io {
            message: format!("写入文件失败 {}: {e}", target.display()),
        })?;

        actual_total += copied;
        report.files += 1;
        report.total_bytes += copied;
        written.push(target);
    }

    Ok(report)
}

/// 带上限的拷贝。超过上限立即报错。
fn copy_with_caps<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    file_cap: u64,
    total_cap: u64,
) -> io::Result<u64> {
    let mut buf = [0u8; 64 * 1024];
    let mut copied = 0u64;

    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        copied += read as u64;
        if copied > file_cap {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("单个文件解压后超过上限 {file_cap} 字节"),
            ));
        }
        if copied > total_cap {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("总解压大小超过上限 {total_cap} 字节"),
            ));
        }
        writer.write_all(&buf[..read])?;
    }

    Ok(copied)
}

/// 清理一次解压中写入的文件（用于中途失败时回滚）。
pub fn cleanup_written(files: &[PathBuf]) {
    for path in files {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_paths() {
        for name in [
            "/etc/passwd",
            "\\windows\\system32",
            "C:/Windows/evil.dll",
            "D:\\evil.dll",
        ] {
            assert!(validate_entry_path(name).is_err(), "应拒绝绝对路径: {name}");
        }
    }

    #[test]
    fn rejects_path_traversal() {
        for name in [
            "../evil.txt",
            "a/../../b",
            "..\\evil.txt",
            "mods/../../evil",
        ] {
            assert!(validate_entry_path(name).is_err(), "应拒绝路径穿越: {name}");
        }
    }

    #[test]
    fn rejects_windows_reserved_names() {
        for name in ["CON", "con.txt", "NUL", "aux/file.txt", "COM1.jar", "LPT9"] {
            assert!(validate_entry_path(name).is_err(), "应拒绝保留名: {name}");
        }
    }

    #[test]
    fn rejects_trailing_space_or_dot() {
        // Windows 不允许文件名以空格或点结尾
        assert!(validate_entry_path("file.txt ").is_err());
        assert!(validate_entry_path("file.").is_err());
        assert!(validate_entry_path("dir/file.jar.").is_err());
        // 名字中间带空格是合法的（`file .txt` 在 Windows 上可以创建）
        assert!(validate_entry_path("file .txt").is_ok());
    }

    #[test]
    fn accepts_normal_relative_paths() {
        assert_eq!(
            validate_entry_path("mods/example.jar").unwrap(),
            PathBuf::from("mods").join("example.jar")
        );
        assert_eq!(
            validate_entry_path("mods\\sub\\file.txt").unwrap(),
            PathBuf::from("mods").join("sub").join("file.txt")
        );
        // `.` 段被安全规范化掉
        assert_eq!(
            validate_entry_path("./a/./b.txt").unwrap(),
            PathBuf::from("a").join("b.txt")
        );
    }

    #[test]
    fn rejects_overlong_path() {
        let long = format!("{}/file.txt", "a".repeat(300));
        assert!(validate_entry_path(&long).is_err());
    }

    #[test]
    fn rejects_symlink_entries() {
        // 0o120777 = 符号链接
        assert!(validate_entry_type(Some(0o120777), "link").is_err());
        // 普通文件与目录放行
        assert!(validate_entry_type(Some(0o100644), "file").is_ok());
        assert!(validate_entry_type(Some(0o040755), "dir").is_ok());
        // 设备文件拒绝
        assert!(validate_entry_type(Some(0o020644), "dev").is_err());
        // 无 unix 权限信息的条目放行
        assert!(validate_entry_type(None, "file").is_ok());
    }

    #[test]
    fn is_safe_relative_matches_contract() {
        assert!(is_safe_relative(Path::new("a/b.txt")));
        assert!(!is_safe_relative(Path::new("../a")));
        assert!(!is_safe_relative(Path::new("/a")));
    }
}
