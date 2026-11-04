//! 启动游戏子进程：转发控制台输出、写 `latest.log`、捕获秒崩。

use std::path::Path;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::{LauncherError, Result};

use super::args::LaunchArgs;

pub async fn launch_game<F>(
    java_exe: &str,
    args: LaunchArgs,
    instance_dir: &Path,
    on_exit: F,
) -> Result<()>
where
    F: FnOnce(Option<i32>) + Send + 'static,
{
    let log_path = instance_dir
        .join("minecraft")
        .join("logs")
        .join("latest.log");
    crate::utils::paths::ensure_dir(log_path.parent().unwrap())?;

    // 游戏日志同时写 `latest.log`：崩溃后这是用户唯一能贴出来的现场
    let log_file = tokio::fs::File::create(&log_path)
        .await
        .map_err(|e| LauncherError::Io {
            message: format!("创建游戏日志失败 {}: {e}", log_path.display()),
        })?;
    let log = Arc::new(Mutex::new(tokio::io::BufWriter::with_capacity(
        64 * 1024,
        log_file,
    )));

    let mut cmd = Command::new(java_exe);
    cmd.args(&args.jvm_args)
        .args(&args.game_args)
        .current_dir(instance_dir.join("minecraft"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = cmd.spawn().map_err(|e| LauncherError::Internal {
        message: format!("拉起 Java 失败: {e}"),
    })?;

    // 写跨进程运行锁：启动器重启后还能认出“这个实例正在跑”，
    // 避免重复启动同一个实例导致存档并发写入。
    let pid = child.id().unwrap_or(0);
    if let Err(error) = crate::instance::lock::mark(instance_dir, pid) {
        tracing::warn!(%error, "写入实例运行锁失败，跨进程启动保护不可用");
    }

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    spawn_log_reader(stdout, Arc::clone(&log), false);
    spawn_log_reader(stderr, Arc::clone(&log), true);

    // 等待 5 秒，看是否瞬间崩溃。正常进入游戏加载流程就返回，
    // 后续退出由后台任务记录退出码。
    let timeout_res = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;

    match timeout_res {
        Ok(Ok(status)) => {
            append_log(&log, &format!("进程已退出，退出码: {:?}", status.code())).await;
            crate::instance::lock::clear(instance_dir);
            Err(LauncherError::Internal {
                message: format!(
                    "游戏启动即崩溃（退出码: {:?}）\n请查看实例 minecraft/logs/latest.log",
                    status.code()
                ),
            })
        }
        Ok(Err(e)) => {
            crate::instance::lock::clear(instance_dir);
            Err(LauncherError::Internal {
                message: format!("等待进程时出错: {e}"),
            })
        }
        Err(_) => {
            let log = Arc::clone(&log);
            let dir = instance_dir.to_path_buf();
            tokio::spawn(async move {
                match child.wait().await {
                    Ok(status) => {
                        append_log(&log, &format!("进程已退出，退出码: {:?}", status.code())).await;
                        on_exit(status.code());
                    }
                    Err(e) => {
                        append_log(&log, &format!("等待进程失败: {e}")).await;
                        on_exit(None);
                    }
                }
                crate::instance::lock::clear(&dir);
            });
            Ok(())
        }
    }
}

fn spawn_log_reader(
    stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    log: Arc<Mutex<tokio::io::BufWriter<tokio::fs::File>>>,
    is_error: bool,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stream).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            // 游戏输出**不进**启动器的 info 日志：它已经有 `latest.log` 这一份归宿，
            // 再复制一遍会让启动器日志被几万行游戏日志淹没。
            // 这里只保留 trace 级别，需要时用 `RUST_LOG=trace` 打开。
            if is_error {
                // 现代 MC 会把 log4j 状态、JVM 警告都写 stderr，量级远小于 stdout
                tracing::debug!(target: "sereno_core::mc", "{}", line);
            } else {
                tracing::trace!(target: "sereno_core::mc", "{}", line);
            }
            append_log(&log, &line).await;
        }
    });
}

async fn append_log(log: &Arc<Mutex<tokio::io::BufWriter<tokio::fs::File>>>, line: &str) {
    let mut writer = log.lock().await;
    let _ = writer.write_all(line.as_bytes()).await;
    let _ = writer.write_all(b"\n").await;
    let _ = writer.flush().await;
}
