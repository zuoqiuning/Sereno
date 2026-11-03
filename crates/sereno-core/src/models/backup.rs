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
