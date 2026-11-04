//! 内置内核分发路径的集成测试。
//!
//! 覆盖实测清单的第 1 项（随包分发）与第 5 项（篡改防御）的可自动化部分：
//! - 内置目录哈希全部匹配 → 文件被复制到缓存目录，二进制可运行；
//! - 任一文件被篡改 → 拒绝执行（ChecksumMismatch），坏文件不落盘。
//!
//! 资源目录来自仓库内的 `apps/desktop/src-tauri/resources/easytier/`
//! （CI 的 release 流程会在测试前跑 `fetch-easytier.mjs` 生成）；
//! 目录不存在时跳过（如未跑 fetch 的开发机）。
//!
//! 注意：两个阶段共享真实缓存目录 `<cache>/easytier/<version>/`
//! （可再生数据），因此合并在一个测试里**串行**执行，结束时清理。

use std::path::{Path, PathBuf};

const RESOURCES_DIR: &str = "../../apps/desktop/src-tauri/resources/easytier";

fn resources_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(RESOURCES_DIR);
    dir.join("easytier-core.exe").is_file().then_some(dir)
}

fn cache_kernel_dir() -> PathBuf {
    sereno_core::utils::paths::cache_dir()
        .expect("缓存目录可用")
        .join("easytier")
        .join(sereno_core::multiplayer::kernel::KERNEL_VERSION)
}

fn clean_cache() {
    let _ = std::fs::remove_dir_all(cache_kernel_dir());
}

#[tokio::test]
async fn bundled_kernel_lifecycle() {
    let Some(dir) = resources_dir() else {
        eprintln!("跳过：resources/easytier 下没有内核（先跑 fetch-easytier.mjs）");
        return;
    };

    // ---- 阶段 1：正确安装 + 二进制可运行（实测清单第 1 项的机制等价）----
    clean_cache();

    let installed = match sereno_core::multiplayer::kernel::install_bundled(&dir).await {
        Ok(()) => true,
        // 环境守卫：本测试操作全局缓存，若安装版正在运行联机内核，
        // 缓存文件被锁（os error 5 / 拒绝访问）——跳过而不是误报失败
        Err(error)
            if error.to_string().contains("拒绝访问")
                || error.to_string().contains("os error 5")
                || error.to_string().contains("being used") =>
        {
            eprintln!("跳过：缓存目录被占用（安装版可能正在运行联机内核）: {error}");
            return;
        }
        Err(error) => panic!("内置内核应校验通过并安装: {error}"),
    };
    if !installed {
        return;
    }

    let path = cache_kernel_dir().join("easytier-core.exe");
    assert!(path.is_file(), "内核二进制应就位: {}", path.display());
    // 五件套必须全部落盘（wintun.dll 是房主 TUN 的硬依赖）
    for name in [
        "wintun.dll",
        "Packet.dll",
        "WinDivert64.sys",
        "easytier-cli.exe",
    ] {
        assert!(
            cache_kernel_dir().join(name).is_file(),
            "{name} 应随内核一起安装"
        );
    }

    // 直接运行 `--version`：证明二进制与同目录 DLL 依赖完整（不联网也能跑）
    let output = std::process::Command::new(&path)
        .arg("--version")
        .output()
        .expect("运行内核 --version");
    assert!(output.status.success());
    let version = String::from_utf8_lossy(&output.stdout);
    assert!(version.contains("easytier-core"), "输出异常: {version}");

    // ---- 阶段 2：篡改防御（实测清单第 5 项的机制等价）----
    clean_cache();

    // 把内置目录复制到临时位置并篡改 wintun.dll（模拟安装包被投毒）
    let tampered = std::env::temp_dir().join("sereno-kernel-tamper-test");
    let _ = std::fs::remove_dir_all(&tampered);
    std::fs::create_dir_all(&tampered).unwrap();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let dest = tampered.join(entry.file_name());
        std::fs::copy(entry.path(), &dest).unwrap();
        if entry.file_name() == "wintun.dll" {
            let mut bytes = std::fs::read(&dest).unwrap();
            bytes.extend_from_slice(b"tampered-by-attacker");
            std::fs::write(&dest, &bytes).unwrap();
        }
    }

    // 直接测「校验+落盘」入口：篡改必须在此被拒绝
    // （ensure_kernel_with_bundled 拒绝后会回退在线下载，整体返回 Ok 是设计行为）
    let result = sereno_core::multiplayer::kernel::install_bundled(&tampered).await;
    assert!(result.is_err(), "被篡改的内核必须被拒绝");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("校验") || err.contains("Checksum") || err.contains("mismatch"),
        "错误应指向校验失败: {err}"
    );
    // 坏文件不能落盘
    assert!(
        !cache_kernel_dir().join("wintun.dll").is_file(),
        "被篡改的 wintun.dll 绝不能被复制到运行目录"
    );

    let _ = std::fs::remove_dir_all(&tampered);
    clean_cache();
}
