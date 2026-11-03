//! 加载器域：Forge / NeoForge / Fabric / Quilt 的安装。

pub mod fabric;
pub mod forge;
pub mod install;
pub mod meta;
pub mod neoforge;
pub mod optifine;
pub mod quilt;

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zip::ZipArchive;

use crate::download::counter::SessionCounter;
use crate::download::queue::DownloadQueue;
use crate::download::session::DownloadSession;
use crate::download::sink::NoopSink;
use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::models::instance::InstanceMetadata;
use crate::models::settings::AppSettings;
use crate::version::artifact::maven_relative_path;
use crate::{LauncherError, Result};

/// 安装器 JAR 的最小可信体积（防止把 HTML 错误页当成 jar 执行）。
const MIN_INSTALLER_BYTES: u64 = 64 * 1024;

/// 下载加载器安装器 JAR（带镜像回退与重试）。
///
/// `prefer_mirror` 为真时先走 BMCLAPI 的 Maven 镜像，国内下载明显更快；
/// 镜像不可用时下载队列会自动回退官方地址。
pub(crate) async fn fetch_installer(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    label: &str,
    prefer_mirror: bool,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        crate::utils::paths::ensure_dir(parent)?;
    }
    // 安装器没有官方哈希，且旧文件可能已损坏，每次都重新拉取
    let _ = std::fs::remove_file(dest);

    let queue = std::sync::Arc::new(DownloadQueue::new(client.clone(), 4));
    let counter = std::sync::Arc::new(SessionCounter::new("loader-installer", "loader"));
    counter.add_total(1, 0);

    queue
        .execute_task(
            DownloadTask {
                id: "installer".to_string(),
                display_name: label.to_string(),
                sources: crate::download::mirror::build_sources_with(url, prefer_mirror),
                destination: dest.to_path_buf(),
                checksum: Checksum::None,
                size_hint: None,
                priority: 6,
                retry_policy: RetryPolicy {
                    max_retries: 5,
                    ..RetryPolicy::default()
                },
            },
            CancellationToken::new(),
            None,
            "loader-installer",
            counter,
        )
        .await?;

    let size = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if size < MIN_INSTALLER_BYTES {
        let _ = std::fs::remove_file(dest);
        return Err(LauncherError::DownloadFailed {
            message: format!("{label} 下载结果不是有效 JAR（{size} 字节）"),
            url: url.to_string(),
        });
    }

    Ok(())
}

