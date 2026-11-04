//! 版本域：版本清单、版本详情、规则求值与 inheritsFrom 合并。
//!
//! **边界**：本模块是纯 Rust 领域层，不引用 `tauri` / `tauri_specta`。
//! 网络访问统一走 [`crate::utils::http::build_client`] 造出来的 client，
//! 因此超时、重定向、UA、代理策略在版本域里也是统一的。

pub mod artifact;
pub mod detail;
pub mod inherits;
pub mod manifest;
pub mod rules;
pub mod update;

pub use detail::{fetch_detail, parse_detail, resolve_detail};
pub use inherits::{merge_inherits, merge_libraries, resolve_with, resolve_with_async};
pub use manifest::{VERSION_MANIFEST_URL, fetch_manifest, parse_manifest};
pub use rules::{OsContext, applies_to_client, evaluate_rules, filter_libraries, rule_matches};
