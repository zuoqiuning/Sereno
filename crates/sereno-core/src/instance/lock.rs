//! 实例运行锁：把「哪个实例正在运行」写进实例目录。
//!
//! 内存里的运行注册表在启动器退出后就没了：重启启动器后，正在运行的游戏
//! 会被当作“未运行”，从而允许重复启动同一个实例。这里用带 PID 的锁文件
//! 补上跨进程判断；PID 已退出时自动清理。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::models::settings::AppSettings;
use crate::{LauncherError, Result};

const LOCK_FILE: &str = "running.json";

#[derive(Debug, Serialize, Deserialize)]
struct RunningRecord {
    pid: u32,
    /// 进程创建时间（Windows FILETIME ticks）。
    ///
    /// 只靠 PID 判断「还在跑」并不可靠：Windows 的 PID 回收很快，
    /// 旧 PID 被别的进程占用时，实例会永久显示「运行中」，
    /// 删除 / 改设置 / 备份全部被拦住，用户只能重启启动器。
    /// 加上创建时间后，同一 PID 换了进程也能识别出来。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    started: Option<u64>,
}

fn lock_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".launcher").join(LOCK_FILE)
}

/// 写入运行锁；`pid` 是 Java 进程 PID。
pub fn mark(instance_dir: &Path, pid: u32) -> Result<()> {
    let path = lock_path(instance_dir);
    crate::utils::paths::ensure_dir(path.parent().expect("锁文件必有父目录"))?;
    let record = RunningRecord {
        pid,
        started: process_start_marker(pid),
    };
    let text = serde_json::to_string(&record).map_err(|e| LauncherError::Internal {
        message: format!("序列化运行锁失败: {e}"),
    })?;
    std::fs::write(&path, text).map_err(|e| LauncherError::Io {
        message: format!("写入运行锁失败 {}: {e}", path.display()),
    })
}

/// 删除运行锁；失败不报错（实例可能已经被删掉）。
pub fn clear(instance_dir: &Path) {
    let _ = std::fs::remove_file(lock_path(instance_dir));
}

/// 读锁并校验 PID（+ 进程创建时间）；失效时顺手清掉锁。
pub fn alive_pid(instance_dir: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(lock_path(instance_dir)).ok()?;
    let record: RunningRecord = serde_json::from_str(&text).ok()?;

    if !pid_alive(record.pid) {
        clear(instance_dir);
        return None;
    }

    // PID 还活着，但已经不是当初那个进程 → PID 被复用了
    if let Some(expected) = record.started
        && let Some(actual) = process_start_marker(record.pid)
        && actual != expected
    {
        tracing::debug!(
            pid = record.pid,
            instance = %instance_dir.display(),
            "PID 已被其它进程复用，清除运行锁"
        );
        clear(instance_dir);
        return None;
    }

    Some(record.pid)
}

/// 扫描实例根目录，返回仍活着的实例 ID。
pub fn running_instances(settings: &AppSettings) -> Vec<String> {
    let Ok(root) = crate::utils::paths::instances_root(settings) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().to_string();
            if id.starts_with('.') {
                return None;
            }
            alive_pid(&entry.path()).map(|_| id)
        })
        .collect()
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        // STILL_ACTIVE = 259
        ok != 0 && code == 259
    }
}

#[cfg(not(windows))]
fn pid_alive(pid: u32) -> bool {
    pid != 0 && Path::new(&format!("/proc/{pid}")).exists()
}

/// 进程创建时间，用于识别 PID 复用。拿不到时返回 `None`（退化为只看 PID）。
#[cfg(windows)]
fn process_start_marker(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if handle.is_null() {
            return None;
        }
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
        let ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        Some(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
    }
}

/// Linux：`/proc/<pid>/stat` 第 22 个字段是启动时间（clock ticks）。
#[cfg(target_os = "linux")]
fn process_start_marker(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm 字段可能含空格，从最后一个 ')' 之后再切
    let tail = stat.rsplit_once(')')?.1;
    tail.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(not(any(windows, target_os = "linux")))]
fn process_start_marker(_pid: u32) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_lock_is_cleared_on_read() {
        let dir = tempfile::tempdir().unwrap();
        mark(dir.path(), 999_999_999).unwrap();
        assert!(alive_pid(dir.path()).is_none());
        assert!(!lock_path(dir.path()).exists());
    }

    #[test]
    fn mark_and_clear_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        mark(dir.path(), std::process::id()).unwrap();
        assert!(alive_pid(dir.path()).is_some());
        clear(dir.path());
        assert!(alive_pid(dir.path()).is_none());
    }

    #[test]
    fn running_scan_ignores_plain_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("plain")).unwrap();
        let settings = AppSettings {
            instances_root: Some(root.path().to_string_lossy().to_string()),
            ..AppSettings::default()
        };
        assert!(running_instances(&settings).is_empty());
    }
}
