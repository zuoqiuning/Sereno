//! 实例健康检查：把启动前预检、Mod 依赖图与崩溃诊断串成一份报告。
//!
//! 设计目标是「可解释 + 可修复」：每条结论都说明为什么，
//! 能在启动器内直接处理的项附带一个动作入口，而不是让用户自己去翻日志。

pub mod mods;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Result;
use crate::models::health::{HealthAction, HealthCheck, HealthReport};
use crate::models::settings::AppSettings;
use crate::models::version::VersionDetail;
use crate::storage::instance::read_metadata;
use crate::utils::http::build_client;
use crate::utils::paths::instance_dir;
use crate::version::rules::OsContext;

/// 运行一次完整健康检查。
pub async fn check(
    settings: &AppSettings,
    instance_id: &str,
    running: bool,
) -> Result<HealthReport> {
    let inst_dir = instance_dir(settings, instance_id)?;
    let metadata = read_metadata(&inst_dir)?;

    let client = build_client(settings)?;
    let version_id = crate::loader::resolve_installed_version(&inst_dir, &metadata);
    let version = match version_id.as_deref() {
        Some(id) => crate::version::detail::resolve_for_instance(&client, None, &inst_dir, id)
            .await
            .ok(),
        None => None,
    };

    let mut checks = vec![
        check_instance(&inst_dir, version.as_ref()),
        check_java(settings, &metadata, version.as_ref()).await,
        check_memory(settings, &metadata),
        check_disk(&inst_dir, version.as_ref()),
    ];

    let mods_dir = inst_dir.join("minecraft").join("mods");
    let mods = mods::analyze(&mods_dir);
    checks.push(check_mods(&mods));

    if running {
        checks.push(HealthCheck {
            id: "running".into(),
            title: "游戏正在运行".into(),
            status: "warn".into(),
            detail: "修复、重装或切换加载器前请先退出游戏。".into(),
            action: None,
        });
    }

    let logs = crate::instance::logs::read(settings, instance_id).ok();
    let crash = logs.as_ref().map(|logs| logs.analysis.clone()).unwrap_or(
        crate::models::crash::CrashAnalysis {
            findings: Vec::new(),
            excerpt: Vec::new(),
        },
    );

    // 只有真的存在崩溃报告时才把日志诊断列为预检项，
    // 否则 latest.log 里残留的旧警告会制造假故障。
    if logs.as_ref().is_some_and(|logs| !logs.crashes.is_empty())
        && let Some(finding) = crash.findings.first()
    {
        checks.push(HealthCheck {
            id: "crash".into(),
            title: "最近一次崩溃".into(),
            status: if finding.severity == "error" {
                "error".into()
            } else {
                "warn".into()
            },
            detail: format!("{}：{}", finding.title, finding.suggestion),
            action: Some(HealthAction {
                kind: "open-logs".into(),
                label: "查看崩溃报告".into(),
            }),
        });
    }

    Ok(HealthReport {
        instance_id: instance_id.to_string(),
        checks,
        mods,
        crash,
        checked_at: now_secs().to_string(),
    })
}

fn check_instance(inst_dir: &Path, version: Option<&VersionDetail>) -> HealthCheck {
    let Some(version) = version else {
        return HealthCheck {
            id: "instance".into(),
            title: "游戏文件".into(),
            status: "error".into(),
            detail: "版本文件缺失或无法解析，实例无法启动。".into(),
            action: Some(repair_action("重新下载游戏文件")),
        };
    };

    let jar_id = version.jar.as_deref().unwrap_or(&version.id);
    let client_jar = inst_dir
        .join("versions")
        .join(jar_id)
        .join(format!("{jar_id}.jar"));

    let mut missing = 0usize;
    if !client_jar.is_file() {
        missing += 1;
    }

    let os_ctx = OsContext::current();
    for lib in crate::version::rules::filter_libraries(&version.libraries, &os_ctx) {
        if !crate::version::rules::applies_to_client(lib) {
            continue;
        }
        let relative = lib
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
            .and_then(|artifact| artifact.path.clone())
            .or_else(|| crate::version::artifact::maven_relative_path(&lib.name));
        if let Some(relative) = relative
            && !inst_dir.join("libraries").join(relative).is_file()
        {
            missing += 1;
        }
    }

    if missing > 0 {
        HealthCheck {
            id: "instance".into(),
            title: "游戏文件不完整".into(),
            status: "error".into(),
            detail: format!("缺少客户端主文件或 {missing} 个依赖库；可以直接重新下载补齐。"),
            action: Some(repair_action("修复游戏文件")),
        }
    } else {
        HealthCheck {
            id: "instance".into(),
            title: "游戏文件".into(),
            status: "ok".into(),
            detail: format!("客户端与 {} 个依赖库均已就绪。", version.libraries.len()),
            action: None,
        }
    }
}

async fn check_java(
    settings: &AppSettings,
    metadata: &crate::models::instance::InstanceMetadata,
    version: Option<&VersionDetail>,
) -> HealthCheck {
    let required = version.and_then(|version| version.required_java_major());
    let custom = metadata
        .java_path
        .as_deref()
        .or(settings.java.custom_path.as_deref());

    match crate::java::detector::resolve(required, custom).await {
        Ok(java) => HealthCheck {
            id: "java".into(),
            title: "Java 运行时".into(),
            status: "ok".into(),
            detail: format!(
                "Java {}（{} 位）· {}",
                java.version_string, java.arch, java.path
            ),
            action: None,
        },
        Err(error) => HealthCheck {
            id: "java".into(),
            title: "Java 运行时".into(),
            status: "error".into(),
            detail: match required {
                Some(major) => format!("找不到可用的 Java {major}（{error}）。"),
                None => format!("找不到可用的 Java（{error}）。"),
            },
            action: Some(HealthAction {
                kind: "open-settings".into(),
                label: "选择 Java".into(),
            }),
        },
    }
}

