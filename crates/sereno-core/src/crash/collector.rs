//! 诊断包收集：按《日志规范与诊断包》的包结构收集、脱敏并打包为 zip。
//!
//! 设计原则：
//! - **只读**：不修改实例与设置；
//! - **脱敏**：所有文本都经 `utils::redact::redact_line` 处理后再写包；
//! - **有上限**：`latest.log` 保留尾部 20MB，其它单文件 10MB，总包 50MB；
//!   超出部分写入 `manifest.json` 的 `truncated` 列表或 `skipped` 记录。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::models::java::JavaCandidate;
use crate::models::settings::AppSettings;
use crate::utils::redact::{redact_line, redact_proxy};
use crate::{APP_VERSION, LauncherError, Result};

const MAX_TEXT_BYTES: u64 = 10 * 1024 * 1024;
const MAX_LOG_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;

pub struct DiagnosticReport {
    pub path: PathBuf,
    pub entries: Vec<String>,
    pub skipped: Vec<String>,
    pub truncated: Vec<String>,
}

/// 生成诊断包，返回产物路径与收录清单。
pub fn export(
    settings: &AppSettings,
    instance_dir: Option<&Path>,
    state_dir: &Path,
    output_dir: &Path,
    java: Option<&JavaCandidate>,
) -> Result<DiagnosticReport> {
    crate::utils::paths::ensure_dir(output_dir)?;

    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let zip_path = output_dir.join(format!("sereno-diag-{}.zip", compact_stamp(seconds)));

    let file = File::create(&zip_path).map_err(|e| LauncherError::Io {
        message: format!("创建诊断包失败 {}: {e}", zip_path.display()),
    })?;

    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut report = DiagnosticReport {
        path: zip_path,
        entries: Vec::new(),
        skipped: Vec::new(),
        truncated: Vec::new(),
    };
    let mut total_bytes = 0u64;

    // 1. 启动器日志：最多最近 2 天
    for path in latest_files(&state_dir.join("logs"), 2, |name| name.ends_with(".log")) {
        let name = file_name(&path, "launcher.log");
        read_and_write_text(
            &mut writer,
            &options,
            &format!("launcher/{name}"),
            &path,
            MAX_TEXT_BYTES,
            &mut report,
            &mut total_bytes,
        )?;
    }

    // 2. 游戏侧文件
    if let Some(inst_dir) = instance_dir {
        let minecraft = inst_dir.join("minecraft");

        // latest.log：超过 20MB 保留尾部
        read_and_write_text(
            &mut writer,
            &options,
            "game/latest.log",
            &minecraft.join("logs").join("latest.log"),
            MAX_LOG_BYTES,
            &mut report,
            &mut total_bytes,
        )?;

        // crash-reports：最多 3 份
        for path in latest_files(&minecraft.join("crash-reports"), 3, |name| {
            name.ends_with(".txt")
        }) {
            let name = file_name(&path, "crash.txt");
            read_and_write_text(
                &mut writer,
                &options,
                &format!("game/crash-reports/{name}"),
                &path,
                MAX_TEXT_BYTES,
                &mut report,
                &mut total_bytes,
            )?;
        }

        // JVM 崩溃日志：最多 1 份
        for path in latest_files(&minecraft, 1, |name| {
            name.starts_with("hs_err_pid") && name.ends_with(".log")
        }) {
            let name = file_name(&path, "hs_err.log");
            read_and_write_text(
                &mut writer,
                &options,
                &format!("game/{name}"),
                &path,
                MAX_TEXT_BYTES,
                &mut report,
                &mut total_bytes,
            )?;
        }

        // 实例元数据 / 运行锁 / Mod 清单
        read_and_write_text(
            &mut writer,
            &options,
            "instance/instance.json",
            &inst_dir.join(".launcher").join("instance.json"),
            MAX_TEXT_BYTES,
            &mut report,
            &mut total_bytes,
        )?;
        read_and_write_text(
            &mut writer,
            &options,
            "instance/lock.json",
            &inst_dir.join(".launcher").join("lock.json"),
            MAX_TEXT_BYTES,
            &mut report,
            &mut total_bytes,
        )?;

        let mods_text = build_mods_list(&minecraft.join("mods"), &mut report);
        write_bytes(
            &mut writer,
            &options,
            "instance/mods.list.txt",
            mods_text.as_bytes(),
            &mut report,
            &mut total_bytes,
        )?;
    }

    // 3. 系统与 Java
    write_bytes(
        &mut writer,
        &options,
        "system.txt",
        build_system_info().as_bytes(),
        &mut report,
        &mut total_bytes,
    )?;
    write_bytes(
        &mut writer,
        &options,
        "instance/java.txt",
        build_java_info(java).as_bytes(),
        &mut report,
        &mut total_bytes,
    )?;
    write_bytes(
        &mut writer,
        &options,
        "settings.redacted.json",
        build_settings_redacted(settings).as_bytes(),
        &mut report,
        &mut total_bytes,
    )?;

    // 4. 说明与 manifest（manifest 最后写，才能带上 truncated 列表）
    write_bytes(
        &mut writer,
        &options,
        "README.txt",
        README.as_bytes(),
        &mut report,
        &mut total_bytes,
    )?;

    let manifest = build_manifest(settings, instance_dir, java, seconds, &report);
    write_bytes(
        &mut writer,
        &options,
        "manifest.json",
        manifest.as_bytes(),
        &mut report,
        &mut total_bytes,
    )?;

    // 5. 跳过的条目单独记录，避免 manifest 膨胀
    if !report.skipped.is_empty() {
        let note = format!(
            "以下文件因体积或编码原因未收录：\n{}\n",
            report.skipped.join("\n")
        );
        write_bytes(
            &mut writer,
            &options,
            "skipped.txt",
            note.as_bytes(),
            &mut report,
            &mut total_bytes,
        )?;
    }

    writer.finish().map_err(|e| LauncherError::Io {
        message: format!("写入诊断包失败: {e}"),
    })?;

    Ok(report)
}

