//! 全局事件：事件名、载荷类型、唯一注册清单。
//!
//! - **全局性、低频的状态变化走 Event**（`AppHandle::emit()` 广播，前端 `listen()` 订阅）。
//! - **逐会话的高频进度走 Channel**，见 `ipc::progress`。
//!
//! 事件名统一 `sereno://` 前缀，不要用泛化的 `launcher://`，
//! 避免与插件或其他启动器冲突。

use serde::{Deserialize, Serialize};

/// 应用就绪事件：前端首帧渲染完成后可监听。
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

/// 事件名前缀常量。
pub const EVENT_PREFIX: &str = "sereno://";

/// 拼接完整事件名。
pub fn event_name(local: &str) -> String {
    format!("{EVENT_PREFIX}{local}")
}
