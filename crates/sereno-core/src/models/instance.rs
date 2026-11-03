//! 实例相关的数据契约。

use serde::{Deserialize, Serialize};

/// 加载器类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LoaderInfo {
    /// 加载器类型：`vanilla` / `fabric` / `forge` / `neoforge`
    pub r#type: String,
    /// 加载器版本（原版为 null）
    pub version: Option<String>,
}

/// 实例元数据（对应 `.launcher/instance.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMetadata {
    /// schema 版本。
    pub schema_version: u32,
    /// 稳定 ID（目录名）。
    pub id: String,
    /// 显示名称。
    pub display_name: String,
    /// Minecraft 版本号，如 `1.21.1`。
    pub game_version: String,
    /// 加载器信息。
    pub loader: LoaderInfo,
    /// 状态：`created` / `installing` / `ready`。
    pub status: String,
    /// 创建时间（Unix 时间戳字符串）。
    pub created_at: String,
    /// 最后游玩时间。
    #[serde(default)]
    pub last_played_at: Option<String>,
    /// 游玩时长（秒）。用 u32 足够，避免 specta 的 u64(BigInt) 限制。
    #[serde(default)]
    pub playtime_secs: u32,
    /// 实例专用 Java 路径；为空时回落到全局设置。
    #[serde(default)]
    pub java_path: Option<String>,
    /// 实例专用最大堆内存（MB）；为空时回落到全局设置。
    #[serde(default)]
    pub memory_mb: Option<u32>,
    /// 实例专用附加 JVM 参数（一行一个，已去掉空行）。
    #[serde(default)]
    pub jvm_args: Vec<String>,
}
