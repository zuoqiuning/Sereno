use serde::{Deserialize, Serialize};

use crate::models::crash::CrashAnalysis;

/// 实例健康检查报告：启动前预检 + Mod 依赖图 + 最近崩溃诊断。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub instance_id: String,
    /// 预检项（Java / 内存 / 磁盘 / 文件完整性 / Mod / 崩溃）。
    pub checks: Vec<HealthCheck>,
    /// Mod 依赖图检查结果。
    pub mods: ModHealth,
    /// 最近一次崩溃（或最新日志）的自动诊断。
    pub crash: CrashAnalysis,
    /// 检查时间（Unix 秒）。
    pub checked_at: String,
}

/// 一条预检结论。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    /// 稳定的检查 ID：`instance` / `java` / `memory` / `disk` / `mods` / `crash` / `running`。
    pub id: String,
    pub title: String,
    /// `ok` / `warn` / `error`。
    pub status: String,
    pub detail: String,
    /// 可直接执行的一键修复入口；前端按 `kind` 分发。
    pub action: Option<HealthAction>,
}

/// 一键修复入口。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HealthAction {
    /// `repair` / `open-settings` / `open-mods` / `open-logs`。
    pub kind: String,
    pub label: String,
}

/// Mod 依赖图检查结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModHealth {
    /// 已启用的 Mod 数量。
    pub enabled: u32,
    /// 被禁用（`.disabled`）的 Mod 数量。
    pub disabled: u32,
    /// 同一个 mod id 被多个文件提供。
    pub duplicates: Vec<ModDuplicate>,
    /// 缺失的必需前置。
    pub missing: Vec<ModDependencyIssue>,
    /// 已安装但不满足版本区间的前置。
    pub mismatched: Vec<ModDependencyIssue>,
}

/// 重复安装的 Mod。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModDuplicate {
    pub mod_id: String,
    pub name: String,
    /// 提供同一个 mod id 的所有文件名。
    pub files: Vec<String>,
}

/// 一条依赖问题。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModDependencyIssue {
    /// 出问题的前置 mod id。
    pub mod_id: String,
    /// 是哪些 Mod 在要求它。
    pub required_by: Vec<String>,
    /// 要求的版本区间；缺失时为空（表示只要求存在）。
    pub requirement: String,
    /// 已安装的版本；缺失前置时为 `None`。
    pub installed_version: Option<String>,
}