const README: &str = "\
Sereno 诊断包
============

包含：
- manifest.json：本次导出的概要（版本、实例、Java、脱敏与截断记录）
- launcher/：启动器日志（最近 2 天）
- game/：latest.log、crash-reports、hs_err_pid*.log
- instance/：instance.json、lock.json、mods.list.txt、java.txt
- settings.redacted.json：已脱敏的设置摘要
- system.txt：操作系统与 Java 概况

不包含：
- accessToken / refresh_token
- Mod 的 jar 本体、assets、世界存档、整个 .minecraft 目录

提交方式：
把整个 zip 发给开发者即可。如果介意用户名或路径，请先自行检查并替换。
";

fn build_manifest(
    settings: &AppSettings,
    instance_dir: Option<&Path>,
    java: Option<&JavaCandidate>,
    seconds: u64,
    report: &DiagnosticReport,
) -> String {
    let metadata = instance_dir.and_then(|dir| crate::storage::instance::read_metadata(dir).ok());
    let mod_count = instance_dir
        .map(|dir| count_mods(&dir.join("minecraft").join("mods")))
        .unwrap_or(0);

    let instance = match &metadata {
        Some(metadata) => json!({
            "id": metadata.id,
            "mc_version": metadata.game_version,
            "loader": {
                "type": metadata.loader.r#type,
                "version": metadata.loader.version,
            },
            "mod_count": mod_count,
        }),
        None => json!(null),
    };

    let java_json = match java {
        Some(java) => json!({
            "path": redact_line(&java.path),
            "major": java.major_version,
            "arch": java.arch,
            "vendor": null,
        }),
        None => json!(null),
    };

    let manifest = json!({
        "schema_version": 1,
        "generated_at": format!("{}Z", format_unix(seconds)),
        "launcher_version": APP_VERSION,
        "os": {
            "name": std::env::consts::OS,
            "version": std::env::var("OS").unwrap_or_else(|_| "unknown".into()),
            "arch": std::env::consts::ARCH,
        },
        "memory_mb": settings.java.default_memory_mb,
        "trigger": "manual",
        "instance": instance,
        "java": java_json,
        "game_exit_code": null,
        "redactions": ["accessToken", "refresh_token", "userprofile", "email", "jwt"],
        "truncated": report.truncated,
    });

    serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string())
}

fn build_system_info() -> String {
    format!(
        "启动器版本: {APP_VERSION}\nOS: {}\nARCH: {}\n处理器: {}\n逻辑核心: {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "未知".into()),
        std::env::var("NUMBER_OF_PROCESSORS").unwrap_or_else(|_| "未知".into()),
    )
}

