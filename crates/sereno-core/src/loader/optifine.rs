//! OptiFine 安装。
//!
//! ## 为什么不用图形界面
//!
//! OptiFine 的安装器是个 Swing 程序，直接 `java -jar` 会弹窗并一直阻塞。
//! 但它的**核心类 `optifine.Installer` 本身就是无头的**：跑起来直接往
//! `<Minecraft 目录>/versions/<新版本>/` 里写版本 JSON 与打过补丁的客户端 jar，
//! 全程只在 stdout 打印进度。图形界面是另一个类 `optifine.InstallerFrame`。
//!
//! ## 安装器怎么找到目标目录
//!
//! `optifine.Utils.getWorkingDirectory()` 的逻辑（从 class 常量池里读出来的）：
//!
//! - Windows：取环境变量 `APPDATA`，拼上 `.minecraft`；
//! - 其它平台：取系统属性 `user.home`，拼上 `.minecraft`。
//!
//! 因此**只要把 `APPDATA` 与 `user.home` 都指到沙箱目录**，
//! 安装器就会把结果写进沙箱，完全不碰用户的真实游戏目录。
//!
//! ## 沙箱里要放什么
//!
//! 安装器会把 `<minecraft>/versions/<mc>/` **整个复制**成
//! `<minecraft>/versions/<mc>-OptiFine_xxx/`，所以源目录里必须有原版的
//! 版本 JSON 与客户端 jar。Sereno 的实例目录本来就有一份，复制过去即可。
//!
//! ## Java 版本
//!
//! 安装器 class 文件的 major version 减 44 就是它需要的 Java 主版本
//! （class 52 → Java 8）。这是 PCL 用的同一套换算。
//!
//! ## 弹窗陷阱（重要）
//!
//! 安装器内部任何异常都会走 `optifine.Utils.showErrorMessage`，
//! 而它是 `javax.swing.JOptionPane.showMessageDialog`——**一个模态对话框**。
//! 无头运行时会弹出真实的 Error 窗口，并且**进程永远不退出**，
//! 只能等到超时被杀掉。
//!
//! 最常见的触发点是缺少 `<minecraft>/launcher_profiles.json`：
//! 安装器装完补丁后会去更新原版启动器的配置档案，文件不在就抛
//! 「File not found」并弹窗。
//!
//! 因此这里做了两道防线：
//!
//! 1. 沙箱里**预置** `launcher_profiles.json`（内容与 PCL 的做法一致），
//!    让安装器有档案可更新，不进入错误分支；
//! 2. 运行时加 `-Djava.awt.headless=true`，万一还有别的路径触发弹窗，
//!    会抛 `HeadlessException` 快速失败，而不是挂起等用户点「确定」。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;

use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::models::instance::LoaderInfo;
use crate::{LauncherError, Result};

/// BMCLAPI 的 OptiFine 版本列表接口（按 MC 版本过滤，体积更小）。
const VERSION_LIST_API: &str = "https://bmclapi2.bangbang93.com/optifine/versionList";

/// BMCLAPI 的「按 MC 版本取列表」接口。
const VERSION_LIST_BY_MC_API: &str = "https://bmclapi2.bangbang93.com/optifine";

/// 安装超时。安装器要逐个 class 打补丁，几十秒起步。
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

/// 版本列表里的一条记录。
#[derive(Debug, Clone, Deserialize)]
struct VersionListEntry {
    /// 对应的 Minecraft 版本，如 `1.20.1`。
    #[serde(default)]
    mcversion: String,
    /// 补丁号，如 `I6`。
    #[serde(default)]
    patch: String,
    /// 版本类型，如 `HD_U`。
    #[serde(default, rename = "type")]
    kind: String,
    /// 文件名，如 `OptiFine_1.20.1_HD_U_I6.jar`。
    #[serde(default)]
    filename: String,
}

/// 一个可选的 OptiFine 版本（供界面展示与选择）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OptiFineVersion {
    /// 展示用标签，如 `1.20.1 HD_U I6`。
    pub label: String,
    /// Minecraft 版本。
    pub mc_version: String,
    /// 版本类型，如 `HD_U`。
    ///
    /// **注意类型本身带下划线**，因此不能靠「按下划线切分」还原
    /// 类型与补丁号——必须分开存。
    pub kind: String,
    /// 补丁号，如 `I6`。
    pub patch: String,
    /// 拼接版本号，如 `HD_U_I6`；写进实例元数据，启动时用它匹配版本目录。
    pub edition: String,
    /// 安装器文件名。
    pub file_name: String,
}

