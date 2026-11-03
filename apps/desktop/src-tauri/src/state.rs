//! 跨命令共享的应用状态。

use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use sereno_core::models::settings::AppSettings;
use sereno_core::models::version::VersionManifest;
use sereno_core::{LauncherError, Result};

/// 版本清单缓存：10 分钟 TTL。
///
/// 每次进下载页都会调 `get_version_manifest`；
/// 没缓存就要重新走网络，国内网络下相当于每次多等 1-3 秒。
pub struct ManifestCache {
    inner: RwLock<Option<(Instant, VersionManifest)>>,
}

impl Default for ManifestCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ManifestCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    pub fn get(&self) -> Option<VersionManifest> {
        let guard = self.inner.read().ok()?;
        let (t, m) = guard.as_ref()?;
        if t.elapsed() < Duration::from_secs(600) {
            Some(m.clone())
        } else {
            None
        }
    }

    pub fn set(&self, manifest: VersionManifest) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = Some((Instant::now(), manifest));
        }
    }
}

pub struct AppState {
    pub settings: RwLock<AppSettings>,
    pub manifest_cache: ManifestCache,
    pub started_at: Instant,
    /// 正在运行的实例 ID 集合。只存在于内存，进程退出即消失。
    running: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    pub fn new() -> Result<Self> {
        sereno_core::utils::paths::ensure_state_layout()?;
        let settings = sereno_core::storage::settings::load_or_recover();

        // 全局限速是进程级状态：启动时与每次保存设置都要同步到下载器
        sereno_core::download::ratelimit::set_limit_kbps(
            settings.download.speed_limit_kbps.unwrap_or(0) as u64,
        );

        // 上次异常退出可能把实例留在 `installing` / 删除标记状态，启动时收尾
        if let Err(error) = sereno_core::instance::lifecycle::recover_interrupted(&settings) {
            tracing::warn!(%error, "实例状态恢复失败，已跳过");
        }

        Ok(Self {
            settings: RwLock::new(settings),
            manifest_cache: ManifestCache::new(),
            started_at: Instant::now(),
            running: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// 运行注册表的共享句柄，供进程退出回调解除占用。
    pub fn running_registry(&self) -> Arc<Mutex<HashSet<String>>> {
        Arc::clone(&self.running)
    }

    /// 标记实例正在运行。已在运行时返回 `false`（调用方应回 `InstanceBusy`）。
    pub fn mark_running(&self, instance_id: &str) -> bool {
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        running.insert(instance_id.to_string())
    }

    pub fn unmark_running(&self, instance_id: &str) {
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        running.remove(instance_id);
    }

    pub fn is_running(&self, instance_id: &str) -> bool {
        self.running
            .lock()
            .map(|running| running.contains(instance_id))
            .unwrap_or(false)
    }

    pub fn running_instances(&self) -> Vec<String> {
        self.running
            .lock()
            .map(|running| running.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn snapshot_settings(&self) -> Result<AppSettings> {
        self.settings
            .read()
            .map(|s| s.clone())
            .map_err(|_| LauncherError::Internal {
                message: "设置锁被 poisoning".into(),
            })
    }

    pub fn update_settings(&self, settings: AppSettings) -> Result<()> {
        let settings = settings.clone().normalized();
        sereno_core::storage::settings::save(&settings)?;
        // 保存后立刻生效，不需要重启下载任务
        sereno_core::download::ratelimit::set_limit_kbps(
            settings.download.speed_limit_kbps.unwrap_or(0) as u64,
        );
        let mut guard = self.settings.write().map_err(|_| LauncherError::Internal {
            message: "设置锁被 poisoning".into(),
        })?;
        *guard = settings;
        Ok(())
    }
}
