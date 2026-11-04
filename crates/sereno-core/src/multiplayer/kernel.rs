//! EasyTier 内核管理：分发、校验、定位。
//!
//! 为什么用**官方预编译内核 + 子进程托管**而不是库内嵌：
//!
//! 1. 社区库（`qomicex-connector` 等）的核心联机模块并未公开，无法作为依赖；
//!    crates.io 上的 `easytier` 停在 2.0.3（约两年前），与现网 2.4+ 的组网
//!    协议兼容性没有保障。
//! 2. 子进程 + `--no-tun`（smoltcp 用户态栈）+ `--port-forward` 可以让
//!    **房客全程零权限**；房主的 TUN 模式只在开房时请求一次 UAC。
//!    参数语义全部经官方文档与源码核实（见 `session.rs`）。
//!
//! ## 分发策略（三级回退）
//!
//! ① 缓存目录已有内核 → 直接用；
//! ② **应用内置文件**（随安装包分发，见下方 `BUNDLED_FILES`）→ 逐文件
//!    校验 SHA-256 后复制到缓存目录；
//! ③ 在线下载官方 zip（官方 + 镜像回退）→ 校验 zip 摘要后安全解压。
//!
//! 三条路径共用同一套硬编码哈希锚点——内置文件只是省去首次下载，
//! 供应链防线不因来源放松。EasyTier 以子进程 + TUN 权限运行，
//! 被替换的内核等于一次本机提权。
//!
//! **为什么内置的是散文件而不是 zip**：Windows 内核依赖同目录的
//! `wintun.dll` / `Packet.dll` / `WinDivert64.sys`，缺任何一个房主
//! TUN 模式都会失败；散文件方案让安装包里就是可直接运行的一套。
//! Web 面板（`easytier-web*.exe`，60MB+）与 Sereno 无关，不打包。
//!
//! **升级内核**：换新版本文件后同步更新 `KERNEL_VERSION`、
//! `BUNDLED_FILES`（新文件哈希）与 `KERNEL_SHA256`（zip 摘要，
//! 取自 GitHub Releases API 的 `digest` 字段），并同步
//! `scripts/fetch-easytier.mjs`。

use std::path::{Path, PathBuf};

use crate::utils::{archive, fs, http, paths};
use crate::{LauncherError, Result};

/// 当前跟随的 EasyTier 内核版本。
///
/// 选 2.7.0（当前最新稳定 release）而非 nightly：与 HMCL / PCL CE 社区
/// 实际分发的内核版本对齐，最大化组网互通性。
pub const KERNEL_VERSION: &str = "2.7.0";

/// 随安装包分发的内核文件清单（来自官方 release，SHA-256 逐文件锚定）。
///
/// - `easytier-core.exe`：核心主程序（Rust 侧启动并传参的对象）
/// - `wintun.dll`：Wintun 虚拟网卡驱动——**房主 TUN 模式的硬依赖**，
///   Windows 加载 DLL 时默认在 exe 同目录查找，必须与 exe 同文件夹
/// - `Packet.dll` / `WinDivert64.sys`：底层抓包 / 流量拦截依赖
/// - `easytier-cli.exe`：官方 CLI——M2 的「直连/中继徽章」会用它查
///   实时组网状态，体积不大随包带上
///
/// 不打包 `easytier-web*.exe`（官方 Web 面板，60MB+，Sereno 有自己的 UI）。
const BUNDLED_FILES: &[(&str, &str)] = &[
    (
        "easytier-core.exe",
        "6490c6f5b5b809065517bb3324ca34c0f1f367f81b7fda04a6352391508b8ff8",
    ),
    (
        "wintun.dll",
        "e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce",
    ),
    (
        "Packet.dll",
        "c7c03a87eac7243ccbe331554624b18803010b740e311fc8cfddb573096eacac",
    ),
    (
        "WinDivert64.sys",
        "8da085332782708d8767bcace5327a6ec7283c17cfb85e40b03cd2323a90ddc2",
    ),
    (
        "easytier-cli.exe",
        "3afdf2e4306c3f8d65408ddd230c7e4118ffdb5d550921022aed9966693c82ba",
    ),
];

/// 各平台 release zip 的**官方 SHA-256**（在线下载回退路径的锚点）。
///
/// 来源：GitHub Releases API 的 `digest` 字段（官方上传时由 GitHub 计算）。
/// 升级内核版本时必须同步更新这张表。
const KERNEL_SHA256: &[(&str, &str)] = &[
    (
        "easytier-windows-x86_64",
        "790fd9be347b00753570ac2b65f874a591ede9427603fe34e74a86b2b084847f",
    ),
    (
        "easytier-linux-x86_64",
        "442238912065537c280869089217303e3d2813e9c5d2a6b14deaf25667065aa2",
    ),
    (
        "easytier-macos-x86_64",
        "4a52b977eba3c29fdf392748318992be766dbab1d144d7f5cb69bfea572e237b",
    ),
    (
        "easytier-macos-aarch64",
        "ff0ad04fd6fcc462021a3b3161ed82270dd9e7927221ecb22009f0df697bdf52",
    ),
];