/// 拉取 OptiFine 版本列表。
///
/// 优先用「按 MC 版本取」的接口（响应小得多），
/// 失败时回落到全量列表接口再本地过滤。
pub async fn list_versions(
    client: &reqwest::Client,
    mc_version: &str,
) -> Result<Vec<OptiFineVersion>> {
    let per_mc = format!("{VERSION_LIST_BY_MC_API}/{mc_version}");
    match fetch_entries(client, &per_mc).await {
        Ok(entries) => Ok(to_versions(entries, mc_version)),
        Err(error) => {
            tracing::debug!(%error, "按版本取 OptiFine 列表失败，回落到全量列表");
            let entries = fetch_entries(client, VERSION_LIST_API).await?;
            Ok(to_versions(entries, mc_version))
        }
    }
}

/// 拉取版本列表原始数据。
async fn fetch_entries(client: &reqwest::Client, url: &str) -> Result<Vec<VersionListEntry>> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("获取 OptiFine 版本列表失败：{e}"),
            url: url.to_string(),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("获取 OptiFine 版本列表失败：服务器返回 HTTP {status}"),
            url: url.to_string(),
        });
    }

    crate::utils::http::read_json_limited(response, 8 * 1024 * 1024).await
}

/// 把原始条目转成可选版本列表。
fn to_versions(entries: Vec<VersionListEntry>, mc_version: &str) -> Vec<OptiFineVersion> {
    let mut versions: Vec<OptiFineVersion> = entries
        .into_iter()
        .filter(|entry| mc_version.is_empty() || entry.mcversion == mc_version)
        .filter(|entry| !entry.mcversion.is_empty() && !entry.patch.is_empty())
        .filter(|entry| !entry.kind.is_empty())
        .map(|entry| {
            let edition = format!("{}_{}", entry.kind, entry.patch);
            OptiFineVersion {
                label: format!("{} {} {}", entry.mcversion, entry.kind, entry.patch),
                mc_version: entry.mcversion,
                kind: entry.kind,
                patch: entry.patch,
                edition,
                file_name: entry.filename,
            }
        })
        .collect();

    // 同一条记录可能在列表里出现多次（不同 `_id`），按标签去重
    versions.sort_by(|a, b| a.label.cmp(&b.label));
    versions.dedup_by(|a, b| a.label == b.label);

    versions
}

/// 按标签查找某个 OptiFine 版本。
pub fn find_by_label(versions: &[OptiFineVersion], label: &str) -> Option<OptiFineVersion> {
    versions
        .iter()
        .find(|version| version.label == label)
        .or_else(|| {
            // 允许只给补丁号（如 `I6`）：界面上常常只显示补丁号
            versions
                .iter()
                .find(|version| version.edition.ends_with(label))
        })
        .cloned()
}

/// 安装器的下载地址。
///
/// BMCLAPI 的路径格式是 `/optifine/<mc版本>/<类型>/<补丁号>`。
fn download_url(entry: &OptiFineVersion) -> Result<String> {
    if entry.kind.is_empty() || entry.patch.is_empty() {
        return Err(LauncherError::InvalidConfig {
            key: "optifine.edition".into(),
            message: format!("OptiFine 版本号不完整: {}", entry.label),
        });
    }

    Ok(format!(
        "https://bmclapi2.bangbang93.com/optifine/{}/{}/{}",
        entry.mc_version, entry.kind, entry.patch
    ))
}

