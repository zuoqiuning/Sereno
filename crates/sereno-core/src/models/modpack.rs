//! 整合包相关的数据契约。

use serde::{Deserialize, Serialize};

/// 整合包概览：安装前展示给用户的关键信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackInfo {
    /// 整合包名称。
    pub name: String,
    /// 整合包要求的 Minecraft 版本。
    pub minecraft_version: String,
    /// 加载器类型：`fabric` / `neoforge` / `forge` / `quilt`，空字符串表示原版。
    pub loader: String,
    /// 加载器版本号。
    pub loader_version: String,
    /// 客户端需要额外下载的文件数（不含游戏本体）。
    pub file_count: u32,
}
