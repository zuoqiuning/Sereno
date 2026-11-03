//! IPC 适配层：事件与通道。
//!
//! **它是唯一允许出现 `Channel` 类型的地方**，core 不认识它。

pub mod events;
pub mod progress;