/// 安装 OptiFine 到实例。
pub async fn install_optifine(
    client: &reqwest::Client,
    settings: &crate::models::settings::AppSettings,
    instance_dir: &Path,
    mc_version: &str,
    version: &OptiFineVersion,
) -> Result<LoaderInfo> {
    // 1. 下载安装器
    let installer = fetch_installer(client, settings, version).await?;

    // 2. 安装器要求的 Java 版本（从 class 文件头读出来）
    let required_java = required_java_major(&installer)?.unwrap_or(8);
    // 安装器只给出「最低 Java 版本」（class 52 → Java 8），
    // 用更新的 Java 跑同样没问题，因此这里按「不低于下限」选。
    let java = crate::java::detector::resolve_at_least(
        required_java,
        settings.java.custom_path.as_deref(),
    )
    .await?;

    // 3. 准备沙箱：把原版版本文件复制进去
    let sandbox = crate::utils::paths::cache_dir()?
        .join("optifine")
        .join("sandbox");
    let _ = std::fs::remove_dir_all(&sandbox);
    let minecraft_dir = sandbox.join(".minecraft");
    let version_dir = minecraft_dir.join("versions").join(mc_version);
    crate::utils::paths::ensure_dir(&version_dir)?;

    for file_name in [format!("{mc_version}.json"), format!("{mc_version}.jar")] {
        let source = instance_dir
            .join("versions")
            .join(mc_version)
            .join(&file_name);
        if !source.is_file() {
            return Err(LauncherError::LoaderInstallFailed {
                message: format!("安装 OptiFine 前需要先装好原版 {mc_version}（缺少 {file_name}）"),
            });
        }
        let _ = std::fs::remove_file(version_dir.join(&file_name));
        std::fs::copy(&source, version_dir.join(&file_name)).map_err(|e| LauncherError::Io {
            message: format!("准备 OptiFine 安装沙箱失败：{e}"),
        })?;
    }
    crate::utils::paths::ensure_dir(&minecraft_dir.join("libraries"))?;

    // 3.5 预置 launcher_profiles.json
    //
    // 安装器装完补丁后会去更新原版启动器的配置档案。文件不存在时它会
    // 抛「File not found」并弹一个 Swing 模态框，**进程不会退出**——
    // 用户会看到一个 Error 窗口，任务则一直挂到超时。
    // 这里先造一份最小可用的档案，安装器就能正常走完更新流程。
    write_launcher_profiles(&minecraft_dir)?;

    // 4. 跑安装器
    let run = run_installer(&java.path, &installer, &sandbox).await?;

    // 5. 收集产物
    //
    // **以产物判定成败，而不是看退出码**：headless 下安装器可能抛
    // HeadlessException 后退出码非 0，但它写出来的版本文件其实完好
    // （补丁是在报错之前打的）。反过来，退出码为 0 也不保证产物齐全。
    let Some(produced) = collect_produced(&minecraft_dir, mc_version) else {
        return Err(LauncherError::LoaderInstallFailed {
            message: format!(
                "OptiFine 安装器没有产出任何版本文件。安装器输出：\n{}",
                tail(&run.output)
            ),
        });
    };

    if !run.success {
        // 产物齐全但退出码异常：记录下来，不阻断安装。
        tracing::warn!(
            exit_code = ?run.exit_code,
            version = %produced.id,
            "OptiFine 安装器退出码非零，但版本文件已产出，继续安装"
        );
    }

    // 6. 把版本 JSON 与打过补丁的 jar 放进实例
    let target = instance_dir.join("versions").join(&produced.id);
    crate::utils::paths::ensure_dir(&target)?;
    for (name, source) in &produced.files {
        std::fs::copy(source, target.join(name)).map_err(|e| LauncherError::Io {
            message: format!("写入 OptiFine 版本文件失败：{e}"),
        })?;
    }

    // 7. 把 OptiFine 自己的库文件搬进实例
    copy_optifine_libraries(
        &minecraft_dir.join("libraries"),
        &instance_dir.join("libraries"),
    )?;

    // 8. 清理沙箱：里面有一份打过补丁的客户端 jar（几十 MB）
    let _ = std::fs::remove_dir_all(&sandbox);

    tracing::info!(
        instance = %instance_dir.display(),
        version = %produced.id,
        "OptiFine 安装完成"
    );

    Ok(LoaderInfo {
        r#type: "optifine".to_string(),
        version: Some(version.edition.clone()),
    })
}

