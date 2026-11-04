//! 外部链接：只允许白名单内的固定地址，前端不能传任意 URL。

use sereno_core::{LauncherError, Result};

const REPO: &str = "https://github.com/zuoqiuning/Sereno";
const ISSUES: &str = "https://github.com/zuoqiuning/Sereno/issues";

fn open_external(url: &str) -> Result<()> {
    opener::open(url).map_err(|e| LauncherError::Io {
        message: format!("打开链接失败: {e}"),
    })
}

/// 鸣谢页与关于页的赞助入口。
#[tauri::command]
#[specta::specta]
pub fn open_supporter_page(target: String) -> Result<()> {
    let url = match target.as_str() {
        "pcl" => "https://afdian.com/a/LTCat",
        "bmclapi" => "https://afdian.com/a/bangbang93",
        // MCIM 的赞助页：它提供 Modrinth / CurseForge 的国内镜像，
        // 是本项目能免 API Key 访问 CurseForge 的前提。
        "mcim" => sereno_core::resource::mcim::MCIM_SPONSOR_URL,
        "sereno" => "https://ifdian.net/a/Sereno",
        // EasyTier：联机模块的组网内核，官方预编译二进制的来源。
        // 鸣谢页给出其赞助入口，符合开源礼仪也方便用户支持上游。
        "easytier" => "https://www.easytier.cn/#sponsor",
        _ => {
            return Err(LauncherError::InvalidConfig {
                key: "target".into(),
                message: format!("未知的赞助目标: {target}"),
            });
        }
    };

    open_external(url)
}

/// 关于页：直达项目仓库。
#[tauri::command]
#[specta::specta]
pub fn open_project_repo() -> Result<()> {
    open_external(REPO)
}

/// 反馈页：直达 GitHub Issues。
#[tauri::command]
#[specta::specta]
pub fn open_project_issues() -> Result<()> {
    open_external(ISSUES)
}
