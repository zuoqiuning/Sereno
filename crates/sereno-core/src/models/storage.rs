use serde::{Deserialize, Serialize};

/// 单个实例的磁盘占用。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceUsage {
    pub id: String,
    pub name: String,
    #[specta(type = specta_typescript::Number)]
    pub bytes: u64,
}

/// 启动器的磁盘占用概览。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    /// 按体积从大到小排列的实例列表。
    pub instances: Vec<InstanceUsage>,
    #[specta(type = specta_typescript::Number)]
    pub instance_bytes: u64,
    /// 元数据缓存（搜索页、版本列表、哈希缓存）。
    #[specta(type = specta_typescript::Number)]
    pub cache_bytes: u64,
    /// 启动器日志。
    #[specta(type = specta_typescript::Number)]
    pub log_bytes: u64,
}
