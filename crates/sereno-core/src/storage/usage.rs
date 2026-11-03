//! 磁盘占用统计与缓存清理。
//!
//! 缓存目录按 namespace 分层，用户不需要知道细节；
//! 这里只给三个能看懂的桶：实例、缓存、日志。

use crate::models::settings::AppSettings;
use crate::models::storage::{InstanceUsage, StorageUsage};
use crate::utils::fs::dir_size;
use crate::utils::paths::{cache_dir, instance_dir, instances_root, logs_dir};
use crate::{LauncherError, Result};

pub fn usage(settings: &AppSettings) -> Result<StorageUsage> {
    let mut instances = Vec::new();
    for metadata in crate::instance::lifecycle::list_instances(settings)? {
        let bytes = instance_dir(settings, &metadata.id)
            .map(|dir| dir_size(&dir))
            .unwrap_or(0);
        instances.push(InstanceUsage {
            id: metadata.id,
            name: metadata.display_name,
            bytes,
        });
    }
    instances.sort_by_key(|item| std::cmp::Reverse(item.bytes));

    let instance_bytes = instances.iter().map(|item| item.bytes).sum();
    let cache_bytes = cache_dir().map(|dir| dir_size(&dir)).unwrap_or(0);
    let log_bytes = logs_dir().map(|dir| dir_size(&dir)).unwrap_or(0);

    Ok(StorageUsage {
        instances,
        instance_bytes,
        cache_bytes,
        log_bytes,
    })
}

/// 清空缓存目录里的所有内容（目录本身保留）。
pub fn clear_cache() -> Result<()> {
    let dir = cache_dir()?;
    if !dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(&dir).map_err(|error| LauncherError::Io {
        message: format!("读取缓存目录失败 {}: {error}", dir.display()),
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        let _ = result;
    }
    Ok(())
}

/// 实例根目录（设置页要展示它，避免用户找不到文件）。
pub fn instances_path(settings: &AppSettings) -> Result<String> {
    Ok(instances_root(settings)?.display().to_string())
}