fn build_java_info(java: Option<&JavaCandidate>) -> String {
    match java {
        Some(java) => format!(
            "路径: {}\n版本: {}\n主版本: {}\n架构: {}\n自定义: {}\n",
            redact_line(&java.path),
            java.version_string,
            java.major_version,
            java.arch,
            java.is_custom,
        ),
        None => "未检测到可用的 Java 环境。\n".to_string(),
    }
}

fn build_settings_redacted(settings: &AppSettings) -> String {
    // 逐字段挑出来，而不是把 `settings.http` 整份塞进去：
    // 代理地址的常见形态是 `http://user:password@host:port`，
    // 而这份 zip 的用途就是发给开发者排查问题。
    let http = json!({
        "connect_timeout_secs": settings.http.connect_timeout_secs,
        "request_timeout_secs": settings.http.request_timeout_secs,
        "read_timeout_secs": settings.http.read_timeout_secs,
        "max_redirects": settings.http.max_redirects,
        "proxy": settings.http.proxy.as_deref().map(redact_proxy),
    });

    let summary = json!({
        "download": settings.download,
        "java": {
            "custom_path": settings.java.custom_path.as_deref().map(redact_line),
            "default_memory_mb": settings.java.default_memory_mb,
        },
        "logging": settings.logging,
        "http": http,
    });
    serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "{}".to_string())
}

fn build_mods_list(mods_dir: &Path, report: &mut DiagnosticReport) -> String {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return "（未找到 mods 目录）\n".to_string();
    };

    let mut files: Vec<(String, u64, String)> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            let path = entry.path();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let sha1 = sha1_of(&path).unwrap_or_else(|| "unavailable".into());
            (entry.file_name().to_string_lossy().to_string(), size, sha1)
        })
        .collect();

    if files.len() != count_mods(mods_dir) {
        report
            .skipped
            .push("instance/mods.list.txt 中部分文件不可读".into());
    }

    files.sort_by_key(|(name, _, _)| name.to_lowercase());

    let mut text = format!("共 {} 个文件\n", files.len());
    for (name, size, sha1) in files {
        text.push_str(&format!("{name}\t{size}\t{sha1}\n"));
    }
    text
}

fn count_mods(mods_dir: &Path) -> usize {
    std::fs::read_dir(mods_dir)
        .map(|entries| entries.flatten().filter(|e| e.path().is_file()).count())
        .unwrap_or(0)
}

fn sha1_of(path: &Path) -> Option<String> {
    use sha1::{Digest, Sha1};

    let mut file = File::open(path).ok()?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

fn read_and_write_text(
    writer: &mut ZipWriter<File>,
    options: &SimpleFileOptions,
    entry_name: &str,
    path: &Path,
    max_bytes: u64,
    report: &mut DiagnosticReport,
    total: &mut u64,
) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let raw = if size > max_bytes {
        report.truncated.push(entry_name.to_string());
        read_tail(path, max_bytes)?
    } else {
        match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => {
                // 非 UTF-8 日志（少见）用 lossy 读取，避免整包失败
                let bytes = std::fs::read(path).map_err(|e| LauncherError::Io {
                    message: format!("读取 {} 失败: {e}", path.display()),
                })?;
                String::from_utf8_lossy(&bytes).to_string()
            }
        }
    };

    let redacted = raw.lines().map(redact_line).collect::<Vec<_>>().join("\n");
    write_bytes(
        writer,
        options,
        entry_name,
        redacted.as_bytes(),
        report,
        total,
    )
}

/// 读取文件尾部 `max_bytes`，并尽量从行首开始。
fn read_tail(path: &Path, max_bytes: u64) -> Result<String> {
    let mut file = File::open(path).map_err(|e| LauncherError::Io {
        message: format!("读取 {} 失败: {e}", path.display()),
    })?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = size.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| LauncherError::Io {
            message: format!("定位 {} 失败: {e}", path.display()),
        })?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| LauncherError::Io {
            message: format!("读取 {} 尾部失败: {e}", path.display()),
        })?;

    let mut text = String::from_utf8_lossy(&buffer).to_string();
    if start > 0
        && let Some(pos) = text.find('\n')
    {
        text.drain(..=pos);
    }
    Ok(text)
}

