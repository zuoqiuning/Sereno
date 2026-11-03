//! Sereno 领域层（`sereno-core`）。
//!
//! **边界约束**：本 crate 的依赖图里**没有 `tauri`，也没有 `tauri-specta`**。
//! 下载器、版本解析、启动参数构建、Java 管理、加载器安装、实例生命周期
//! 全部在这里，可以 `cargo test -p sereno-core` 独立测试，不链接 WebView。
//!
//! 上层依赖方向是单向的：
//!
//! ```text
//! Vue → composables → api → bindings.ts → invoke → commands → sereno-core
//! ```
//!
//! 反过来不成立：`sereno-core` 不认识 `commands`，`commands` 不认识 Vue。

pub mod account;
pub mod backup;
pub mod crash;
pub mod download;
pub mod error;
pub mod health;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loader;
pub mod models;
pub mod modpack;
pub mod resource;
pub mod storage;
pub mod utils;
pub mod version;

pub use error::{LauncherError, Result};

/// 产品版本号的唯一来源。
///
/// 根目录 `version.json` 由 `crates/sereno-core/build.rs` 读入并注入，
/// 运行时通过 `env!` 读取。UI、安装包、Cargo、package.json 全部由它派生，
/// 任何地方都不允许再维护第二份版本号。
pub const APP_VERSION: &str = env!("SERENO_APP_VERSION");
