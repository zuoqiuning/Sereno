//! 实例元数据的读写。

use std::path::Path;

use crate::models::instance::InstanceMetadata;
use crate::utils::fs::atomic_write_str;
use crate::{LauncherError, Result};

/// 实例元数据的当前 schema 版本。
///
/// 与 `settings.rs` 的迁移模式对齐：新增**带默认值的字段**不需要迁移
/// （serde default 兜底）；未来出现「旧数据无法表达」的必填字段时，
/// 在 [`migrate`] 里按版本补链，避免旧实例反序列化失败后从列表消失。
pub const SCHEMA_VERSION: u32 = 1;

/// 从指定路径读取实例元数据（含 schema 迁移）。
pub fn read_metadata(dir: &Path) -> Result<InstanceMetadata> {
    let path = dir.join(".launcher").join("instance.json");
    let raw = std::fs::read_to_string(&path).map_err(|e| LauncherError::Io {
        message: format!("读取实例元数据失败 {}: {e}", path.display()),
    })?;

    let mut metadata: InstanceMetadata =
        serde_json::from_str(&raw).map_err(|e| LauncherError::Parse {
            message: format!("解析实例元数据失败 {}: {e}", path.display()),
        })?;

    let changed = migrate(&mut metadata);
    if changed {
        // 迁移产生变化就立刻落盘，避免每次读取都重复迁移
        write_metadata(dir, &metadata)?;
    }
    Ok(metadata)
}

/// 实例元数据迁移链（与 `settings.rs::migrate` 对齐）。
///
/// 返回是否产生了需要落盘的变化。
fn migrate(metadata: &mut InstanceMetadata) -> bool {
    let version = metadata.schema_version;

    if version > SCHEMA_VERSION {
        tracing::warn!(
            schema_version = version,
            supported = SCHEMA_VERSION,
            instance = %metadata.id,
            "实例元数据的 schema 版本高于当前程序，已按原样保留"
        );
        return false;
    }

    // 迁移链示例：
    // if version < 2 {
    //     metadata.some_new_field = ...;
    // }

    if version != SCHEMA_VERSION {
        metadata.schema_version = SCHEMA_VERSION;
        return true;
    }
    false
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
