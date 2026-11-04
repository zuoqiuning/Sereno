//! 下载调度：有界队列 + Semaphore + 分片下载。

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex as StdMutex};

use futures_util::StreamExt;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::download::counter::SessionCounter;
use crate::download::sink::ProgressSink;
use crate::download::task::{Checksum, DownloadTask};
use crate::models::download::{DownloadProgress, DownloadSnapshot, DownloadState};
use crate::{LauncherError, Result};

/// 只有大文件（客户端 jar、超大库）才分片：资产多为小文件，
/// 32 个任务 × 8 分片会产生 256 条连接，反而被镜像限流/超时。
const CHUNK_THRESHOLD: u64 = 16 * 1024 * 1024;
const MAX_CHUNKS: u32 = 4;
const PROGRESS_INTERVAL_MS: u64 = 250;

/// 全局限速之外的并发闸门。
///
/// 之前每个下载命令都会 `DownloadQueue::new(..., max_concurrent)` 各建一份信号量，
/// 同时下 3 个 Mod 就是 3 倍并发，`最大下载线程数` 形同虚设。
/// 现在所有队列共享同一份许可，设置变更时按新值整体替换。
static GLOBAL_SLOTS: LazyLock<StdMutex<(usize, Arc<Semaphore>)>> =
    LazyLock::new(|| StdMutex::new((0, Arc::new(Semaphore::new(1)))));
/// 同路径互斥也要跨队列共享，否则两个队列仍可能同时写同一个文件。
type PathLockMap = HashMap<PathBuf, Arc<Mutex<()>>>;
static GLOBAL_PATH_LOCKS: LazyLock<Arc<Mutex<PathLockMap>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

// ---- 单 host 熔断（对齐 PCL 的镜像熔断思路）----
//
// 场景：BMCLAPI 抖动时，几十个任务各自把「连不上 → 重试 → 超时」走一遍，
// 每个任务都白等一轮超时，整段下载就像冻结。熔断后同 host 的源被直接
// 跳过，流量立刻切到其它源（官方 / 备用镜像）。

/// 连续失败多少次后熔断。
const HOST_BREAK_THRESHOLD: u32 = 3;
/// 熔断持续秒数；冷却期结束自动半开（允许一次尝试探测恢复）。
const HOST_BREAK_OPEN_SECS: u64 = 60;

#[derive(Default)]
struct HostBreaker {
    consecutive_fails: u32,
    open_until: Option<std::time::Instant>,
}

/// 跨会话共享：一个会话里踩过的坑，下一个会话不必再踩。
static HOST_BREAKERS: LazyLock<StdMutex<HashMap<String, HostBreaker>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

fn host_of(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()?
        .host_str()
        .map(str::to_string)
}

fn host_is_open(host: &str) -> bool {
    let Ok(map) = HOST_BREAKERS.lock() else {
        return false;
    };
    let Some(breaker) = map.get(host) else {
        return false;
    };
    breaker
        .open_until
        .is_some_and(|until| std::time::Instant::now() < until)
}

fn host_mark_success(host: &str) {
    if let Ok(mut map) = HOST_BREAKERS.lock() {
        map.remove(host);
    }
}

fn host_mark_failure(host: &str) {
    let Ok(mut map) = HOST_BREAKERS.lock() else {
        return;
    };
    let breaker = map.entry(host.to_string()).or_default();
    breaker.consecutive_fails += 1;
    if breaker.consecutive_fails >= HOST_BREAK_THRESHOLD {
        breaker.open_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(HOST_BREAK_OPEN_SECS));
        breaker.consecutive_fails = 0;
        tracing::warn!(host, "连续失败达到阈值，熔断 {} 秒", HOST_BREAK_OPEN_SECS);
    }
}

fn shared_semaphore(max_concurrent: usize) -> Arc<Semaphore> {
    let max = max_concurrent.max(1);
    let mut guard = GLOBAL_SLOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.0 != max {
        *guard = (max, Arc::new(Semaphore::new(max)));
    }
    Arc::clone(&guard.1)
}

