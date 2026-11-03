//! 进程内 TTL 缓存。
//!
//! 搜索页、双源总数、CurseForge 响应与资源版本列表共用这一份实现：
//! 统一「过期 + 容量上限 + 最近最少使用淘汰」，避免每处各写一套 HashMap。
//!
//! **只在内存里**，进程退出即清空；启动器的搜索缓存不需要落盘。

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Entry<V> {
    value: V,
    expires_at: Instant,
    last_access: Instant,
}

pub struct TtlCache<K, V> {
    inner: Mutex<HashMap<K, Entry<V>>>,
    ttl: Duration,
    capacity: usize,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl,
            capacity: capacity.max(1),
        }
    }

    /// 命中时刷新「最近使用」，过期条目顺手删除。
    pub fn get(&self, key: &K) -> Option<V> {
        let Ok(mut cache) = self.inner.lock() else {
            return None;
        };
        let now = Instant::now();

        match cache.get(key) {
            Some(entry) if entry.expires_at > now => {}
            Some(_) => {
                cache.remove(key);
                return None;
            }
            None => return None,
        }

        let entry = cache.get_mut(key)?;
        entry.last_access = now;
        Some(entry.value.clone())
    }

    pub fn insert(&self, key: K, value: V) {
        let Ok(mut cache) = self.inner.lock() else {
            return;
        };
        let now = Instant::now();

        cache.retain(|_, entry| entry.expires_at > now);
        while cache.len() >= self.capacity {
            let oldest = cache
                .iter()
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(key, _)| key.clone());
            match oldest {
                Some(oldest) => {
                    cache.remove(&oldest);
                }
                None => break,
            }
        }

        cache.insert(
            key,
            Entry {
                value,
                expires_at: now + self.ttl,
                last_access: now,
            },
        );
    }

    pub fn remove(&self, key: &K) {
        if let Ok(mut cache) = self.inner.lock() {
            cache.remove(key);
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|cache| cache.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_within_ttl() {
        let cache = TtlCache::new(Duration::from_secs(60), 8);
        cache.insert("key", 42);
        assert_eq!(cache.get(&"key"), Some(42));
        assert_eq!(cache.get(&"missing"), None);
    }

    #[test]
    fn evicts_least_recently_used_when_full() {
        let cache = TtlCache::new(Duration::from_secs(60), 2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        // 访问 a，让它变成「最近使用」，随后插入 c 时应淘汰 b。
        assert_eq!(cache.get(&"a"), Some(1));
        cache.insert("c", 3);

        assert_eq!(cache.get(&"c"), Some(3));
        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), None);
        assert_eq!(cache.len(), 2);
    }
}