fn check_memory(
    settings: &AppSettings,
    metadata: &crate::models::instance::InstanceMetadata,
) -> HealthCheck {
    let memory_mb = metadata
        .memory_mb
        .unwrap_or(settings.java.default_memory_mb.unwrap_or(2048));

    let mut detail = format!("已分配 {memory_mb} MB 堆内存。");
    let mut status = "ok";
    if let Some(total) = total_memory_bytes() {
        let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
        detail = format!("已分配 {memory_mb} MB，物理内存共 {total_gb:.1} GB。");
        if u64::from(memory_mb) * 1024 * 1024 > total * 8 / 10 {
            status = "warn";
            detail = format!(
                "已分配 {memory_mb} MB，超过物理内存（{total_gb:.1} GB）的 80%，可能拖慢系统。"
            );
        }
    }
    if memory_mb < 1024 {
        status = "warn";
        detail = format!("只分配了 {memory_mb} MB，带有 Mod 的实例容易内存不足。");
    }

    HealthCheck {
        id: "memory".into(),
        title: "内存分配".into(),
        status: status.into(),
        detail,
        action: (status != "ok").then(|| HealthAction {
            kind: "open-settings".into(),
            label: "调整内存".into(),
        }),
    }
}

fn check_disk(inst_dir: &Path, version: Option<&VersionDetail>) -> HealthCheck {
    let expected = version.map(expected_bytes).unwrap_or(0);
    let Some(available) = available_space(inst_dir) else {
        return HealthCheck {
            id: "disk".into(),
            title: "磁盘空间".into(),
            status: "ok".into(),
            detail: "无法读取磁盘剩余空间，已跳过检查。".into(),
            action: None,
        };
    };

    let available_text = human_bytes(available);
    let expected_text = human_bytes(expected);
    let mut detail = format!("剩余 {available_text}");
    if expected > 0 {
        detail.push_str(&format!("，完整文件约 {expected_text}"));
    }

    let status = if available < 256 * 1024 * 1024 {
        detail.push_str("；空间过低，下载与存档都可能失败。");
        "error"
    } else if expected > 0 && available < expected {
        detail.push_str("；剩余空间小于完整安装体积，修复时可能中途失败。");
        "warn"
    } else {
        detail.push('。');
        "ok"
    };

    HealthCheck {
        id: "disk".into(),
        title: "磁盘空间".into(),
        status: status.into(),
        detail,
        action: None,
    }
}

fn check_mods(mods: &crate::models::health::ModHealth) -> HealthCheck {
    let total = mods.enabled + mods.disabled;
    if total == 0 {
        return HealthCheck {
            id: "mods".into(),
            title: "Mod 依赖".into(),
            status: "ok".into(),
            detail: "没有安装 Mod。".into(),
            action: None,
        };
    }

    let issues = mods.missing.len() + mods.mismatched.len() + mods.duplicates.len();
    if issues == 0 {
        return HealthCheck {
            id: "mods".into(),
            title: "Mod 依赖".into(),
            status: "ok".into(),
            detail: format!("{} 个启用的 Mod 依赖完整。", mods.enabled),
            action: None,
        };
    }

    let mut parts = Vec::new();
    if !mods.missing.is_empty() {
        parts.push(format!("缺失 {} 个前置", mods.missing.len()));
    }
    if !mods.mismatched.is_empty() {
        parts.push(format!("{} 个版本不满足要求", mods.mismatched.len()));
    }
    if !mods.duplicates.is_empty() {
        parts.push(format!("{} 组重复安装", mods.duplicates.len()));
    }

    HealthCheck {
        id: "mods".into(),
        title: "Mod 依赖".into(),
        status: "error".into(),
        detail: format!("{}；修复后游戏才能正常启动。", parts.join("，")),
        action: Some(HealthAction {
            kind: "open-mods".into(),
            label: "打开 Mod 目录".into(),
        }),
    }
}

fn repair_action(label: &str) -> HealthAction {
    HealthAction {
        kind: "repair".into(),
        label: label.to_string(),
    }
}

/// 完整安装大约需要多少字节：客户端 + 依赖库 + 资源对象。
fn expected_bytes(version: &VersionDetail) -> u64 {
    let client = version
        .client_download()
        .map(|download| u64::from(download.size))
        .unwrap_or(0);
    let assets = version
        .asset_index
        .as_ref()
        .map(|index| u64::from(index.total_size))
        .unwrap_or(0);
    let libraries: u64 = version
        .libraries
        .iter()
        .filter_map(|lib| lib.downloads.as_ref())
        .filter_map(|downloads| downloads.artifact.as_ref())
        .map(|artifact| u64::from(artifact.size))
        .sum();

    client + assets + libraries
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(windows)]
fn total_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: status 是合法的 MEMORYSTATUSEX，dwLength 已按结构体大小填好。
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0).then_some(status.ullTotalPhys)
}

#[cfg(not(windows))]
fn total_memory_bytes() -> Option<u64> {
    None
}

#[cfg(windows)]
fn available_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);

    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    // SAFETY: wide 以 NUL 结尾；输出指针指向栈上的 u64。
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total,
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free_to_caller)
}

#[cfg(not(windows))]
fn available_space(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_with_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
