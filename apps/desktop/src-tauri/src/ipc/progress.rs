//! `ProgressSink` → `Channel` 的适配。

use tauri::ipc::Channel;

use sereno_core::download::sink::ProgressSink;
use sereno_core::models::download::DownloadProgress;

/// 把 core 的进度事件转发到 Tauri 通道。
pub struct ChannelSink(pub Channel<DownloadProgress>);

impl ProgressSink for ChannelSink {
    fn publish(&self, progress: DownloadProgress) {
        let _ = self.0.send(progress);
    }
}