/// 分片下载的并发池。
///
/// 主信号量管的是**文件级**并发，而一个大文件会内部再开 [`MAX_CHUNKS`] 条连接。
/// 如果分片不额外限流，「设置 8 并发」在分片场景下会变成 32 条连接，
/// 镜像站会直接 503——这也是设置项与实际行为不符的老问题。
/// 这里按同一个 `max_concurrent` 单独限一层：分片总数不超过用户设定的并发。
static CHUNK_SLOTS: LazyLock<StdMutex<(usize, Arc<Semaphore>)>> =
    LazyLock::new(|| StdMutex::new((0, Arc::new(Semaphore::new(1)))));

fn shared_chunk_semaphore(max_concurrent: usize) -> Arc<Semaphore> {
    // 分片许可放宽为并发的 2 倍：大文件（客户端 jar / 库）的分片是
    // 短促的高并发请求，与「任务数」共用一层许可时，8 个大文件会把
    // 许可占满，导致并发小文件（assets）全部排队——这正是
    // 「下载原版慢」的元凶之一。放宽后总量仍在带宽层面受用户限速约束。
    let max = (max_concurrent.max(1)).saturating_mul(2);
    let mut guard = CHUNK_SLOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.0 != max {
        *guard = (max, Arc::new(Semaphore::new(max)));
    }
    Arc::clone(&guard.1)
}

/// 坏文件统一用「原文件名 + `.part`」。
///
/// 旧实现用 `with_extension("part")`，同目录下 `a.jar` 与 `a.zip` 都会变成
/// `a.part`，两个任务并行时会互相覆盖。
fn part_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    destination.with_file_name(format!("{name}.part"))
}

pub struct DownloadQueue {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    path_locks: Arc<Mutex<HashMap<std::path::PathBuf, Arc<Mutex<()>>>>>,
    /// 分片任务专用并发池（见 [`CHUNK_SLOTS`]）。
    chunk_slots: Arc<Semaphore>,
}

/// 一次任务执行的共享上下文。
///
/// 把取消、进度出口、会话计数器收进一个结构，避免每个内部函数都拖着
/// 七八个参数（也避免 clippy 的 `too_many_arguments`）。
struct TaskCtx {
    cancel: CancellationToken,
    sink: Option<Arc<dyn ProgressSink>>,
    session_id: String,
    counter: Arc<SessionCounter>,
}

impl TaskCtx {
    fn publish_task(
        &self,
        task: &DownloadTask,
        downloaded: u64,
        total: u64,
        speed_bps: u64,
        state: DownloadState,
    ) {
        if let Some(sink) = &self.sink {
            sink.publish(DownloadProgress::Task {
                snapshot: DownloadSnapshot {
                    session_id: self.session_id.clone(),
                    task_id: task.id.clone(),
                    file_name: task.display_name.clone(),
                    downloaded,
                    total,
                    speed_bps,
                    state,
                },
            });
        }
    }
}

impl DownloadQueue {
    pub fn new(client: reqwest::Client, max_concurrent: usize) -> Self {
        Self {
            client,
            // 二道保险：配置层已经夹过 1–32，这里再挡一次 0，
            // 0 个许可会让 acquire() 永远不返回。
            semaphore: shared_semaphore(max_concurrent),
            path_locks: Arc::clone(&GLOBAL_PATH_LOCKS),
            chunk_slots: shared_chunk_semaphore(max_concurrent),
        }
    }