/// 下载安装器到缓存目录。
async fn fetch_installer(
    client: &reqwest::Client,
    settings: &crate::models::settings::AppSettings,
    version: &OptiFineVersion,
) -> Result<PathBuf> {
    let dir = crate::utils::paths::cache_dir()?.join("optifine");
    crate::utils::paths::ensure_dir(&dir)?;
    let path = dir.join(&version.file_name);

    let url = download_url(version)?;

    // 已下载过就直接用：安装器是固定版本，重复下载没意义
    if path.is_file()
        && let Ok(meta) = std::fs::metadata(&path)
        && meta.len() > 1024 * 1024
    {
        return Ok(path);
    }

    let queue = crate::download::queue::DownloadQueue::new(client.clone(), 2);
    let counter = std::sync::Arc::new(crate::download::counter::SessionCounter::new(
        "optifine-fetch",
        "loader",
    ));
    counter.add_total(1, 0);

    queue
        .execute_task(
            DownloadTask {
                id: "optifine-installer".to_string(),
                display_name: format!("OptiFine {}", version.label),
                sources: crate::download::mirror::build_sources_with(
                    &url,
                    settings.download.prefer_mirror,
                ),
                destination: path.clone(),
                checksum: Checksum::None,
                size_hint: None,
                priority: 6,
                retry_policy: RetryPolicy {
                    max_retries: 3,
                    ..RetryPolicy::default()
                },
            },
            tokio_util::sync::CancellationToken::new(),
            None,
            "optifine-fetch",
            counter,
        )
        .await?;

    // 安装器至少几 MB，太小的多半是错误页
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if size < 1024 * 1024 {
        let _ = std::fs::remove_file(&path);
        return Err(LauncherError::DownloadFailed {
            message: format!("下载到的 OptiFine 安装器只有 {size} 字节，不是有效的 jar"),
            url,
        });
    }

    Ok(path)
}

/// 从安装器 jar 里读出它需要的 Java 主版本。
///
/// `optifine/Installer.class` 的 class 文件头第 6–7 字节是 major version，
/// 减 44 就是 Java 主版本（class 52 → Java 8）。
pub fn required_java_major(installer: &Path) -> Result<Option<u32>> {
    let file = std::fs::File::open(installer).map_err(|e| LauncherError::Io {
        message: format!("打开 OptiFine 安装器失败：{e}"),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| LauncherError::Parse {
        message: format!("解析 OptiFine 安装器失败：{e}"),
    })?;

    let mut entry =
        archive
            .by_name("optifine/Installer.class")
            .map_err(|_| LauncherError::Parse {
                message: "OptiFine 安装器里没有 optifine/Installer.class，可能不是有效的安装器"
                    .into(),
            })?;

    let mut header = [0u8; 8];
    std::io::Read::read_exact(&mut entry, &mut header).map_err(|e| LauncherError::Parse {
        message: format!("读取 OptiFine 安装器 class 头失败：{e}"),
    })?;

    if header[0] != 0xCA || header[1] != 0xFE || header[2] != 0xBA || header[3] != 0xBE {
        return Err(LauncherError::Parse {
            message: "OptiFine 安装器的 class 文件头无效".into(),
        });
    }

    let class_version = u16::from_be_bytes([header[6], header[7]]) as u32;
    if !(49..=100).contains(&class_version) {
        return Err(LauncherError::Parse {
            message: format!("OptiFine 安装器 class 版本异常：{class_version}"),
        });
    }

    Ok(Some(class_version.saturating_sub(44)))
}

/// 在沙箱里写一份最小的 `launcher_profiles.json`。
///
/// 安装器结束时会把新版本登记进原版启动器的配置档案，
/// 文件缺失会触发模态错误框（见模块文档）。
///
/// 结构与官方启动器一致：一个 `profiles` 对象、`selectedProfile` 与
/// `clientToken`。档案名固定用 `Sereno`——沙箱是一次性的，
/// 这个名字不会被用户看到。
fn write_launcher_profiles(minecraft_dir: &Path) -> Result<()> {
    let path = minecraft_dir.join("launcher_profiles.json");

    let profiles = serde_json::json!({
        "profiles": {
            "Sereno": {
                "icon": "Grass",
                "name": "Sereno",
                "lastVersionId": "latest-release",
                "type": "latest-release",
                "lastUsed": "2026-01-01T00:00:00.0000Z"
            }
        },
        "selectedProfile": "Sereno",
        "clientToken": "23323323323323323323323323323333"
    });

    let text = serde_json::to_string_pretty(&profiles).map_err(|e| LauncherError::Io {
        message: format!("生成 launcher_profiles.json 失败：{e}"),
    })?;

    crate::utils::fs::atomic_write(&path, text.as_bytes()).map_err(|e| LauncherError::Io {
        message: format!("写入 launcher_profiles.json 失败：{e}"),
    })
}

/// 安装器的一次运行结果。
struct InstallerRun {
    /// 标准输出与标准错误合并后的文本。
    output: String,
    /// 退出码是否为零。
    success: bool,
    /// 退出码（进程被信号杀死时为 `None`）。
    exit_code: Option<i32>,
}

/// 运行安装器。
///
/// **环境变量与系统属性都指向沙箱**：安装器据此定位 Minecraft 目录，
/// 绝不会写到用户的真实游戏目录里。
///
/// **不因退出码非零直接判失败**：调用方应以「产物是否齐全」为准。
async fn run_installer(java_exe: &str, installer: &Path, sandbox: &Path) -> Result<InstallerRun> {
    let sandbox_str = sandbox.to_string_lossy().to_string();
    let installer_str = installer.to_string_lossy().to_string();

    let mut command = tokio::process::Command::new(java_exe);
    command
        // headless：安装器内部任何错误都会走 Swing 的模态对话框，
        // 无头环境下会挂起等用户点「确定」。加上这个参数后，
        // 同样的错误会抛 HeadlessException 快速失败。
        .arg("-Djava.awt.headless=true")
        .arg(format!("-Duser.home={sandbox_str}"))
        .arg("-cp")
        .arg(&installer_str)
        .arg("optifine.Installer")
        .current_dir(sandbox)
        // 关掉标准输入：安装器一旦等待输入就会永远卡住
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // 用户取消时上层 future 被 drop，必须把安装器子进程一起收掉，
        // 否则会留下一个还在打补丁的 Java 进程。
        .kill_on_drop(true)
        // Windows 上隐藏控制台窗口
        .env("APPDATA", &sandbox_str);

    #[cfg(windows)]
    {
        command.creation_flags(0x08000000);
    }

    let child = command
        .spawn()
        .map_err(|e| LauncherError::LoaderInstallFailed {
            message: format!("启动 OptiFine 安装器失败：{e}"),
        })?;

    // 超时后必须把子进程收掉，否则会留下一个常驻的 Java 进程
    let output = tokio::time::timeout(INSTALL_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| LauncherError::LoaderInstallFailed {
            message: format!(
                "OptiFine 安装器超时（{} 分钟），已中止",
                INSTALL_TIMEOUT.as_secs() / 60
            ),
        })?
        .map_err(|e| LauncherError::LoaderInstallFailed {
            message: format!("运行 OptiFine 安装器失败：{e}"),
        })?;

    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));

    let success = output.status.success();
    let exit_code = output.status.code();

    if !success {
        tracing::debug!(
            ?exit_code,
            "OptiFine 安装器退出码非零（以产物是否齐全判定成败）"
        );
    }

    Ok(InstallerRun {
        output: text,
        success,
        exit_code,
    })
}

