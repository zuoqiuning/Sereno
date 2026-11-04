//! 联机模块：基于 EasyTier 虚拟网的「房间码」联机。
//!
//! ## 用户视角
//!
//! - **房主**：点「开房」→ 给一次 UAC → 拿到房间码发 给朋友 →
//!   游戏里「对局域网开放」→ 好友进世界。
//! - **房客**：粘贴房间码 → 点「加入」→ 游戏里「直接连接」
//!   `127.0.0.1:<端口>`（界面一键复制）。
//!
//! ## 模块边界
//!
//! - [`room_code`]：房间码格式 / 校验 / 凭证派生（纯函数，全量单测）
//! - [`kernel`]：EasyTier 预编译内核的下载与定位
//! - [`ping`]：MC 状态探测（「真正连上了」的判据）
//! - [`session`]：会话状态机与进程托管（Tauri `commands` 只做薄适配）
//!
//! 设计决策与参数语义的核实过程见各子模块文档注释。
pub mod detect;
pub mod kernel;
pub mod link;
pub mod ping;
pub mod room_code;
pub mod session;

pub use detect::DetectedWorld;
pub use ping::ServerStatus;
pub use session::{MultiplayerStatus, Phase, Role, SessionManager};
