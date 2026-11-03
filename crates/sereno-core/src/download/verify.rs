//! 文件校验：流式 SHA-1、大小校验与增量哈希缓存。
//!
//! ## 为什么要缓存
//!
//! 修复实例会对几百个文件逐个算 SHA-1，文件总量常有几 GB。
//! 但绝大多数文件根本没变——用「路径 + 大小 + 修改时间」做指纹，
//! 命中就直接信任上次的哈希，一次修复从「几 GB IO」降到「几百次元数据查询」。
//!
//! 代价与 PCL 的 `ModHash.ini` 相同：如果内容被替换而大小与 mtime 恰好不变，
//! 缓存会误判。这种情况在正常安装 / 修复流程里不会出现。

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::download::task::Checksum;
use crate::{LauncherError, Result};

/// 缓存条目：大小 + 修改时间（毫秒）足以识别一次文件替换。
#[derive(Clone, Serialize, Deserialize)]
struct CacheEntry {
    size: u64,
    mtime_ms: u64,
    sha1: String,
}

/// 未落盘的写入次数；攒够一批再写，避免每个文件都重写整份 JSON。
const SAVE_INTERVAL: u32 = 32;

static CACHE: LazyLock<Mutex<HashMap<String, CacheEntry>>> =
    LazyLock::new(|| Mutex::new(load_cache()));
static PENDING: AtomicU32 = AtomicU32::new(0);

fn cache_file() -> Option<PathBuf> {
    crate::utils::paths::cache_dir()
        .ok()
        .map(|dir| dir.join("hash-cache.json"))
}

fn load_cache() -> HashMap<String, CacheEntry> {
    let Some(path) = cache_file() else {
        return HashMap::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return HashMap::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_cache_if_needed() {
    let pending = PENDING.fetch_add(1, Ordering::Relaxed) + 1;
    if pending < SAVE_INTERVAL {
        return;
    }
    PENDING.store(0, Ordering::Relaxed);

    let Some(path) = cache_file() else {
        return;
    };
    let Ok(map) = CACHE.lock() else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec(&*map) else {
        return;
    };
    let _ = crate::utils::fs::atomic_write(&path, &bytes);
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// 校验一个已完成下载的文件。
pub fn verify_file(path: &Path, checksum: &Checksum) -> Result<()> {
    let meta = std::fs::metadata(path).map_err(|e| LauncherError::Io {
        message: format!("读取文件元数据失败 {}: {e}", path.display()),
    })?;

    match checksum {
        Checksum::None => Ok(()),
        Checksum::Size(expected) => {
            if meta.len() == *expected {
                Ok(())
            } else {
                Err(LauncherError::ChecksumMismatch {
                    path: path.display().to_string(),
                    expected: format!("{} bytes", expected),
                    actual: format!("{} bytes", meta.len()),
                })
            }
        }
        Checksum::Sha1(expected) => {
            let actual = cached_sha1(path, &meta);
            if actual.to_lowercase() == expected.to_lowercase() {
                Ok(())
            } else {
                Err(LauncherError::ChecksumMismatch {
                    path: path.display().to_string(),
                    expected: expected.clone(),
                    actual,
                })
            }
        }
    }
}

/// 取文件 SHA-1：命中缓存就跳过读取，否则算完写回缓存。
fn cached_sha1(path: &Path, meta: &std::fs::Metadata) -> String {
    let key = path.to_string_lossy().to_string();
    let size = meta.len();
    let mtime = mtime_ms(meta);

    if let Ok(map) = CACHE.lock()
        && let Some(entry) = map.get(&key)
        && entry.size == size
        && entry.mtime_ms == mtime
    {
        return entry.sha1.clone();
    }

    // 缓存未命中或文件已变：重新计算。计算失败时在调用方转为错误更合适，
    // 但这里签名保持 String，失败时返回空串，校验一定不通过。
    let sha1 = compute_sha1(path).unwrap_or_default();

    if let Ok(mut map) = CACHE.lock() {
        map.insert(
            key,
            CacheEntry {
                size,
                mtime_ms: mtime,
                sha1: sha1.clone(),
            },
        );
    }
    save_cache_if_needed();

    sha1
}

/// 计算文件的 SHA-1。
pub fn compute_sha1(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| LauncherError::Io {
        message: format!("打开文件失败 {}: {e}", path.display()),
    })?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];

    loop {
        let n = file.read(&mut buf).map_err(|e| LauncherError::Io {
            message: format!("读取文件失败 {}: {e}", path.display()),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    fn temp_file(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("sereno-verify-{name}-{unique}.bin"))
    }

    #[test]
    fn verifies_sha1_and_size() {
        let path = temp_file("sha1");
        std::fs::write(&path, b"hello").unwrap();
        let expected = {
            let mut hasher = Sha1::new();
            hasher.update(b"hello");
            format!("{:x}", hasher.finalize())
        };

        assert!(verify_file(&path, &Checksum::Sha1(expected.clone())).is_ok());
        assert!(verify_file(&path, &Checksum::Sha1("bad".into())).is_err());
        assert!(verify_file(&path, &Checksum::Size(5)).is_ok());
        assert!(verify_file(&path, &Checksum::Size(6)).is_err());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalidates_cache_when_file_changes() {
        let path = temp_file("invalidate");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"abc").unwrap();
        drop(file);

        let expected = compute_sha1(&path).unwrap();
        assert!(verify_file(&path, &Checksum::Sha1(expected.clone())).is_ok());

        // 同长度改写：只有 mtime 变化能让缓存失效，这正是要验证的点
        std::thread::sleep(Duration::from_millis(30));
        std::fs::write(&path, b"abd").unwrap();
        assert!(
            verify_file(&path, &Checksum::Sha1(expected)).is_err(),
            "同长度改写后仍命中旧哈希，说明缓存没有按 mtime 失效"
        );

        let _ = std::fs::remove_file(&path);
    }
}
