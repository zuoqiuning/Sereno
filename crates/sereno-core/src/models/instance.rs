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
    ///
    /// 必须带 default：迁移函数只在反序列化成功后才执行，裸字段遇到
    /// 旧文件（字段缺失）会直接反序列化失败，实例从列表里消失——
    /// 迁移根本没机会跑。default 返回 1（初始 schema）。
    #[serde(default = "default_schema_version")]
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
    /// 是否被用户「收藏」：列表里置顶显示。
    ///
    /// 老实例的 `instance.json` 里没有这个字段，`default` 保证能正常读取。
    #[serde(default)]
    pub favorite: bool,
    /// 分组名；`None` = 未分组。纯展示用元数据，不参与路径。
    ///
    /// 老实例同样靠 `default` 兼容。
    #[serde(default)]
    pub group: Option<String>,
}

fn default_schema_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_without_schema_version_defaults_and_migrates() {
        // 模拟「schema_version 字段加入之前」的旧 instance.json：
        // 反序列化必须成功，且迁移链把它视作 v1
        let json = serde_json::json!({
            "id": "abc123",
            "displayName": "测试实例",
            "gameVersion": "1.21.1",
            "loader": { "type": "vanilla", "version": null },
            "status": "ready",
            "createdAt": "1700000000",
            "playtimeSecs": 0
        });
        let meta: InstanceMetadata = serde_json::from_value(json).unwrap();
        assert_eq!(meta.schema_version, 1, "缺失的 schema_version 应默认为 1");
    }
}
