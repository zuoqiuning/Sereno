//! 实例元数据的读写。

use std::path::Path;

use crate::models::instance::InstanceMetadata;
use crate::utils::fs::atomic_write_str;
use crate::{LauncherError, Result};

/// 从指定路径读取实例元数据。
pub fn read_metadata(dir: &Path) -> Result<InstanceMetadata> {
    let path = dir.join(".launcher").join("instance.json");
    let raw = std::fs::read_to_string(&path).map_err(|e| LauncherError::Io {
        message: format!("读取实例元数据失败 {}: {e}", path.display()),
    })?;

    serde_json::from_str(&raw).map_err(|e| LauncherError::Parse {
        message: format!("解析实例元数据失败 {}: {e}", path.display()),
    })
}

/// 将实例元数据写入磁盘（原子写）。
pub fn write_metadata(dir: &Path, metadata: &InstanceMetadata) -> Result<()> {
    let path = dir.join(".launcher").join("instance.json");
    let mut json = serde_json::to_string_pretty(metadata).map_err(|e| LauncherError::Parse {
        message: format!("序列化实例元数据失败: {e}"),
    })?;
    json.push('\n');
    atomic_write_str(&path, &json)
}
