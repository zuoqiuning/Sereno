use serde::{Deserialize, Serialize};

/// 一次世界（存档）备份。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldBackup {
    /// 备份文件名，恢复时原样传回。
    pub name: String,
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
    /// Unix 时间戳（秒）。
    #[specta(type = specta_typescript::Number)]
    pub created_at: u64,
}

/// 一次内容快照（`mods` / `config` / `resourcepacks` / `shaderpacks`）。
///
/// 与 [`WorldBackup`] 的区别：世界备份保的是玩家的**存档**，内容快照保的是
/// 玩家的**模组与配置**。两者触发时机、体积、保留策略完全不同，不能合并。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentSnapshot {
    /// 快照 ID（目录名），恢复 / 删除时原样传回。
    pub id: String,
    /// 触发原因：`manual` / `before-mod-change` / `before-loader-change`。
    pub reason: String,
    /// Unix 时间戳（秒）。
    #[specta(type = specta_typescript::Number)]
    pub created_at: u64,
    /// 压缩包大小（字节）。
    #[specta(type = specta_typescript::Number)]
    pub size_bytes: u64,
    /// 快照内的文件数。
    pub file_count: u32,
    /// 手动标记为「保留」，不参与自动淘汰。
    pub pinned: bool,
}