fn write_bytes(
    writer: &mut ZipWriter<File>,
    options: &SimpleFileOptions,
    entry_name: &str,
    bytes: &[u8],
    report: &mut DiagnosticReport,
    total: &mut u64,
) -> Result<()> {
    if *total + bytes.len() as u64 > MAX_TOTAL_BYTES {
        report
            .skipped
            .push(format!("{entry_name}（总包体积超过上限）"));
        return Ok(());
    }

    writer
        .start_file(entry_name, *options)
        .map_err(|e| LauncherError::Io {
            message: format!("写入诊断包条目 {entry_name} 失败: {e}"),
        })?;
    writer.write_all(bytes).map_err(|e| LauncherError::Io {
        message: format!("写入诊断包条目 {entry_name} 失败: {e}"),
    })?;

    *total += bytes.len() as u64;
    report.entries.push(entry_name.to_string());
    Ok(())
}

/// 目录下按修改时间倒序的最新 `limit` 个文件。
fn latest_files(dir: &Path, limit: usize, predicate: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter(|entry| predicate(&entry.file_name().to_string_lossy()))
        .map(|entry| {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (modified, entry.path())
        })
        .collect();

    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    files
        .into_iter()
        .take(limit)
        .map(|(_, path)| path)
        .collect()
}

fn file_name(path: &Path, fallback: &str) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn compact_stamp(seconds: u64) -> String {
    let days = seconds / 86_400;
    let rem = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn format_unix(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant 的 `civil_from_days` 算法（1970-01-01 起）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn civil_dates_are_correct() {
        assert_eq!(format_unix(0), "1970-01-01 00:00:00");
        assert_eq!(format_unix(86_400), "1970-01-02 00:00:00");
        assert_eq!(format_unix(1_700_000_000), "2023-11-14 22:13:20");
    }

    fn temp_root() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        std::env::temp_dir().join(format!("sereno-diagnostics-{stamp}"))
    }

    #[test]
    fn exports_redacted_zip_with_expected_entries() {
        let root = temp_root();
        let state_dir = root.join("state");
        let instances_root = root.join("instances");
        std::fs::create_dir_all(state_dir.join("logs")).unwrap();

        let settings = AppSettings {
            instances_root: Some(instances_root.display().to_string()),
            ..Default::default()
        };
        let metadata =
            crate::instance::lifecycle::create_instance(&settings, "1.20.1", "诊断测试").unwrap();
        let inst_dir = instances_root.join(&metadata.id);

        std::fs::write(
            state_dir.join("logs").join("sereno.2026-11-01.log"),
            "level=INFO accessToken=secret-token user alice@example.com\n",
        )
        .unwrap();

        std::fs::create_dir_all(inst_dir.join("minecraft/logs")).unwrap();
        std::fs::create_dir_all(inst_dir.join("minecraft/crash-reports")).unwrap();
        std::fs::create_dir_all(inst_dir.join("minecraft/mods")).unwrap();
        std::fs::write(
            inst_dir.join("minecraft/logs/latest.log"),
            "email: bob@example.com\nAuthorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.signature\n",
        )
        .unwrap();
        std::fs::write(
            inst_dir.join("minecraft/crash-reports/crash-1.txt"),
            "java.lang.NullPointerException\n",
        )
        .unwrap();
        std::fs::write(inst_dir.join("minecraft/mods/example.jar"), b"jar").unwrap();

        let output_dir = root.join("diagnostics");
        let report = export(&settings, Some(&inst_dir), &state_dir, &output_dir, None).unwrap();

        assert!(report.path.exists());
        assert!(
            report
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("sereno-diag-")
        );
        assert!(report.entries.iter().any(|e| e == "instance/mods.list.txt"));
        assert!(report.entries.iter().any(|e| e == "manifest.json"));
        assert!(
            report
                .entries
                .iter()
                .any(|e| e.starts_with("game/crash-reports/"))
        );

        let file = File::open(&report.path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        let mut log = String::new();
        archive
            .by_name("game/latest.log")
            .unwrap()
            .read_to_string(&mut log)
            .unwrap();
        assert!(log.contains("b***@example.com"), "邮箱必须打码: {log}");
        assert!(
            !log.contains("eyJhbGciOiJIUzI1NiJ9"),
            "JWT 必须被遮蔽: {log}"
        );

        let mut launcher_log = String::new();
        archive
            .by_name("launcher/sereno.2026-11-01.log")
            .unwrap()
            .read_to_string(&mut launcher_log)
            .unwrap();
        assert!(
            !launcher_log.contains("secret-token"),
            "Token 必须被遮蔽: {launcher_log}"
        );

        drop(archive);
        let _ = std::fs::remove_dir_all(&root);
    }
}
