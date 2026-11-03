use serde::{Deserialize, Serialize};

/// 一份日志文件（崩溃报告或 latest.log）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub name: String,
    pub content: String,
}

/// 一条崩溃诊断结论。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashFinding {
    /// 结论标题（一句话说清「哪里坏了」）。
    pub title: String,
    /// 解释：为什么会这样。
    pub detail: String,
    /// 建议：用户下一步该做什么。
    pub suggestion: String,
    /// `error` / `warning`。
    pub severity: String,
    /// 判断的确定程度：`high` / `medium`。
    /// 低置信度的结论不提供一键修复，只作为参考建议。
    #[serde(default)]
    pub confidence: String,
    /// 可直接执行的修复入口：`open-settings` / `open-mods` / `repair`。
    #[serde(default)]
    pub action_kind: Option<String>,
    /// 修复按钮文案。
    #[serde(default)]
    pub action_label: Option<String>,
}

/// 日志诊断结果。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashAnalysis {
    /// 命中的诊断结论（按严重程度与出现顺序排列）。
    pub findings: Vec<CrashFinding>,
    /// 从日志里摘出的关键报错行，方便用户一眼看到原始证据。
    pub excerpt: Vec<String>,
}

/// 实例的日志快照：最新日志 + 最近的崩溃报告 + 诊断结论。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceLogs {
    pub latest_log: Option<String>,
    pub crashes: Vec<LogFile>,
    /// 自动诊断：对最新崩溃报告（没有则对 latest.log）做规则匹配。
    pub analysis: CrashAnalysis,
}
