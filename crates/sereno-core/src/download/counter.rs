//! 会话计数器：聚合整个会话的进度，跨阶段共享。

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::download::task::DownloadTask;
use crate::models::download::DownloadSessionSummary;

pub fn count_totals(tasks: &[DownloadTask]) -> (u32, u64) {
    let bytes: u64 = tasks.iter().filter_map(|t| t.size_hint).sum();
    (tasks.len() as u32, bytes)
}

pub struct SessionCounter {
    session_id: String,
    purpose: String,
    stage: AtomicU32,
    total_files: AtomicU32,
    total_bytes: AtomicU64,
    done_files: AtomicU32,
    failed_files: AtomicU32,
    downloaded_bytes: AtomicU64,
}

impl SessionCounter {
    pub fn new(session_id: impl Into<String>, purpose: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            purpose: purpose.into(),
            stage: AtomicU32::new(1),
            total_files: AtomicU32::new(0),
            total_bytes: AtomicU64::new(0),
            done_files: AtomicU32::new(0),
            failed_files: AtomicU32::new(0),
            downloaded_bytes: AtomicU64::new(0),
        }
    }

    pub fn set_stage(&self, stage: u32) {
        self.stage.store(stage, Ordering::Relaxed);
    }

    pub fn add_total(&self, files: u32, bytes: u64) {
        self.total_files.fetch_add(files, Ordering::Relaxed);
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_downloaded(&self, bytes: u64) {
        self.downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// 预播种一个**在下载队列之外已完成**的阶段。
    ///
    /// 版本 JSON 解析、清单落盘等工作发生在计数器创建之后、下载任务
    /// 开始之前——不播种的话进度条会在这几秒停在 0%，观感是
    /// 「卡住了」。播种后这些工作计入总量**和**已完成，进度从真实
    /// 的起点开始走（对齐 PCL「元数据阶段也计入总进度」的观感）。
    pub fn seed_completed(&self, files: u32, bytes: u64) {
        self.total_files.fetch_add(files, Ordering::Relaxed);
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.done_files.fetch_add(files, Ordering::Relaxed);
        self.downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn mark_done(&self) {
        self.done_files.fetch_add(1, Ordering::Relaxed);
    }

    pub fn mark_failed(&self) {
        self.failed_files.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> DownloadSessionSummary {
        DownloadSessionSummary {
            session_id: self.session_id.clone(),
            purpose: self.purpose.clone(),
            stage: self.stage.load(Ordering::Relaxed),
            instance_id: None,
            total_files: self.total_files.load(Ordering::Relaxed),
            done_files: self.done_files.load(Ordering::Relaxed),
            failed_files: self.failed_files.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            // 强制限制不超过总数，解决并发导致的进度溢出
            downloaded_bytes: std::cmp::min(
                self.downloaded_bytes.load(Ordering::Relaxed),
                self.total_bytes.load(Ordering::Relaxed),
            ),
        }
    }
}
