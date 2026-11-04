//! 全局事件：事件名、载荷类型、唯一注册清单。
//!
//! - **全局性、低频的状态变化走 Event**（`AppHandle::emit()` 广播，前端 `listen()` 订阅）。
//! - **逐会话的高频进度走 Channel**，见 `ipc::progress`。
//!
//! 事件名由 `tauri_specta::Event` 派生按**结构体的 kebab-case** 生成
//! （`GameExited` → `game-exited`）。曾经规划过 `sereno://` 前缀，
//! 但本项目只有一个窗口、也没有与插件共用事件名的场景，加前缀只会让
//! 「Rust 里 emit 的名字」与「前端 listen 的名字」多一层手工同步，
//! 因此**统一不加前缀**，以派生结果为准。

use serde::{Deserialize, Serialize};

/// 应用就绪事件：主窗口已经显示、启动画面已关闭。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AppReady {
    /// 启动器版本。
    pub version: String,
    /// 状态根目录（已脱敏）。
    pub state_dir: String,
}

/// 游戏进程退出：前端据此刷新实例列表与运行状态。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct GameExited {
    pub instance_id: String,
    pub exit_code: Option<i32>,
}

/// 更新下载进度：前端据此画进度条。
///
/// **用事件而不是 Channel**：Windows 上安装器启动后进程会立刻退出，
/// Channel 的发送端随命令一起被 drop，最后一个进度事件可能送不到。
/// 事件是广播式的，不受影响。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    /// 已下载字节数。
    ///
    /// 映射成 JS `number`（而不是 BigInt）：下载进度只用于画进度条，
    /// 字节数超过 2^53 的场景不存在（PB 级）。
    #[specta(type = specta_typescript::Number)]
    pub downloaded: u64,
    /// 总字节数（服务端未提供时为 0）。
    #[specta(type = specta_typescript::Number)]
    pub total: u64,
    /// 0–100 的百分比（总量未知时为 0）。
    pub percent: u8,
}

/// 检查到新版本：前端据此弹出全局更新卡片。
///
/// 在**启动画面阶段**由后台任务发出——检测不等结果，
/// 界面照常显示，结果回来再提示。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAvailable {
    /// 新版本号。
    pub latest_version: String,
    /// 当前版本号。
    pub current_version: String,
    /// 更新说明（可能为空）。
    pub notes: String,
}

/// 事件注册清单：**全项目唯一一份**。
macro_rules! all_events {
    () => {
        tauri_specta::collect_events![
            crate::ipc::events::AppReady,
            crate::ipc::events::GameExited,
            crate::ipc::events::UpdateProgress,
            crate::ipc::events::UpdateAvailable
        ]
    };
}
pub(crate) use all_events;
