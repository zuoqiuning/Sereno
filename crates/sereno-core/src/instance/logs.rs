//! 读取实例的运行日志与崩溃报告，供前端内置查看器展示。

use std::path::Path;

use crate::Result;
use crate::models::crash::{InstanceLogs, LogFile};
use crate::models::settings::AppSettings;
use crate::utils::paths::instance_dir;

/// 单个文件最多返回的字符数，避免把几十 MB 日志塞进 IPC。
const MAX_LOG_CHARS: usize = 200_000;
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
    let text = std::fs::read_to_string(path).ok()?;
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