    pub async fn execute_task(
        &self,
        task: DownloadTask,
        cancel: CancellationToken,
        sink: Option<Arc<dyn ProgressSink>>,
        session_id: &str,
        counter: Arc<SessionCounter>,
    ) -> Result<()> {
        // 1. 已存在且校验通过 → 跳过
        if task.destination.exists()
            && crate::download::verify::verify_file(&task.destination, &task.checksum).is_ok()
        {
            let size = std::fs::metadata(&task.destination)
                .map(|m| m.len())
                .unwrap_or(0);
            counter.add_downloaded(size);
            let ctx = TaskCtx {
                cancel,
                sink,
                session_id: session_id.to_string(),
                counter,
            };
            ctx.publish_task(&task, size, size, 0, DownloadState::Done);
            return Ok(());
        }

        if let Some(parent) = task.destination.parent() {
            crate::utils::paths::ensure_dir(parent)?;
        }

        // 2. 文件级互斥，防止多个任务同时写同一个路径
        let path_lock = {
            let mut locks = self.path_locks.lock().await;
            locks
                .entry(task.destination.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = path_lock.lock().await;

        let ctx = TaskCtx {
            cancel,
            sink,
            session_id: session_id.to_string(),
            counter,
        };

        let mut attempt = 0u32;
        let mut last_err = None;

        while attempt <= task.retry_policy.max_retries {
            if ctx.cancel.is_cancelled() {
                return Err(LauncherError::Cancelled);
            }

            // 并发许可只覆盖「一次尝试」。
            //
            // 如果把许可提到 while 外面（或与 sleep 同作用域），正在退避的任务
            // 会一直占着槽位：8 个任务同时踩到超时，整个下载器就会停止派发，
            // 用户看到的是「进度条不动」，而实际带宽是空的。
            let outcome = {
                let _permit = self
                    .semaphore
                    .acquire()
                    .await
                    .map_err(|_| LauncherError::Cancelled)?;
                self.download_any_source(&task, &ctx).await
            };

            match outcome {
                Ok(()) => return Ok(()),
                Err(LauncherError::Cancelled) => return Err(LauncherError::Cancelled),
                // 校验失败说明这组源给出的内容都不对，重试同一组源没有意义
                Err(e @ LauncherError::ChecksumMismatch { .. }) => return Err(e),
                Err(e) => {
                    last_err = Some(e);
                    attempt += 1;
                    if attempt <= task.retry_policy.max_retries {
                        let factor = 1u32.checked_shl(attempt - 1).unwrap_or(u32::MAX / 2);
                        let delay = task
                            .retry_policy
                            .base_delay
                            .saturating_mul(factor)
                            .min(task.retry_policy.max_delay);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_err.unwrap_or(LauncherError::Internal {
            message: "下载失败且无错误信息".into(),
        }))
    }

    async fn download_any_source(&self, task: &DownloadTask, ctx: &TaskCtx) -> Result<()> {
        let temp_path = part_path(&task.destination);

        let mut last_error: Option<String> = None;

        let mut skipped_by_breaker = 0usize;

        for source in &task.sources {
            if ctx.cancel.is_cancelled() {
                return Err(LauncherError::Cancelled);
            }

            // host 熔断中 → 直接跳过该源，流量切到下一个源
            let host = host_of(&source.url);
            if let Some(host) = &host
                && host_is_open(host)
            {
                skipped_by_breaker += 1;
                tracing::debug!(url = %source.url, host, "源处于熔断冷却期，跳过");
                continue;
            }

            let result = if should_chunk(task) {
                self.download_chunked(&source.url, task, ctx, &temp_path)
                    .await
            } else {
                self.download_streaming(&source.url, task, ctx, &temp_path)
                    .await
            };

            match result {
                Ok(()) => {
                    if let Some(host) = &host {
                        host_mark_success(host);
                    }
                    return Ok(());
                }
                Err(LauncherError::Cancelled) => return Err(LauncherError::Cancelled),
                Err(e) => {
                    if let Some(host) = &host {
                        host_mark_failure(host);
                    }
                    tracing::debug!(url = %source.url, error = %e, "源下载失败，尝试下一个源");
                    last_error = Some(e.to_string());
                }
            }
        }

        // 所有源都被熔断跳过：给出专门错误让外层退避重试——
        // 冷却期（60s）通常比退避间隔短，重试时大概率已半开
        if last_error.is_none()
            && skipped_by_breaker > 0
            && skipped_by_breaker == task.sources.len()
        {
            return Err(LauncherError::AllSourcesUnavailable {
                message: format!(
                    "{} 的全部源都在熔断冷却中（{} 秒后自动恢复）",
                    task.display_name, HOST_BREAK_OPEN_SECS
                ),
            });
        }
        Err(LauncherError::AllSourcesUnavailable {
            message: match last_error {
                Some(error) => format!("无法下载 {}（最后一次错误: {error}）", task.display_name),
                None => format!("无法下载 {}", task.display_name),
            },
        })
    }

    async fn download_streaming(
        &self,
        url: &str,
        task: &DownloadTask,
        ctx: &TaskCtx,
        temp_path: &Path,
    ) -> Result<()> {
        // 续传时先把已有片段喂进哈希器，否则完成后校验必然失败、
        // 导致每次续传都从头重下（等于没有续传）。
        let mut existing_size = 0u64;
        let mut hasher = Sha1::new();

        if temp_path.exists()
            && let Ok(meta) = std::fs::metadata(temp_path)
            && meta.len() > 0
        {
            existing_size = meta.len();
            if hash_file_into(temp_path, &mut hasher).is_err() {
                let _ = std::fs::remove_file(temp_path);
                existing_size = 0;
                hasher = Sha1::new();
            }
        }

        let mut req = self.client.get(url);
        if existing_size > 0 {
            req = req.header("Range", format!("bytes={existing_size}-"));
        }

        let response = req
            .send()
            .await
            .map_err(|e| LauncherError::DownloadFailed {
                message: format!("请求失败: {e}"),
                url: url.to_string(),
            })?;

        let is_resume = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if !response.status().is_success() && !is_resume {
            if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                let _ = tokio::fs::remove_file(temp_path).await;
            }
            return Err(LauncherError::DownloadFailed {
                message: format!("HTTP {}", response.status()),
                url: url.to_string(),
            });
        }

        // 服务器忽略了 Range、返回完整文件：丢弃旧片段，重新从头算哈希
        if !is_resume && existing_size > 0 {
            let _ = tokio::fs::remove_file(temp_path).await;
            existing_size = 0;
            hasher = Sha1::new();
        }

        let total_size = response.content_length().unwrap_or(0) + existing_size;
        let file = if is_resume {
            tokio::fs::OpenOptions::new()
                .write(true)
                .append(true)
                .open(temp_path)
                .await
        } else {
            tokio::fs::File::create(temp_path).await
        }
        .map_err(|e| LauncherError::Io {
            message: format!("打开临时文件失败: {e}"),
        })?;

        let mut writer = tokio::io::BufWriter::with_capacity(128 * 1024, file);
        let mut stream = response.bytes_stream();
        let mut downloaded = existing_size;
        let mut last_report = tokio::time::Instant::now();
        let mut last_bytes = downloaded;

        while let Some(chunk) = stream.next().await {
            if ctx.cancel.is_cancelled() {
                return Err(LauncherError::Cancelled);
            }
            let bytes = chunk.map_err(|e| LauncherError::DownloadFailed {
                message: format!("流读取失败: {e}"),
                url: url.to_string(),
            })?;

            // 全局限速：所有下载共享同一份令牌，设置里的「限速」才是真的总带宽上限
            crate::download::ratelimit::throttle(bytes.len()).await;

            hasher.update(&bytes);
            writer
                .write_all(&bytes)
                .await
                .map_err(|e| LauncherError::Io {
                    message: format!("写入失败: {e}"),
                })?;

            downloaded += bytes.len() as u64;
            ctx.counter.add_downloaded(bytes.len() as u64);

            let elapsed = last_report.elapsed();
            if elapsed >= tokio::time::Duration::from_millis(PROGRESS_INTERVAL_MS) {
                let delta = downloaded.saturating_sub(last_bytes);
                let speed_bps = (delta as f64 / elapsed.as_secs_f64()) as u64;
                last_bytes = downloaded;
                last_report = tokio::time::Instant::now();
                ctx.publish_task(
                    task,
                    downloaded,
                    total_size,
                    speed_bps,
                    DownloadState::Running,
                );
            }
        }

        writer.flush().await.map_err(|e| LauncherError::Io {
            message: format!("flush 失败: {e}"),
        })?;
        drop(writer);

        let hash_hex = format!("{:x}", hasher.finalize());
        let pass = match &task.checksum {
            Checksum::Sha1(expected) => hash_hex.eq_ignore_ascii_case(expected),
            Checksum::Size(size) => downloaded == *size,
            Checksum::None => true,
        };

        if !pass {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(LauncherError::ChecksumMismatch {
                path: task.destination.display().to_string(),
                expected: match &task.checksum {
                    Checksum::Sha1(s) => s.clone(),
                    Checksum::Size(n) => format!("{n} bytes"),
                    Checksum::None => String::new(),
                },
                actual: hash_hex,
            });
        }

        rename_with_retry(temp_path, &task.destination).await?;
        ctx.publish_task(task, total_size, total_size, 0, DownloadState::Done);
        Ok(())
    }

    async fn download_chunked(
        &self,
        url: &str,
        task: &DownloadTask,
        ctx: &TaskCtx,
        temp_path: &Path,
    ) -> Result<()> {
        let total_size = task.size_hint.ok_or(LauncherError::Internal {
            message: "分片下载需要 size_hint".into(),
        })?;
        if total_size < CHUNK_THRESHOLD {
            return self.download_streaming(url, task, ctx, temp_path).await;
        }

        // 分片下载不做续传：每次都从整文件重新拼，避免错位数据。
        let _ = tokio::fs::remove_file(temp_path).await;
        {
            let f = tokio::fs::File::create(temp_path)
                .await
                .map_err(|e| LauncherError::Io {
                    message: format!("创建分片文件失败: {e}"),
                })?;
            f.set_len(total_size).await.map_err(|e| LauncherError::Io {
                message: format!("预分配失败: {e}"),
            })?;
        }

        let chunk_size = (total_size / MAX_CHUNKS as u64).max(1);
        let task_bytes = Arc::new(AtomicU64::new(0));
        let chunk_cancel = CancellationToken::new();
        let mut tasks = Vec::new();

        for i in 0..MAX_CHUNKS {
            let start = i as u64 * chunk_size;
            let end = if i == MAX_CHUNKS - 1 {
                total_size - 1
            } else {
                (i as u64 + 1) * chunk_size - 1
            };
            if start > end {
                break;
            }

            tasks.push(tokio::spawn(download_chunk(ChunkJob {
                client: self.client.clone(),
                url: url.to_string(),
                start,
                end,
                path: temp_path.to_path_buf(),
                cancel: chunk_cancel.clone(),
                task_bytes: Arc::clone(&task_bytes),
                session_counter: Arc::clone(&ctx.counter),
                slots: Arc::clone(&self.chunk_slots),
            })));
        }

        let stop_pusher = CancellationToken::new();
        if let Some(ctx_sink) = ctx.sink.as_ref() {
            let ctx_sink = Arc::clone(ctx_sink);
            let session_id = ctx.session_id.clone();
            let task_id = task.id.clone();
            let file_name = task.display_name.clone();
            let bytes = Arc::clone(&task_bytes);
            let stop = stop_pusher.clone();
            let task_bytes_total = total_size;

            tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(tokio::time::Duration::from_millis(PROGRESS_INTERVAL_MS));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut last_bytes = 0u64;
                let mut last_time = tokio::time::Instant::now();

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            let downloaded = bytes.load(Ordering::Relaxed);
                            let elapsed = last_time.elapsed();
                            let speed = if elapsed.as_millis() > 0 {
                                (downloaded.saturating_sub(last_bytes) as f64
                                    / elapsed.as_secs_f64()) as u64
                            } else {
                                0
                            };
                            last_bytes = downloaded;
                            last_time = tokio::time::Instant::now();

                            ctx_sink.publish(DownloadProgress::Task {
                                snapshot: DownloadSnapshot {
                                    session_id: session_id.clone(),
                                    task_id: task_id.clone(),
                                    file_name: file_name.clone(),
                                    downloaded,
                                    total: task_bytes_total,
                                    speed_bps: speed,
                                    state: DownloadState::Running,
                                },
                            });
                        }
                        _ = stop.cancelled() => break,
                    }
                }
            });
        }

