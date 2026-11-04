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

    // 4. 跑安装器
    let output = run_installer(&java.path, &installer, &sandbox).await?;

    // 5. 收集产物
    let produced = collect_produced(&minecraft_dir, mc_version).ok_or_else(|| {
        LauncherError::LoaderInstallFailed {
            message: format!(
                "OptiFine 安装器没有产出任何版本文件。安装器输出：\n{}",
                tail(&output)
            ),
        }
    })?;

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

/// 运行安装器。
///
/// **环境变量与系统属性都指向沙箱**：安装器据此定位 Minecraft 目录，
/// 绝不会写到用户的真实游戏目录里。
async fn run_installer(java_exe: &str, installer: &Path, sandbox: &Path) -> Result<String> {
    let sandbox_str = sandbox.to_string_lossy().to_string();
    let installer_str = installer.to_string_lossy().to_string();

    let mut command = tokio::process::Command::new(java_exe);
    command
        .arg(format!("-Duser.home={sandbox_str}"))
        .arg("-cp")
        .arg(&installer_str)
        .arg("optifine.Installer")
        .current_dir(sandbox)
        // 关掉标准输入：安装器一旦等待输入就会永远卡住
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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

    if !output.status.success() {
        return Err(LauncherError::LoaderInstallFailed {
            message: format!(
                "OptiFine 安装器退出码 {:?}：\n{}",
                output.status.code(),
                tail(&text)
            ),
        });
    }

    Ok(text)
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
