//! 下载会话：管理一组任务的执行、整体进度汇总与取消。

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::download::counter::{SessionCounter, count_totals};
use crate::download::queue::DownloadQueue;
use crate::download::sink::ProgressSink;
use crate::download::task::DownloadTask;
use crate::models::download::{DownloadProgress, DownloadSnapshot, DownloadState};
use crate::{LauncherError, Result};

pub struct DownloadSession {
    pub session_id: String,
    pub queue: Arc<DownloadQueue>,
    pub cancel_token: CancellationToken,
}

impl DownloadSession {
    pub fn new(session_id: String, queue: Arc<DownloadQueue>) -> Self {
        Self {
            session_id,
            queue,
            cancel_token: CancellationToken::new(),
        }
    }

    /// 单阶段便捷入口：自建 counter + 汇总推送 + 执行。
    pub async fn run(&self, tasks: Vec<DownloadTask>, sink: Arc<dyn ProgressSink>) -> Result<()> {
        let (files, bytes) = count_totals(&tasks);
        let counter = Arc::new(SessionCounter::new(self.session_id.clone(), "download"));
        counter.add_total(files, bytes);

        let handle = spawn_summary_pusher(
            Arc::clone(&counter),
            Arc::clone(&sink),
            self.cancel_token.clone(),
        );

        let res = self
            .run_with_counter(tasks, sink.clone(), Arc::clone(&counter))
            .await;

        handle.abort();
        sink.publish(DownloadProgress::Session {
            summary: counter.snapshot(),
        });
        res
    }

    /// 多阶段执行：外部传入 counter，由调用方负责起汇总推送。
    pub async fn run_with_counter(
        &self,
        mut tasks: Vec<DownloadTask>,
        sink: Arc<dyn ProgressSink>,
        counter: Arc<SessionCounter>,
    ) -> Result<()> {
        // 高优先级任务先 spawn，先拿到信号量许可：
        // 客户端 jar / 基础库不会被几百个资源对象挤到队尾。
        tasks.sort_by_key(|task| std::cmp::Reverse(task.priority));

        let mut join_set = JoinSet::new();

        for task in tasks {
            let queue = Arc::clone(&self.queue);
            let cancel = self.cancel_token.clone();
            let session_id = self.session_id.clone();
            let sink = Arc::clone(&sink);
            let counter = Arc::clone(&counter);

            let task_id = task.id.clone();
            let file_name = task.display_name.clone();

            join_set.spawn(async move {
                let result = queue
                    .execute_task(
                        task,
                        cancel,
                        Some(sink.clone()),
                        &session_id,
                        Arc::clone(&counter),
                    )
                    .await;

                match &result {
                    Ok(_) => counter.mark_done(),
                    Err(LauncherError::Cancelled) => {}
                    Err(_) => {
                        counter.mark_failed();
                        sink.publish(DownloadProgress::Task {
                            snapshot: DownloadSnapshot {
                                session_id: session_id.clone(),
                                task_id,
                                file_name,
                                downloaded: 0,
                                total: 0,
                                speed_bps: 0,
                                state: DownloadState::Failed,
                            },
                        });
                    }
                }
                result
            });
        }

        // 失败的任务**不能让会话整体表现为成功**：
        // 否则安装命令会把“缺了一堆库”的实例标记为 ready。
        let mut first_error: Option<LauncherError> = None;

        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(Ok(())) => {}
                Ok(Err(LauncherError::Cancelled)) => {
                    self.cancel_token.cancel();
                    return Err(LauncherError::Cancelled);
                }
                Ok(Err(e)) => {
                    tracing::warn!("任务失败: {}", e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                Err(e) => {
                    tracing::error!("下载任务 panic: {:?}", e);
                    if first_error.is_none() {
                        first_error = Some(LauncherError::Internal {
                            message: format!("下载任务异常退出: {e}"),
                        });
                    }
                }
            }
        }

        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// 起一个后台任务，每 200ms 向 sink 推一次 Session 汇总。
pub fn spawn_summary_pusher(
    counter: Arc<SessionCounter>,
    sink: Arc<dyn ProgressSink>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(200));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // 立即推一次，让前端马上看到初始状态
        sink.publish(DownloadProgress::Session {
            summary: counter.snapshot(),
        });

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    sink.publish(DownloadProgress::Session {
                        summary: counter.snapshot(),
                    });
                }
                _ = cancel.cancelled() => break,
            }
        }
    })
}
