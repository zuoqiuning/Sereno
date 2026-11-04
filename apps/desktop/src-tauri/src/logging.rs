//! 日志管线：`tracing` 是唯一门面，本地文件 sink 是唯一落盘通道。
//!
//! 落地要点（详见 `docs/日志规范与诊断包.md`）：
//!
//! - **文件输出 JSON Lines，终端输出人类可读**，两者共用同一套字段；
//! - **按天滚动 + 单文件 50 MB 分段**，两者都按 UTC 时间计算，**保留 7 天**；
//!   文件名带进程 ID（`sereno.<日期>.<pid>.<序号>.log`），多进程不会互相覆盖。
//! - **脱敏在写出层统一实现**：不在每个调用点手动打码，一定漏；
//! - dev 默认 `debug`，release 默认 `info`，可用 `RUST_LOG` 覆盖。
//!
//! 为什么不直接用 `tracing-appender`：它只支持按时间滚动，无法在单日内按大小切分。
//! 一次整合包安装可能写出几百 MB 日志，所以这里用一个小的自管 sink，
//! 同时拿到「按天 + 按 50 MB 分段 + 自动清理过期文件」三个行为。

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use sereno_core::utils::redact::redact_line;

/// 单个日志文件的上限；超过就切到 `sereno.<日期>.<序号>.log`。
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// 保留最近 7 天（含今天）。
const KEEP_DAYS: i64 = 7;

/// 文件 sink 的全局引用，供退出时 `flush()` 使用。
static FILE_SINK: OnceLock<Arc<FileSink>> = OnceLock::new();

/// 自管的滚动文件 sink。
struct FileSink {
    dir: PathBuf,
    state: Mutex<SinkState>,
}

struct SinkState {
    /// 当前文件对应的 UTC 天（自 1970-01-01 起的天数）。
    day: i64,
    /// 当天的分段序号：0 是 `sereno.<日期>.log`，之后依次递增。
    part: u32,
    file: Option<File>,
    /// 当前文件已有的字节数，避免每次写入都查 metadata。
    size: u64,
}

impl FileSink {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            state: Mutex::new(SinkState {
                day: 0,
                part: 0,
                file: None,
                size: 0,
            }),
        }
    }

    fn path_for(&self, day: i64, part: u32) -> PathBuf {
        let date = date_of(day);
        // 文件名带 PID：dev 模式或异常情况下可能同时跑两个进程，
        // 不同进程各自写自己的文件，切分逻辑不会互相踩。
        let pid = std::process::id();
        if part == 0 {
            self.dir.join(format!("sereno.{date}.{pid}.log"))
        } else {
            self.dir.join(format!("sereno.{date}.{pid}.{part}.log"))
        }
    }

    /// 确保当前文件可写；跨天或超过大小上限时换一个文件。
    fn rotate_if_needed(&self, state: &mut SinkState, incoming: u64) {
        let day = utc_day();
        let overflow = state.size.saturating_add(incoming) > MAX_FILE_BYTES;
        if state.file.is_some() && state.day == day && !overflow {
            return;
        }

        let day_changed = state.day != day;
        state.part = if state.day == day && state.file.is_some() {
            state.part + 1
        } else {
            0
        };
        state.day = day;
        let path = self.path_for(day, state.part);
        state.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        state.size = state
            .file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map(|meta| meta.len())
            .unwrap_or(0);

        // 只在跨天时清理：按大小分段不应每次都扫目录。
        if day_changed {
            cleanup(&self.dir);
        }
    }

    fn write_bytes(&self, buf: &[u8]) -> io::Result<()> {
        let Ok(mut state) = self.state.lock() else {
            return Ok(());
        };
        self.rotate_if_needed(&mut state, buf.len() as u64);

        // 打开失败时丢弃这一行，但要让日志库继续工作。
        if let Some(file) = state.file.as_mut() {
            file.write_all(buf)?;
            state.size = state.size.saturating_add(buf.len() as u64);
        }
        Ok(())
    }

    fn flush_all(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if let Some(file) = state.file.as_mut() {
            let _ = file.flush();
        }
    }
}

/// 删除超过保留期的旧日志。只在启动与跨天时执行，成本可忽略。
fn cleanup(dir: &Path) {
    let threshold = date_of(utc_day() - (KEEP_DAYS - 1));
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        // 只处理本 sink 的产物，避免误删同目录的其它文件。
        if !name.starts_with("sereno.") || !name.ends_with(".log") {
            continue;
        }
        let date = name
            .trim_start_matches("sereno.")
            .split('.')
            .next()
            .unwrap_or("");
        if is_date(date) && date < threshold.as_str() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn is_date(text: &str) -> bool {
    text.len() == 10
        && text.as_bytes()[4] == b'-'
        && text.as_bytes()[7] == b'-'
        && text
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// 当前 UTC 天（自纪元起的天数）。
fn utc_day() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64 / 86_400)
        .unwrap_or(0)
}

/// 把「自纪元起的天数」格式化成 `YYYY-MM-DD`。
fn date_of(day: i64) -> String {
    // Howard Hinnant 的 civil_from_days：无需依赖日期库，逐位反推年月日。
    let z = day + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day_of_month = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day_of_month:02}")
}

