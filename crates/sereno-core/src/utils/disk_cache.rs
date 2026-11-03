//! 带 TTL 与容量上限的磁盘缓存。
//!
//! 只用于公开的搜索 / 版本元数据，不存 Token、账号信息或下载文件。
//! 写入使用原子替换，避免进程崩溃留下半个 JSON。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::utils::fs::atomic_write;
use crate::utils::paths::ensure_dir;
use crate::{LauncherError, Result};

#[derive(Serialize, Deserialize)]
struct Entry<T> {
    expires_at: u64,
    value: T,
}

pub struct DiskCache {
    dir: PathBuf,
    ttl: Duration,
    capacity: usize,
    count: AtomicUsize,
    /// 字节上限；0 表示只按条目数限制。
    max_total_bytes: u64,
    total_bytes: AtomicU64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

impl DiskCache {
    pub fn new(namespace: &str, ttl: Duration, capacity: usize) -> Result<Self> {
        Self::new_with_cap(namespace, ttl, capacity, 0)
    }

    /// 带字节上限的构造：条目数与总字节数任一超限都会触发清理。
    pub fn new_with_cap(
        namespace: &str,
        ttl: Duration,
        capacity: usize,
        max_total_bytes: u64,
    ) -> Result<Self> {
        let dir = crate::utils::paths::cache_dir()?.join(namespace);
        Self::new_at_with_cap(dir, ttl, capacity, max_total_bytes)
    }

    pub fn new_at(dir: PathBuf, ttl: Duration, capacity: usize) -> Result<Self> {
        Self::new_at_with_cap(dir, ttl, capacity, 0)
    }

    pub fn new_at_with_cap(
        dir: PathBuf,
        ttl: Duration,
        capacity: usize,
        max_total_bytes: u64,
    ) -> Result<Self> {
        ensure_dir(&dir)?;
        let mut count = 0usize;
        let mut total_bytes = 0u64;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                count += 1;
                total_bytes += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            }
        }
        Ok(Self {
            dir,
            ttl,
            capacity: capacity.max(1),
            count: AtomicUsize::new(count),
            max_total_bytes,
            total_bytes: AtomicU64::new(total_bytes),
        })
    }

    fn path_for(&self, key: &str) -> PathBuf {
        let hash = format!("{:x}", Sha1::digest(key.as_bytes()));
        self.dir.join(format!("{hash}.json"))
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let path = self.path_for(key);
        let bytes = std::fs::read(&path).ok()?;
        let entry: Entry<T> = serde_json::from_slice(&bytes).ok()?;

        if entry.expires_at <= now_secs() {
            let freed = bytes.len() as u64;
            let _ = std::fs::remove_file(&path);
            let _ = self
                .count
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_sub(1)
                });
            let _ = self
                .total_bytes
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_sub(freed)
                });
            return None;
        }

        Some(entry.value)
    }

    pub fn insert<T: Serialize>(&self, key: &str, value: &T) {
        let entry = Entry {
            expires_at: now_secs() + self.ttl.as_secs(),
            value,
        };
        let Ok(bytes) = serde_json::to_vec(&entry) else {
            return;
        };

        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        let over_count = self.count.load(Ordering::Relaxed) > self.capacity + 64;
        let over_bytes = self.max_total_bytes > 0
            && self.total_bytes.load(Ordering::Relaxed) > self.max_total_bytes;
        if over_count || over_bytes {
            self.prune();
        }

        let path = self.path_for(key);
        if let Err(error) = atomic_write(&path, &bytes) {
            tracing::warn!(path = %path.display(), %error, "写入磁盘缓存失败");
        }
    }

    fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };

        let now = now_secs();
        let mut files: Vec<(u64, u64, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                let _ = std::fs::remove_file(&path);
                continue;
            };
            let expires_at = value
                .get("expires_at")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            if expires_at <= now {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            let size = bytes.len() as u64;
            let modified = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            files.push((modified, size, path));
        }

        files.sort_by_key(|(modified, _, _)| *modified);

        // 条目数与总字节数任一超限都要清理：条目数管不住体积，
        // 一个版本列表 JSON 可能是搜索页的十倍。
        let mut total_bytes: u64 = files.iter().map(|(_, size, _)| *size).sum();
        while !files.is_empty()
            && (files.len() >= self.capacity
                || (self.max_total_bytes > 0 && total_bytes > self.max_total_bytes))
        {
            let (_, size, path) = files.remove(0);
            let _ = std::fs::remove_file(path);
            total_bytes = total_bytes.saturating_sub(size);
        }

        self.count.store(files.len(), Ordering::Relaxed);
        self.total_bytes.store(total_bytes, Ordering::Relaxed);
    }

    pub fn clear(&self) -> Result<()> {
        let entries = std::fs::read_dir(&self.dir).map_err(|error| LauncherError::Io {
            message: format!("读取缓存目录失败: {error}"),
        })?;
        for entry in entries.flatten() {
            if entry.path().is_file() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        self.count.store(0, Ordering::Relaxed);
        self.total_bytes.store(0, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("sereno-disk-cache-{name}-{unique}"))
    }

    #[test]
    fn round_trips_and_respects_ttl() {
        let cache = DiskCache::new_at(temp_dir("ttl"), Duration::from_secs(60), 8).unwrap();
        cache.insert("key", &vec![1, 2, 3]);
        assert_eq!(cache.get::<Vec<i32>>("key"), Some(vec![1, 2, 3]));

        let expired = DiskCache::new_at(temp_dir("expired"), Duration::from_secs(0), 8).unwrap();
        expired.insert("key", &42);
        assert_eq!(expired.get::<i32>("key"), None);
    }

    #[test]
    fn prunes_when_total_bytes_exceed_the_cap() {
        let dir = temp_dir("bytes");
        let cache =
            DiskCache::new_at_with_cap(dir.clone(), Duration::from_secs(60), 100, 300).unwrap();

        for index in 0..5 {
            cache.insert(&format!("key{index}"), &vec![0u8; 100]);
        }

        let remaining = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().is_file())
            .count();
        assert!(
            remaining <= 3,
            "字节上限没有生效，仍保留了 {remaining} 个条目"
        );
    }
}
