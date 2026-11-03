//! 下载相关的数据契约。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSnapshot {
    pub session_id: String,
    pub task_id: String,
    pub file_name: String,
    #[specta(type = specta_typescript::Number)]
    pub downloaded: u64,
    #[specta(type = specta_typescript::Number)]
    pub total: u64,
    #[specta(type = specta_typescript::Number)]
    pub speed_bps: u64,
    pub state: DownloadState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSessionSummary {
    pub session_id: String,
    pub purpose: String,
    /// 当前所处阶段：1 = 基础库, 2 = 资源文件, 3 = 加载器
    pub stage: u32,
    #[serde(default)]
    pub instance_id: Option<String>,
    pub total_files: u32,
    pub done_files: u32,
    pub failed_files: u32,
    #[specta(type = specta_typescript::Number)]
    pub total_bytes: u64,
    #[specta(type = specta_typescript::Number)]
    pub downloaded_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DownloadProgress {
    Task { snapshot: DownloadSnapshot },
    Session { summary: DownloadSessionSummary },
}
