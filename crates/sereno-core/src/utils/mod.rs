//! 通用工具。
//!
//! **约定**：本模块不允许出现「下载」「启动」这类业务词。
//! 一旦出现，说明该函数应该被挪进对应的业务域目录。

pub mod archive;
pub mod disk_cache;
pub mod fs;
pub mod http;
pub mod paths;
pub mod redact;
pub mod ttl_cache;