        let mut error = None;
        let mut total_written = 0u64;

        for handle in tasks {
            match handle.await {
                Ok(Ok(n)) => total_written += n,
                Ok(Err(e)) => {
                    if error.is_none() {
                        error = Some(e);
                    }
                    chunk_cancel.cancel();
                }
                Err(e) => {
                    if error.is_none() {
                        error = Some(LauncherError::Internal {
                            message: format!("分片 panic: {e}"),
                        });
                    }
                    chunk_cancel.cancel();
                }
            }
        }
        stop_pusher.cancel();

        if let Some(e) = error {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(e);
        }

        if total_written != total_size {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(LauncherError::DownloadFailed {
                message: "分片总字节不符".into(),
                url: url.to_string(),
            });
        }

        if let Err(e) = crate::download::verify::verify_file(temp_path, &task.checksum) {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(e);
        }

        rename_with_retry(temp_path, &task.destination).await?;
        ctx.publish_task(task, total_size, total_size, 0, DownloadState::Done);
        Ok(())
    }
}

fn should_chunk(task: &DownloadTask) -> bool {
    matches!(task.checksum, Checksum::Sha1(_))
        && task
            .size_hint
            .map(|s| s >= CHUNK_THRESHOLD)
            .unwrap_or(false)
}

/// 把已有 `.part` 的内容喂进哈希器，用于续传后的完整校验。
fn hash_file_into(path: &Path, hasher: &mut Sha1) -> std::io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(())
}

