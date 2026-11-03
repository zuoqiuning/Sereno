//! 下载调度：有界队列 + Semaphore + 分片下载。

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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

pub struct DownloadQueue {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    path_locks: Arc<Mutex<HashMap<std::path::PathBuf, Arc<Mutex<()>>>>>,
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
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            path_locks: Arc::new(Mutex::new(HashMap::new())),
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

            let _permit = self
                .semaphore
                .acquire()
                .await
                .map_err(|_| LauncherError::Cancelled)?;

            match self.download_any_source(&task, &ctx).await {
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
        let temp_path = task.destination.with_extension("part");

        let mut last_error: Option<String> = None;

        for source in &task.sources {
            if ctx.cancel.is_cancelled() {
                return Err(LauncherError::Cancelled);
            }

            let result = if should_chunk(task) {
                self.download_chunked(&source.url, task, ctx, &temp_path)
                    .await
            } else {
                self.download_streaming(&source.url, task, ctx, &temp_path)
                    .await
            };

            match result {
                Ok(()) => return Ok(()),
                Err(LauncherError::Cancelled) => return Err(LauncherError::Cancelled),
                Err(e) => {
                    tracing::debug!(url = %source.url, error = %e, "源下载失败，尝试下一个源");
                    last_error = Some(e.to_string());
                }
            }
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
    } = job;

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
async fn rename_with_retry(from: &Path, to: &Path) -> Result<()> {
    let mut retries = 0;
    loop {
        match tokio::fs::rename(from, to).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                let is_os_error = e.raw_os_error() == Some(2) || e.raw_os_error() == Some(5);
                if is_os_error && retries < 30 {
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
