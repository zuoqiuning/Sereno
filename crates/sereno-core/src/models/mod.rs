//! 数据结构：跨 core / commands / 前端共享。
//!
//! **约定**：
//! - 本模块**只定义数据，不含行为**。
//! - 所有结构体派生 `Serialize`、`Deserialize`、`specta::Type`，
//!   并统一加 `#[serde(rename_all = "camelCase")]`。
//! - 前端不允许手写同名类型；缺类型就回去补 Rust 的 `specta::Type`。

pub mod account;
pub mod backup;
pub mod crash;
pub mod download;
pub mod health;
pub mod instance;
pub mod java;
pub mod modpack;
pub mod resource;
pub mod settings;
pub mod storage;
pub mod version;