/// 公共共享节点：双方都连它以完成首次握手与 NAT 打洞；
/// 打洞成功后走 P2P，失败时由它中继（EasyTier 自动切换，用户无感）。
pub const DEFAULT_PUBLIC_SERVERS: &[&str] = &["tcp://public.easytier.cn:11010"];

/// 房主默认的 MC 端口（专用服务端）。
///
/// 单人世界「对局域网开放」的端口是**随机的**，房主可在 UI 里
/// 用「自动检测」抓取；只有专用服务端才是 25565。
pub const DEFAULT_MC_PORT: u16 = 25565;

/// 内核二进制名（按平台）。
fn kernel_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "easytier-core.exe"
    } else {
        "easytier-core"
    }
}

/// 当前平台对应的 release 资产名（仅 x86_64 / arm64 主流平台）。
fn release_asset() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "easytier-windows-x86_64",
        ("linux", "x86_64") => "easytier-linux-x86_64",
        ("macos", "x86_64") => "easytier-macos-x86_64",
        ("macos", "aarch64") => "easytier-macos-aarch64",
        (os, arch) => {
            return Err(LauncherError::Internal {
                message: format!("联机模块暂不支持该平台: {os}/{arch}"),
            });
        }
    })
}

/// 内核安装目录：`<cache>/easytier/<version>/`。
fn kernel_dir() -> Result<PathBuf> {
    Ok(paths::cache_dir()?.join("easytier").join(KERNEL_VERSION))
}

/// 内核二进制的期望路径。
fn kernel_binary_path() -> Result<PathBuf> {
    Ok(kernel_dir()?.join(kernel_binary_name()))
}

/// 当前平台 release zip 的官方 SHA-256（小写十六进制）。
fn expected_zip_sha256(asset: &str) -> Result<&'static str> {
    KERNEL_SHA256
        .iter()
        .find(|(name, _)| *name == asset)
        .map(|(_, hash)| *hash)
        .ok_or_else(|| LauncherError::Internal {
            message: format!("内核资产缺少官方校验和记录: {asset}"),
        })
}

/// 确保内核就绪，返回二进制路径。
pub async fn ensure_kernel(client: &reqwest::Client) -> Result<PathBuf> {
    ensure_kernel_with_bundled(client, None).await
}

/// 同 [`ensure_kernel`]，但可传入随安装包分发的内核目录
/// （Tauri 资源目录下的 `easytier/`，含 `BUNDLED_FILES` 列出的散文件）。
///
/// 内置路径核验通过后把文件**复制到缓存目录**再运行——
/// 安装目录在 Windows 上可能没有写权限，统一运行位置也简化排查。
pub async fn ensure_kernel_with_bundled(
    client: &reqwest::Client,
    bundled_dir: Option<&Path>,
) -> Result<PathBuf> {
    let expected = kernel_binary_path()?;
    if expected.is_file() {
        return Ok(expected);
    }

    // ① 内置文件：安装包里带了内核（构建期已校验，这里逐文件再验）
    if let Some(dir) = bundled_dir
        && dir.join(kernel_binary_name()).is_file()
    {
        match install_bundled(dir).await {
            Ok(()) => {
                tracing::info!("使用应用内置的 EasyTier 内核");
                return Ok(expected);
            }
            Err(error) => {
                tracing::warn!(%error, "内置内核校验失败，回退到在线下载");
            }
        }
    }

    let asset = release_asset()?;
    let official = format!(
        "https://github.com/EasyTier/EasyTier/releases/download/v{KERNEL_VERSION}/{asset}-v{KERNEL_VERSION}.zip"
    );

    let sources: Vec<String> = vec![
        official.clone(),
        format!("https://ghfast.top/{official}"),
        format!("https://gh-proxy.com/{official}"),
    ];

    let mut last_error = String::new();
    for url in &sources {
        tracing::info!(url, "下载 EasyTier 内核");
        match download_and_extract(client, url, &expected).await {
            Ok(()) => return Ok(expected),
            Err(error) => {
                tracing::warn!(url, %error, "内核下载源失败，尝试下一源");
                last_error = error.to_string();
            }
        }
    }

    Err(LauncherError::AllSourcesUnavailable {
        message: format!("EasyTier 内核下载失败: {last_error}"),
    })
}

/// 从任意目录安装内核文件（逐文件 SHA-256 校验后复制到缓存目录）。
///
/// 公开给集成测试使用：篡改防御测试需要一个只测「校验+落盘」、
/// 不带在线下载回退的入口。
pub async fn install_bundled(dir: &Path) -> Result<()> {
    let expected = kernel_binary_path()?;
    install_bundled_inner(dir, &expected).await
}

