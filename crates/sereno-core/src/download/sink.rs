//! 进度出口：core 唯一的进度上报接口。

use crate::models::download::DownloadProgress;

pub trait ProgressSink: Send + Sync + 'static {
    fn publish(&self, progress: DownloadProgress);
}

/// 什么都不做的 sink，用于测试或不需要进度的场景。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl ProgressSink for NoopSink {
    fn publish(&self, _progress: DownloadProgress) {}
}