/// 运行安装器，带超时与 stderr 回传。
pub(crate) async fn run_installer(
    java_exe: &str,
    installer: &Path,
    args: &[&str],
    work_dir: &Path,
    label: &str,
) -> Result<()> {
    use std::process::Stdio;

    let mut command = tokio::process::Command::new(java_exe);
    command
        .arg("-jar")
        .arg(installer)
        .args(args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = tokio::time::timeout(Duration::from_secs(600), command.output())
        .await
        .map_err(|_| LauncherError::LoaderInstallFailed {
            message: format!("{label} 安装器超时（10 分钟），已中止"),
        })?
        .map_err(|e| LauncherError::Internal {
            message: format!("启动 {label} 安装器失败: {e}"),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined: Vec<String> = Vec::new();
    combined.extend(stdout.lines().map(str::to_string));
    combined.extend(stderr.lines().map(str::to_string));

    // 安装器的真实错误经常只打在 stdout，必须两边都记
    let tail = combined
        .iter()
        .rev()
        .take(30)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if !tail.is_empty() {
        tracing::info!(target: "sereno_core::loader", "[{label} 安装器]\n{tail}");
    }

    if !output.status.success() {
        return Err(LauncherError::LoaderInstallFailed {
            message: format!("{label} 安装器退出码 {:?}\n{tail}", output.status.code()),
        });
    }

    Ok(())
}

/// 预先下载 Forge / NeoForge 安装器声明的支持库。
///
/// 安装器自己下载依赖是**串行**的，而且默认直连 `maven.neoforged.net`，
/// 国内经常超时（这正是 NeoForge 安装失败与“太慢”的主因）。
/// 这里按 PCL 的做法先从 `install_profile.json` + `version.json` 收集依赖，
/// 用启动器自己的多线程 + 镜像下载器拉齐，再让安装器只做本地处理器工作。
pub(crate) async fn preinstall_libraries(
    client: &reqwest::Client,
    installer: &Path,
    instance_dir: &Path,
    settings: &AppSettings,
    mc_version: &str,
    label: &str,
) -> Result<usize> {
    let file = File::open(installer).map_err(|e| LauncherError::Io {
        message: format!("打开 {label} 安装器失败: {e}"),
    })?;

    // 离线安装的前提：原版客户端已完整落盘
    let client_jar = instance_dir
        .join("versions")
        .join(mc_version)
        .join(format!("{mc_version}.jar"));
    let client_json = instance_dir
        .join("versions")
        .join(mc_version)
        .join(format!("{mc_version}.json"));
    if !client_jar.exists() || !client_json.exists() {
        return Err(LauncherError::LoaderInstallFailed {
            message: format!(
                "{label} 安装前需要原版 {mc_version} 的 JSON 与客户端 jar，请先完成原版安装"
            ),
        });
    }

    let mut archive = ZipArchive::new(file).map_err(|e| LauncherError::Parse {
        message: format!("解析 {label} 安装器失败: {e}"),
    })?;

    let profile = read_zip_json(&mut archive, "install_profile.json");
    let version_json = read_zip_json(&mut archive, "version.json");
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut tasks = Vec::new();

    for json in [profile.as_ref(), version_json.as_ref()]
        .into_iter()
        .flatten()
    {
        let Some(libraries) = json.get("libraries").and_then(|v| v.as_array()) else {
            continue;
        };
        for lib in libraries {
            let Some(task) = library_task(lib, instance_dir, settings.download.prefer_mirror)
            else {
                continue;
            };
            if seen.insert(task.destination.clone()) {
                tasks.push(task);
            }
        }
    }

    // 新版 Forge/NeoForge 处理器需要 Mojang mappings（PCL 也会单独补这一项）
    if let Some(relative) = mappings_relative_path(&profile) {
        let local = crate::version::detail::read_local(instance_dir, mc_version)?
            .and_then(|detail| detail.downloads)
            .and_then(|downloads| downloads.client_mappings);

        // 旧版本保存的 JSON 可能没有 mappings；回源拉一次并缓存
        let mappings = match local {
            Some(mappings) => mappings,
            None => {
                tracing::info!("本地版本 JSON 缺少 mappings，正在重新获取 {mc_version} 元数据");
                let manifest = crate::version::manifest::fetch_manifest(client).await?;
                let detail =
                    crate::version::detail::resolve_detail(client, &manifest, mc_version).await?;
                detail
                    .downloads
                    .and_then(|downloads| downloads.client_mappings)
                    .ok_or_else(|| LauncherError::LoaderInstallFailed {
                        message: format!(
                            "缺少原版 {mc_version} 的 mappings 信息，无法离线安装 {label}"
                        ),
                    })?
            }
        };

        let dest = instance_dir.join("libraries").join(&relative);
        if !dest.exists() && seen.insert(dest.clone()) {
            tasks.push(DownloadTask {
                id: "loader-mappings".to_string(),
                display_name: file_name_of(&relative),
                sources: crate::download::mirror::build_sources_with(
                    &mappings.url,
                    settings.download.prefer_mirror,
                ),
                destination: dest,
                checksum: Checksum::Sha1(mappings.sha1.clone()),
                size_hint: Some(mappings.size as u64),
                priority: 5,
                retry_policy: RetryPolicy {
                    max_retries: 5,
                    ..RetryPolicy::default()
                },
            });
        }
    }

    if tasks.is_empty() {
        return Ok(0);
    }

    let total_bytes: u64 = tasks.iter().filter_map(|t| t.size_hint).sum();
    let counter = std::sync::Arc::new(SessionCounter::new("loader-libs", "loader"));
    counter.add_total(tasks.len() as u32, total_bytes);

    let queue = std::sync::Arc::new(DownloadQueue::new(
        client.clone(),
        // 小文件并发 8 足够；并发过高会被 BMCL 以 503 限流
        (settings.download.max_concurrent as usize).min(8),
    ));

    tracing::info!(
        "{label} 预下载 {} 个支持库（{:.1} MB），并发 {}，镜像优先 {}",
        tasks.len(),
        total_bytes as f64 / 1024.0 / 1024.0,
        settings.download.max_concurrent,
        settings.download.prefer_mirror
    );

    DownloadSession::new("loader-libs".into(), queue)
        .run_with_counter(tasks, std::sync::Arc::new(NoopSink), counter)
        .await?;

    Ok(seen.len())
}

fn read_zip_json(archive: &mut ZipArchive<File>, name: &str) -> Option<Value> {
    let mut entry = archive.by_name(name).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

fn library_task(lib: &Value, instance_dir: &Path, prefer_mirror: bool) -> Option<DownloadTask> {
    let name = lib.get("name").and_then(|v| v.as_str())?;
    let artifact = lib.get("downloads").and_then(|d| d.get("artifact"));

    let (url, relative, sha1, size) = match (artifact, lib.get("url").and_then(|v| v.as_str())) {
        (Some(artifact), _) => {
            let url = artifact.get("url").and_then(|v| v.as_str())?;
            let relative = artifact
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| maven_relative_path(name))?;
            (
                url.to_string(),
                relative,
                artifact
                    .get("sha1")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                artifact.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
            )
        }
        (None, Some(base)) => {
            let relative = maven_relative_path(name)?;
            (
                format!("{}/{}", base.trim_end_matches('/'), relative),
                relative,
                String::new(),
                0,
            )
        }
        (None, None) => return None,
    };

    if !url.starts_with("http") {
        return None;
    }

    let checksum = if sha1.is_empty() {
        Checksum::None
    } else {
        Checksum::Sha1(sha1)
    };

    Some(DownloadTask {
        id: format!("loader-lib-{name}"),
        display_name: file_name_of(&relative),
        sources: crate::download::mirror::build_sources_with(&url, prefer_mirror),
        destination: instance_dir.join("libraries").join(&relative),
        checksum,
        size_hint: (size > 0).then_some(size),
        priority: 7,
        retry_policy: RetryPolicy::default(),
    })
}

/// 从 `install_profile.data.MOJMAPS.client` 推导 mappings 在 `libraries/` 下的路径。
///
/// 例：`[net.minecraft:client:1.21.1-20240808.144430:mappings@txt]`
/// → `net/minecraft/client/1.21.1-20240808.144430/client-1.21.1-20240808.144430-mappings.txt`
fn mappings_relative_path(profile: &Option<Value>) -> Option<String> {
    let coord = profile
        .as_ref()?
        .get("data")?
        .get("MOJMAPS")?
        .get("client")?
        .as_str()?
        .trim_matches(|c| c == '[' || c == ']')
        .split('@')
        .next()?
        .to_string();

    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() != 4 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts[3];
    Some(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}-{classifier}.txt"
    ))
}

fn file_name_of(relative: &str) -> String {
    relative.rsplit('/').next().unwrap_or(relative).to_string()
}

/// 找出实例里**实际安装**的加载器版本目录名。
///
/// 不同安装器生成的 id 拼写并不一致（`1.21.1-forge-52.0.16`、
/// `fabric-loader-0.16.9-1.21.1`、`1.21.1-neoforge-21.1.172`），
/// 因此以磁盘上的 JSON 为准：扫描 `versions/*/*.json`，取
/// 「继承自当前 MC 版本 + 包含加载器版本号」的最新一个。
/// 扫描不到才回退到按命名规则推算。
pub fn resolve_installed_version(
    instance_dir: &Path,
    metadata: &InstanceMetadata,
) -> Option<String> {
    let loader_type = metadata.loader.r#type.to_lowercase();
    if loader_type == "vanilla" || loader_type.is_empty() {
        return Some(metadata.game_version.clone());
    }
    let loader_version = metadata.loader.version.as_deref().unwrap_or("");

    let mut newest: Option<(SystemTime, String)> = None;
    if let Ok(entries) = std::fs::read_dir(instance_dir.join("versions")) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(&dir) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
                    continue;
                };
                let id = json.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                let inherits = json
                    .get("inheritsFrom")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                if inherits != metadata.game_version
                    || !id.to_lowercase().contains(&loader_type)
                    || (!loader_version.is_empty() && !id.contains(loader_version))
                {
                    continue;
                }

                let mtime = file
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                if newest.as_ref().is_none_or(|(t, _)| mtime > *t) {
                    newest = Some((mtime, id.to_string()));
                }
            }
        }
    }

    if let Some((_, id)) = newest {
        return Some(id);
    }

    let mc = &metadata.game_version;
    let fallback = match loader_type.as_str() {
        "fabric" => format!("fabric-loader-{loader_version}-{mc}"),
        "quilt" => format!("quilt-loader-{loader_version}-{mc}"),
        "forge" => format!("{mc}-forge-{loader_version}"),
        "neoforge" => format!("{mc}-neoforge-{loader_version}"),
        "optifine" => format!("{mc}-OptiFine_{loader_version}"),
        _ => return None,
    };

    let exists = instance_dir
        .join("versions")
        .join(&fallback)
        .join(format!("{fallback}.json"))
        .exists();
    exists.then_some(fallback)
}
