//! Java 相关的数据契约。

use serde::{Deserialize, Serialize};

/// 检测到的 Java 运行时。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaCandidate {
    /// Java 可执行文件的绝对路径。
    pub path: String,
    /// 主版本号，如 8, 17, 21。解析失败为 0。
    pub major_version: u32,
    /// 完整版本字符串，如 `1.8.0_402` 或 `21.0.4`。
    pub version_string: String,
    /// 架构，如 `x64` 或 `x86`。
    pub arch: String,
    /// 是否由用户手动指定（未来的字段）。
    pub is_custom: bool,
}
