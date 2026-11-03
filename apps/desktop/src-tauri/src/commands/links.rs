//! 外部链接：只允许白名单内的固定地址，前端不能传任意 URL。

use sereno_core::{LauncherError, Result};

/// 鸣谢页的赞助入口。
#[tauri::command]
#[specta::specta]
pub fn open_supporter_page(target: String) -> Result<()> {
    let url = match target.as_str() {
        "pcl" => "https://afdian.com/a/LTCat",
        "bmclapi" => "https://afdian.com/a/bangbang93",
        _ => {
            return Err(LauncherError::InvalidConfig {
                key: "target".into(),
                message: format!("未知的赞助目标: {target}"),
            });
        }
    };

    opener::open(url).map_err(|e| LauncherError::Io {
        message: format!("打开赞助页失败: {e}"),
    })
}