/// 把内置目录的文件逐个校验并复制到缓存目录。
async fn install_bundled_inner(dir: &Path, expected: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};

    let target_dir = kernel_dir()?;
    paths::ensure_dir(&target_dir)?;

    for (name, hash) in BUNDLED_FILES {
        let source = dir.join(name);
        let bytes = std::fs::read(&source).map_err(|e| LauncherError::Io {
            message: format!("读取内置内核文件失败 {name}: {e}"),
        })?;
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(hash) {
            return Err(LauncherError::ChecksumMismatch {
                path: format!("easytier/{name}"),
                expected: (*hash).to_string(),
                actual,
            });
        }
        fs::atomic_write(&target_dir.join(name), &bytes)?;
    }

    debug_assert!(expected.is_file());
    Ok(())
}

/// 下载 zip 到临时文件并安全解压，成功后保证 `expected` 存在。
async fn download_and_extract(client: &reqwest::Client, url: &str, expected: &Path) -> Result<()> {
    let dir = kernel_dir()?;
    paths::ensure_dir(&dir)?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("请求失败: {e}"),
            url: url.to_string(),
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("HTTP {status}"),
            url: url.to_string(),
        });
    }

    // 内核 zip 约 30–40 MB，上限给到 128 MB 防异常
    let bytes = http::read_limited(resp, 128 * 1024 * 1024).await?;
    extract_verified_zip(&bytes, expected).await
}

/// 校验 zip 字节匹配硬编码官方摘要后安全解压到版本目录。
///
/// **供应链防线**：无论来自官方、镜像还是安装包内置，字节必须匹配
/// 硬编码摘要。EasyTier 以子进程 + TUN 权限运行，被替换的内核
/// 就是一次本机提权。
async fn extract_verified_zip(bytes: &[u8], expected: &Path) -> Result<()> {
    let asset = release_asset()?;
    {
        use sha2::{Digest, Sha256};
        let actual = format!("{:x}", Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(expected_zip_sha256(asset)?) {
            return Err(LauncherError::ChecksumMismatch {
                path: format!("easytier/{asset}"),
                expected: expected_zip_sha256(asset)?.to_string(),
                actual,
            });
        }
    }

    let dir = kernel_dir()?;
    paths::ensure_dir(&dir)?;
    let zip_path = dir.join("kernel.zip");
    fs::atomic_write(&zip_path, bytes)?;

    let file = std::fs::File::open(&zip_path).map_err(|e| LauncherError::Io {
        message: format!("打开内核压缩包失败: {e}"),
    })?;
    // 解压目标是版本目录本身；extract_zip_safely 内部已做路径穿越 /
    // 保留名 / 符号链接防护。解压前清空目录，避免半新半旧。
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
        paths::ensure_dir(&dir)?;
    }
    archive::extract_zip_safely(file, &dir)?;
    let _ = std::fs::remove_file(&zip_path);

    // release zip 内核可能不在根目录，递归找
    let found = find_binary(&dir, kernel_binary_name())?;
    if found != expected {
        std::fs::rename(&found, expected).map_err(|e| LauncherError::Io {
            message: format!("整理内核文件失败: {e}"),
        })?;
    }
    Ok(())
}

/// 在目录中递归查找内核二进制（深度有限，防异常结构）。
fn find_binary(dir: &Path, name: &str) -> Result<PathBuf> {
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > 4 {
            continue;
        }
        let entries = std::fs::read_dir(&current).map_err(|e| LauncherError::Io {
            message: format!("读取内核目录失败: {e}"),
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if path.file_name().is_some_and(|n| n == name) {
                return Ok(path);
            }
        }
    }
    Err(LauncherError::Internal {
        message: format!("内核压缩包中未找到 {name}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_matches_platform() {
        // 至少当前平台能解析出资产名
        assert!(release_asset().is_ok());
    }

    #[test]
    fn public_servers_nonempty() {
        assert!(!DEFAULT_PUBLIC_SERVERS.is_empty());
        assert!(DEFAULT_PUBLIC_SERVERS[0].starts_with("tcp://"));
    }

    #[test]
    fn bundled_files_cover_runtime_dependencies() {
        // 核心程序 + TUN 驱动 + 底层依赖 + CLI，缺一不可
        let names: Vec<&str> = BUNDLED_FILES.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"easytier-core.exe"));
        assert!(names.contains(&"wintun.dll"), "房主 TUN 模式的硬依赖");
        assert!(names.contains(&"Packet.dll"));
        assert!(names.contains(&"WinDivert64.sys"));
        assert!(names.contains(&"easytier-cli.exe"));
        // Web 面板绝不随包
        assert!(!names.iter().any(|n| n.contains("web")));
        // 所有哈希都是 64 位十六进制
        for (_, hash) in BUNDLED_FILES {
            assert_eq!(hash.len(), 64);
            assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn zip_hashes_are_recorded_for_all_platforms() {
        assert_eq!(KERNEL_SHA256.len(), 4);
        for (_, hash) in KERNEL_SHA256 {
            assert_eq!(hash.len(), 64);
        }
    }
}