/// 单个分片任务的参数。
struct ChunkJob {
    client: reqwest::Client,
    url: String,
    start: u64,
    end: u64,
    path: std::path::PathBuf,
    cancel: CancellationToken,
    task_bytes: Arc<AtomicU64>,
    session_counter: Arc<SessionCounter>,
    /// 分片并发池：分片连接数并入全局并发预算。
    slots: Arc<Semaphore>,
}

async fn download_chunk(job: ChunkJob) -> Result<u64> {
    let ChunkJob {
        client,
        url,
        start,
        end,
        path,
        cancel,
        task_bytes,
        session_counter,
        slots,
    } = job;

    // 许可覆盖「一次分片请求」的完整生命周期（连接 + 读流），
    // 保证同一时刻的真实连接数不超过用户设定的并发。
    let _permit = slots
        .acquire()
        .await
        .map_err(|_| LauncherError::Cancelled)?;

    let response = client
        .get(&url)
        .header("Range", format!("bytes={start}-{end}"))
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("分片请求失败: {e}"),
            url: url.clone(),
        })?;

    // 必须拿到 206；200 表示服务器忽略了 Range，写进去的数据会错位
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(LauncherError::DownloadFailed {
            message: format!("分片请求未返回 206（实际 {}）", response.status()),
            url,
        });
    }

    let expected = end - start + 1;
    if let Some(len) = response.content_length()
        && len != expected
    {
        return Err(LauncherError::DownloadFailed {
            message: format!("分片长度不符：期望 {expected}，实际 {len}"),
            url,
        });
    }

    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|e| LauncherError::Io {
            message: format!("打开分片文件失败: {e}"),
        })?;

    let mut stream = response.bytes_stream();
    let mut offset = start;
    let mut total = 0u64;

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(LauncherError::Cancelled);
        }
        let bytes = chunk.map_err(|e| LauncherError::DownloadFailed {
            message: format!("分片流读取失败: {e}"),
            url: url.clone(),
        })?;

        let remaining = (end + 1).saturating_sub(offset);
        if remaining == 0 {
            break;
        }
        let take = bytes.len().min(remaining as usize);

        // 分片下载同样受全局限速约束
        crate::download::ratelimit::throttle(take).await;

        write_all_at(&file, &bytes[..take], offset).map_err(|e| LauncherError::Io {
            message: format!("分片写入失败: {e}"),
        })?;

        offset += take as u64;
        total += take as u64;
        task_bytes.fetch_add(take as u64, Ordering::Relaxed);
        session_counter.add_downloaded(take as u64);
    }

    Ok(total)
}