/// 实际落盘的 sink。
///
/// 日志目录不可写时退化为 `io::Sink`，让应用仍能启动。
struct Sink {
    inner: Option<Arc<FileSink>>,
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(sink) = self.inner.as_ref() {
            sink.write_bytes(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(sink) = self.inner.as_ref() {
            sink.flush_all();
        }
        Ok(())
    }
}

/// 写出层脱敏写入器。
///
/// 包在文件 sink 外面，按**行**处理：
/// 先在内存里拼到换行符，再对整行做脱敏后落盘。
struct RedactingWriter {
    sink: Sink,
    buf: Vec<u8>,
}

impl Write for RedactingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);

        // 逐行切分：找到换行符就整行脱敏后写出
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            // 去掉换行符本身，落盘时再补
            line.pop();

            let text = String::from_utf8_lossy(&line);
            let mut redacted = redact_line(&text);

            // 每条记录都带 app_version，便于用户只发一段日志时判断版本。
            // JSON 行的首尾一定是 `{}`，在 `{` 之后插入是安全的。
            if redacted.starts_with('{') && !redacted.contains("\"app_version\"") {
                redacted.insert_str(
                    1,
                    &format!("\"app_version\":\"{}\",", sereno_core::APP_VERSION),
                );
            }

            self.sink.write_all(redacted.as_bytes())?;
            self.sink.write_all(b"\n")?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.drain_remainder()?;
        self.sink.flush()
    }
}

impl RedactingWriter {
    /// 把缓冲区里剩下的半行写出去。
    ///
    /// `MakeWriter` 每写一条事件就新建一个 writer，若事件内容没以换行结尾，
    /// 残留的字节会随 writer 一起被 drop 掉——所以 `Drop` 里必须补写。
    fn drain_remainder(&mut self) -> io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&self.buf);
        let redacted = redact_line(&text);
        self.sink.write_all(redacted.as_bytes())?;
        self.sink.write_all(b"\n")?;
        self.buf.clear();
        Ok(())
    }
}

impl Drop for RedactingWriter {
    fn drop(&mut self) {
        // 写失败只能忽略：析构里无法向上抛错
        let _ = self.drain_remainder();
    }
}

/// `MakeWriter`：每次写入拿到一个独立的脱敏包装器。
#[derive(Clone)]
struct RedactingMakeWriter {
    /// `None` 表示日志目录不可写，退化为空 sink。
    inner: Option<Arc<FileSink>>,
}

impl<'a> MakeWriter<'a> for RedactingMakeWriter {
    type Writer = RedactingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            sink: Sink {
                inner: self.inner.as_ref().map(Arc::clone),
            },
            buf: Vec::new(),
        }
    }
}

/// 初始化全局日志管线。
///
/// **只应被调用一次**（第二次调用会 panic，因为全局 subscriber 只能设置一次）。
pub fn init() {
    // 默认级别：debug 构建用 debug，release 用 info。
    // 不在 release 里默认 debug：下载器每个任务数条日志，一次补全就是几万行。
    let default_level = if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // dev 打开全部 debug；release 严格 info。
        //
        // **不要**无条件给 `sereno_core` 开 debug：游戏进程的每一行 stdout 都会
        // 走一次日志（见 `launch/process.rs`），一次游玩就是几万行，
        // 50 MB 的分段很快被打满，诊断包里全是噪音。
        // 需要看游戏输出时设 `RUST_LOG=trace`，或直接读实例的 `minecraft/logs/latest.log`。
        if cfg!(debug_assertions) {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        }
    });

    // ---- 文件 sink：JSON Lines，按天 + 50 MB 分段 ----
    let file_writer = build_file_writer();

    // ---- 终端 sink：人类可读 ----
    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_ansi(true)
        .with_writer(std::io::stdout);

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_ansi(false)
        .with_writer(file_writer);

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    tracing::debug!(
        default_level = default_level,
        log_dir = %sereno_core::utils::paths::logs_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        "日志管线已就绪"
    );
}

/// 构建文件日志的写入器。
///
/// 日志目录不可写时**不能让整个应用起不来**：退化为空 sink，只保留终端输出。
fn build_file_writer() -> RedactingMakeWriter {
    let sink = (|| -> Option<Arc<FileSink>> {
        let dir = sereno_core::utils::paths::logs_dir().ok()?;
        sereno_core::utils::paths::ensure_dir(&dir).ok()?;

        let sink = Arc::new(FileSink::new(dir));
        // 启动时先清理过期日志：进程可能没机会走到第一次切文件。
        cleanup(&sink.dir);
        Some(sink)
    })();

    match sink {
        Some(sink) => {
            let _ = FILE_SINK.set(Arc::clone(&sink));
            RedactingMakeWriter { inner: Some(sink) }
        }
        None => {
            eprintln!("[logging] 日志目录不可写，已退回仅终端输出");
            RedactingMakeWriter { inner: None }
        }
    }
}

/// 进程退出时把缓冲区里的半行冲掉。
///
/// 由 `lib.rs` 在 Tauri 的 `RunEvent::Exit` 里调用。
///
/// - 正常退出：控制台与日志文件都会被 flush，**不丢日志**；
/// - 异常退出（panic / 任务管理器结束进程）：**可能丢失最后半行**——
///   这是所有「写文件 + 缓冲」的方案都无法避免的，不做过度承诺。
pub fn flush() {
    let _ = std::io::stdout().flush();

    if let Some(sink) = FILE_SINK.get() {
        sink.flush_all();
    }
}
