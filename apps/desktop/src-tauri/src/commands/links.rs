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
        "sereno" => "https://ifdian.net/a/Sereno",
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
