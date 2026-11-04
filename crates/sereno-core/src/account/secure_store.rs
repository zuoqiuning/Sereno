//! 系统凭据存储的抽象。
//!
//! **为什么先定义 trait 而不是直接用 `keyring`**：
//! 微软登录是账户域的第一件事。如果 `microsoft.rs` 一上来就调 `keyring`，
//! 后面想加降级策略（系统密钥库不可用时退回内存）就得改调用方。
//! 先定接口，业务代码依赖**接口**而不是具体实现，替换实现时零改动。
//!
//! 实现：
//! - [`MemoryStore`]：进程内存，仅用于测试与「系统密钥库不可用」的降级；
//! - `KeyringStore`：接 `keyring` crate（计划接入，见《技术栈选型》）。
//!
//! **安全约束**：本模块是**唯一允许接触系统凭据**的地方。
//! 凭据值绝不能进日志、诊断包或 `settings.json`。

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use crate::{LauncherError, Result};

/// 系统密钥库实现（Windows Credential Manager）。
///
/// 优点：凭据不进磁盘明文，进程退出后仍保留，卸载启动器时随凭据管理器清理。
///
/// 降级策略：系统密钥库不可用（极少数精简版 Windows）时，
/// [`default_store`] 会退回 [`MemoryStore`]，此时登录仍然可用，
/// 但**重启后需要重新登录**——调用方必须在界面上明确提示这一点。
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// 用给定的服务名创建（服务名决定凭据管理器里的分组）。
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, key).map_err(|e| {
            LauncherError::CredentialStoreUnavailable {
                message: format!("无法打开系统密钥库: {e}"),
            }
        })
    }
}

impl SecureStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            // 没存过不是错误，是「未登录」
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(LauncherError::CredentialStoreUnavailable {
                message: format!("读取系统密钥库失败: {e}"),
            }),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?.set_password(value).map_err(|e| {
            LauncherError::CredentialStoreUnavailable {
                message: format!("写入系统密钥库失败: {e}"),
            }
        })
    }

    fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(LauncherError::CredentialStoreUnavailable {
                message: format!("删除系统密钥库条目失败: {e}"),
            }),
        }
    }
}

/// 全进程共用的凭据存储。
///
/// 优先系统密钥库；不可用时退回内存，并由 [`is_ephemeral`] 告知调用方。
struct StoreHolder {
    store: Arc<dyn SecureStore>,
    ephemeral: bool,
}

static HOLDER: LazyLock<StoreHolder> = LazyLock::new(|| {
    let keyring = KeyringStore::new("SerenoLauncher");
    match probe_keyring(&keyring) {
        Ok(()) => StoreHolder {
            store: Arc::new(keyring),
            ephemeral: false,
        },
        Err(error) => {
            tracing::warn!(%error, "系统密钥库不可用，本次登录将只保留在内存中");
            StoreHolder {
                store: Arc::new(MemoryStore::new()),
                ephemeral: true,
            }
        }
    }
});

/// 探测系统密钥库是否可读写。
fn probe_keyring(store: &KeyringStore) -> Result<()> {
    const PROBE_KEY: &str = "sereno.probe";
    store.set(PROBE_KEY, "ok")?;
    store.delete(PROBE_KEY)
}

/// 取全进程共享的凭据存储。
pub fn default_store() -> Arc<dyn SecureStore> {
    Arc::clone(&HOLDER.store)
}

/// 当前凭据是否只存在于内存（密钥库不可用）。
///
/// 界面上必须据此提示「登录状态不会在重启后保留」。
pub fn is_ephemeral() -> bool {
    HOLDER.ephemeral
}

/// 凭据存储接口。
///
/// 所有方法都返回结构化错误；**不存在"静默失败"**——
/// 调用方必须明确知道凭据是读到了还是没读到。
pub trait SecureStore: Send + Sync {
    /// 读取一个凭据。不存在返回 `Ok(None)`。
    fn get(&self, key: &str) -> Result<Option<String>>;

    /// 写入一个凭据（覆盖已有值）。
    fn set(&self, key: &str, value: &str) -> Result<()>;

    /// 删除一个凭据。键不存在也算成功（幂等）。
    fn delete(&self, key: &str) -> Result<()>;
}

/// 内存实现：进程退出即丢失。
///
/// 两个用途：
/// 1. 单元测试里替换系统密钥库，避免污染真实凭据库；
/// 2. 系统密钥库不可用时的降级路径（此时必须明确提示用户「本次登录不会保留」）。
#[derive(Debug, Default)]
pub struct MemoryStore {
    entries: RwLock<HashMap<String, String>>,
}

impl MemoryStore {
    /// 新建一个空的内存存储。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前条目数（测试与诊断用）。
    pub fn len(&self) -> usize {
        self.entries.read().map(|m| m.len()).unwrap_or(0)
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 清空全部条目。
    pub fn clear(&self) {
        if let Ok(mut guard) = self.entries.write() {
            guard.clear();
        }
    }
}

impl SecureStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let guard = self.entries.read().map_err(|_| LauncherError::Internal {
            message: "凭据存储锁被 poisoning".into(),
        })?;
        Ok(guard.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let mut guard = self.entries.write().map_err(|_| LauncherError::Internal {
            message: "凭据存储锁被 poisoning".into(),
        })?;
        guard.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        let mut guard = self.entries.write().map_err(|_| LauncherError::Internal {
            message: "凭据存储锁被 poisoning".into(),
        })?;
        guard.remove(key);
        Ok(())
    }
}

/// 凭据键名常量。
///
/// 集中定义，避免各处硬编码字符串导致拼错。
pub mod keys {
    /// 微软账号 Refresh Token。
    pub const MICROSOFT_REFRESH_TOKEN: &str = "sereno.microsoft.refresh_token";
    /// 微软账号 Access Token。
    pub const MICROSOFT_ACCESS_TOKEN: &str = "sereno.microsoft.access_token";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_back() {
        let store = MemoryStore::new();
        assert!(store.is_empty());
        assert_eq!(store.get("k").unwrap(), None);

        store.set("k", "v").unwrap();
        assert_eq!(store.get("k").unwrap(), Some("v".to_string()));
        assert_eq!(store.len(), 1);

        store.set("k", "v2").unwrap();
        assert_eq!(store.get("k").unwrap(), Some("v2".to_string()));

        store.delete("k").unwrap();
        assert_eq!(store.get("k").unwrap(), None);
    }

    #[test]
    fn delete_is_idempotent() {
        let store = MemoryStore::new();
        store.delete("missing").unwrap();
        assert_eq!(store.get("missing").unwrap(), None);
    }

    #[test]
    fn works_through_the_trait_object() {
        let store: Box<dyn SecureStore> = Box::new(MemoryStore::new());
        store.set("a", "1").unwrap();
        assert_eq!(store.get("a").unwrap(), Some("1".to_string()));
    }
}