/// 安装器产出的版本。
struct ProducedVersion {
    /// 版本 ID（如 `1.20.1-OptiFine_HD_U_I6`）。
    id: String,
    /// `(文件名, 源路径)`。
    files: Vec<(String, PathBuf)>,
}

/// 在沙箱里找安装器产出的版本目录。
fn collect_produced(minecraft_dir: &Path, mc_version: &str) -> Option<ProducedVersion> {
    let versions_dir = minecraft_dir.join("versions");
    let entries = std::fs::read_dir(&versions_dir).ok()?;

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(id) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if id == mc_version {
            continue;
        }

        // 版本目录里应该有 `<id>.json` 与 `<id>.jar`
        let json = dir.join(format!("{id}.json"));
        if !json.is_file() {
            continue;
        }

        let mut files = vec![(format!("{id}.json"), json)];
        let jar = dir.join(format!("{id}.jar"));
        if jar.is_file() {
            files.push((format!("{id}.jar"), jar));
        }

        return Some(ProducedVersion {
            id: id.to_string(),
            files,
        });
    }

    None
}

/// 把 OptiFine 的库文件（`libraries/optifine/**`）复制到实例。
fn copy_optifine_libraries(from: &Path, to: &Path) -> Result<()> {
    let optifine_dir = from.join("optifine");
    if !optifine_dir.is_dir() {
        return Ok(());
    }
    copy_dir_recursive(&optifine_dir, &to.join("optifine"))
}

/// 递归复制目录。
fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    crate::utils::paths::ensure_dir(to)?;
    let entries = std::fs::read_dir(from).map_err(|e| LauncherError::Io {
        message: format!("读取目录失败 {}：{e}", from.display()),
    })?;

    for entry in entries.flatten() {
        let source = entry.path();
        let Some(name) = source.file_name() else {
            continue;
        };
        let target = to.join(name);

        if source.is_dir() {
            copy_dir_recursive(&source, &target)?;
            continue;
        }
        if !source.is_file() {
            continue;
        }
        let _ = std::fs::copy(&source, &target);
    }

    Ok(())
}

