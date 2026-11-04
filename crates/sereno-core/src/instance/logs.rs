//! 读取实例的运行日志与崩溃报告，供前端内置查看器展示。

use std::path::Path;

use crate::Result;
use crate::models::crash::{InstanceLogs, LogFile};
use crate::models::settings::AppSettings;
use crate::utils::paths::instance_dir;

/// 单个文件最多返回的字符数，避免把几十 MB 日志塞进 IPC。
const MAX_LOG_CHARS: usize = 200_000;
/// 单次最多从磁盘读取的字节数：只读尾部，避免几十 MB 的日志全量进内存。
const MAX_READ_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CRASH_REPORTS: usize = 10;

pub fn read(settings: &AppSettings, instance_id: &str) -> Result<InstanceLogs> {
    let minecraft = instance_dir(settings, instance_id)?.join("minecraft");

    let latest_log = read_tail(&minecraft.join("logs").join("latest.log"));

    let mut crashes = Vec::new();
    let crash_dir = minecraft.join("crash-reports");
    if let Ok(entries) = std::fs::read_dir(&crash_dir) {
        let mut files: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("txt"))
            .collect();
        files.sort_by_key(|path| {
            std::cmp::Reverse(
                path.metadata()
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs()),
            )
        });

        for path in files.into_iter().take(MAX_CRASH_REPORTS) {
            let Some(content) = read_tail(&path) else {
                continue;
            };
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("crash-report")
                .to_string();
            crashes.push(LogFile { name, content });
        }
    }

    // 诊断优先看最新崩溃报告；没有报告时退回 latest.log
    let analysis_text = crashes
        .first()
        .map(|crash| crash.content.as_str())
        .or(latest_log.as_deref())
        .unwrap_or("");
    let analysis = crate::crash::analyzer::analyze(analysis_text);

    Ok(InstanceLogs {
        latest_log,
        crashes,
        analysis,
    })
}

fn read_tail(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let len = std::fs::metadata(path).ok()?.len();
    let mut file = std::fs::File::open(path).ok()?;

    // 只读尾部：`latest.log` 可能几十 MB，全量读进内存再截断不划算。
    let truncated_head = len > MAX_READ_BYTES;
    let mut buf = Vec::new();
    if truncated_head {
        file.seek(SeekFrom::Start(len - MAX_READ_BYTES)).ok()?;
    }
    file.read_to_end(&mut buf).ok()?;

    // 严格 UTF-8 优先；失败时退化为有损解码。
    //
    // **为什么要退化**：旧日志（启动器还没传 `-Dstdout.encoding` 时）
    // 和其他启动器生成的日志可能是 GBK。严格读取会直接返回 Err，
    // 于是「日志查看器一片空白」「崩溃分析 0 条结论」——
    // 有损解码至少保住英文行与堆栈，能救回大部分诊断价值。
    let text = match String::from_utf8(buf) {
        Ok(text) => text,
        Err(error) => {
            let lossy = String::from_utf8_lossy(error.as_bytes()).to_string();
            format!(
                "…（此日志不是 UTF-8 编码，部分字符可能显示为乱码；\
                 重新启动一次游戏即可生成 UTF-8 日志）\n{lossy}"
            )
        }
    };

    // 从文件中间开始读时，第一行大概率是半截的，丢掉它
    let text = if truncated_head {
        match text.find('\n') {
            Some(index) => text[index + 1..].to_string(),
            None => text,
        }
    } else {
        text
    };

    if text.chars().count() <= MAX_LOG_CHARS {
        return Some(text);
    }

    let cut = text
        .char_indices()
        .rev()
        .nth(MAX_LOG_CHARS)
        .map(|(index, _)| index)
        .unwrap_or(0);
    Some(format!(
        "…（日志过长，只显示最后 {MAX_LOG_CHARS} 个字符）\n{}",
        &text[cut..]
    ))
}
