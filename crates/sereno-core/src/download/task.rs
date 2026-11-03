//! 下载任务的定义。

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 校验方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Checksum {
    Sha1(String),
    Size(u64),
    None,
}

/// 单个候选下载源。
#[derive(Debug, Clone)]
pub struct SourceUrl {
    pub url: String,
    pub is_mirror: bool,
}

/// 重试策略。
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        }
    }
}

/// 一个待下载任务。
#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub id: String,
    pub display_name: String,
    pub sources: Vec<SourceUrl>,
    pub destination: PathBuf,
    pub checksum: Checksum,
    /// 期望的文件大小，用于会话总字节数统计。
    pub size_hint: Option<u64>,
    /// 调度优先级，数字越大越先拿到并发许可。
    ///
    /// 客户端 jar / 基础库必须先于几百个资源对象完成，
    /// 否则用户会看到「进度条在动但游戏装不完」。
    pub priority: u8,
    pub retry_policy: RetryPolicy,
}

impl DownloadTask {
    pub fn primary_source(&self) -> Option<&SourceUrl> {
        self.sources.first()
    }
}