/// 极度宽容的重命名：给杀毒软件留足时间
/// rename 是否值得重试：文件不存在 / 权限拒绝 / 共享冲突 / 锁冲突。
fn is_retryable_rename_error(code: Option<i32>) -> bool {
    matches!(code, Some(2) | Some(5) | Some(32) | Some(33))
}

async fn rename_with_retry(from: &Path, to: &Path) -> Result<()> {
    let mut retries = 0;
    loop {
        match tokio::fs::rename(from, to).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                // Windows 实测的可重试错误码：
                //   2  ERROR_FILE_NOT_FOUND（瞬时可见性）
                //   5  ERROR_ACCESS_DENIED（权限/占用抖动）
                //   32 ERROR_SHARING_VIOLATION（杀毒/索引器正打开文件——最常见）
                //   33 ERROR_LOCK_VIOLATION（字节范围锁，同上）
                // 杀毒软件扫描下载文件时锁住的是 32/33 而不是 5，
                // 漏掉它们会让大文件下载在最后一步「重命名失败」。
                if is_retryable_rename_error(e.raw_os_error()) && retries < 30 {
                    retries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
                return Err(LauncherError::Io {
                    message: format!("重命名文件失败: {e}"),
                });
            }
        }
    }
}

#[cfg(windows)]
fn write_all_at(file: &std::fs::File, mut buf: &[u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = file.seek_write(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "write_at 0",
            ));
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(unix)]
fn write_all_at(file: &std::fs::File, mut buf: &[u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    while !buf.is_empty() {
        let n = file.write_at(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "write_at 0",
            ));
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn host_breaker_trips_after_threshold_and_resets_on_success() {
        // 全局状态跨测试共享：用独立 host 名 + 先清场
        HOST_BREAKERS.lock().unwrap().clear();
        let host = "breaker-threshold.test";

        for _ in 0..(HOST_BREAK_THRESHOLD - 1) {
            host_mark_failure(host);
        }
        assert!(!host_is_open(host), "未达阈值不应熔断");

        host_mark_failure(host); // 第 3 次
        assert!(host_is_open(host), "达到阈值应熔断");

        // 成功即清场（冷却期内的半开被成功覆盖）
        host_mark_success(host);
        assert!(!host_is_open(host));
    }

    #[test]
    fn host_of_parses_and_tolerates_garbage() {
        assert!(host_of("not a url").is_none());
        assert_eq!(
            host_of("https://bmclapi2.bangbang93.com/forge/minecraft/1.21.1").as_deref(),
            Some("bmclapi2.bangbang93.com")
        );
        assert_eq!(
            host_of("http://127.0.0.1:8080/a").as_deref(),
            Some("127.0.0.1")
        );
    }

    #[test]
    fn retryable_rename_error_codes_cover_antivirus_locks() {
        assert!(is_retryable_rename_error(Some(2)));
        assert!(is_retryable_rename_error(Some(5)));
        assert!(is_retryable_rename_error(Some(32))); // 共享冲突（杀毒最常见）
        assert!(is_retryable_rename_error(Some(33))); // 锁冲突
        assert!(!is_retryable_rename_error(Some(3))); // 路径不存在，重试无意义
        assert!(!is_retryable_rename_error(None)); // 非 OS 错误
    }
    use super::*;

    #[test]
    fn part_file_keeps_full_name() {
        let destination = Path::new("mods/fabric-api.jar");
        assert_eq!(
            part_path(destination),
            PathBuf::from("mods/fabric-api.jar.part")
        );
    }

    #[test]
    fn part_files_do_not_collide_across_extensions() {
        let jar = part_path(Path::new("mods/a.jar"));
        let zip = part_path(Path::new("mods/a.zip"));
        assert_ne!(jar, zip);
    }
}
