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

/// 事件注册清单：**全项目唯一一份**。
macro_rules! all_events {
    () => {
        tauri_specta::collect_events![crate::ipc::events::AppReady, crate::ipc::events::GameExited]
    };
}
pub(crate) use all_events;