/// 取输出文本的最后几行。
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(12);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_download_url_from_edition() {
        let version = OptiFineVersion {
            label: "1.20.1 HD_U I6".into(),
            mc_version: "1.20.1".into(),
            kind: "HD_U".into(),
            patch: "I6".into(),
            edition: "HD_U_I6".into(),
            file_name: "OptiFine_1.20.1_HD_U_I6.jar".into(),
        };
        assert_eq!(
            download_url(&version).unwrap(),
            "https://bmclapi2.bangbang93.com/optifine/1.20.1/HD_U/I6"
        );
    }

    #[test]
    fn rejects_malformed_edition() {
        let version = OptiFineVersion {
            label: "x".into(),
            mc_version: "1.20.1".into(),
            kind: String::new(),
            patch: String::new(),
            edition: "HDU".into(),
            file_name: "x.jar".into(),
        };
        assert!(download_url(&version).is_err());
    }

    /// 用真实的 class 文件头验证「major - 44 = Java 主版本」。
    #[test]
    fn reads_required_java_from_class_header() {
        let dir = std::env::temp_dir().join(format!("sereno-of-class-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let jar = dir.join("installer.jar");
        {
            use std::io::Write;
            use zip::write::SimpleFileOptions;
            let file = std::fs::File::create(&jar).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            writer
                .start_file("optifine/Installer.class", options)
                .unwrap();
            // class 头：magic + minor + major(52 = Java 8)
            let mut header = vec![0xCAu8, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34];
            header.extend_from_slice(&[0u8; 16]);
            writer.write_all(&header).unwrap();
            writer.finish().unwrap();
        }

        assert_eq!(required_java_major(&jar).unwrap(), Some(8));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_installer_class_is_an_error() {
        let dir = std::env::temp_dir().join(format!("sereno-of-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let jar = dir.join("installer.jar");
        {
            use std::io::Write;
            use zip::write::SimpleFileOptions;
            let file = std::fs::File::create(&jar).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }

        assert!(required_java_major(&jar).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 沙箱里必须有一份合法的 `launcher_profiles.json`，
    /// 否则安装器会弹 Swing 模态框并且永不退出。
    #[test]
    fn writes_launcher_profiles_for_sandbox() {
        let dir = std::env::temp_dir().join(format!("sereno-of-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        write_launcher_profiles(&dir).expect("必须写出 launcher_profiles.json");

        let path = dir.join("launcher_profiles.json");
        assert!(path.is_file(), "launcher_profiles.json 必须落盘");

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

        // 安装器要读这三个字段，缺一个就进错误分支
        assert!(value.get("profiles").is_some(), "缺少 profiles");
        assert_eq!(value.get("selectedProfile").unwrap(), "Sereno");
        assert!(value.get("clientToken").is_some(), "缺少 clientToken");

        // profiles 必须是个非空对象
        let profiles = value.get("profiles").unwrap().as_object().unwrap();
        assert!(!profiles.is_empty(), "profiles 不能为空");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 沙箱里已有原版目录时，产出目录应当是**新**的那个。
    #[test]
    fn finds_produced_version_directory() {
        let dir = std::env::temp_dir().join(format!("sereno-of-prod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let versions = dir.join("versions");
        std::fs::create_dir_all(versions.join("1.20.1")).unwrap();
        std::fs::create_dir_all(versions.join("1.20.1-OptiFine_HD_U_I6")).unwrap();
        std::fs::write(versions.join("1.20.1").join("1.20.1.json"), "{}").unwrap();
        std::fs::write(
            versions
                .join("1.20.1-OptiFine_HD_U_I6")
                .join("1.20.1-OptiFine_HD_U_I6.json"),
            "{}",
        )
        .unwrap();
        std::fs::write(
            versions
                .join("1.20.1-OptiFine_HD_U_I6")
                .join("1.20.1-OptiFine_HD_U_I6.jar"),
            "jar",
        )
        .unwrap();

        let produced = collect_produced(&dir, "1.20.1").expect("必须找到产出版本");
        assert_eq!(produced.id, "1.20.1-OptiFine_HD_U_I6");
        assert_eq!(produced.files.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
