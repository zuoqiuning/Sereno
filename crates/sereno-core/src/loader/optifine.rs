//! OptiFine 安装。
//!
//! OptiFine 的安装器没有可靠的命令行无头入口（直接 `java -jar` 会弹 Swing 窗口
//! 并一直阻塞），PCL 的做法是调用安装器内部的 `optifine.Installer` 类。
//! 在移植该方案之前，这里保持明确的「未实现」错误，避免误触发 GUI 卡死。

use std::path::Path;

use crate::models::instance::LoaderInfo;
use crate::{LauncherError, Result};

pub async fn install_optifine(
    _client: &reqwest::Client,
    _instance_dir: &Path,
    mc_version: &str,
    _java_exe: &str,
    _specified_version: Option<&str>,
) -> Result<LoaderInfo> {
    Err(LauncherError::LoaderInstallFailed {
        message: format!(
            "OptiFine 暂不支持一键安装（MC {mc_version}）。\
             请到 https://optifine.net/downloads 下载安装器，\
             用同版本 Java 以客户端模式手动安装后，在启动器里选择它生成的版本；\
             或改用 Fabric/Quilt + Iris（https://irisshaders.dev）获得同样的光影与性能优化。"
        ),
    })
}
