//! 文件持久化：JSON 读写、原子写、schemaVersion 迁移。
//!
//! **约定**：本模块**只负责 JSON 读写与原子落盘**，
//! 不含「实例该不该允许删除」这类业务判断——
//! 判断在 `instance::lifecycle`，`storage` 只执行写入。

pub mod cache;
pub mod instance;
pub mod settings;
pub mod usage;
