//! 资源索引（Asset Index）的解析与任务生成。

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::{LauncherError, Result};

#[derive(Debug, Deserialize)]
pub struct AssetIndexJson {
    pub objects: BTreeMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

impl AssetIndexJson {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| LauncherError::Io {
            message: format!("读取资源索引失败 {}: {e}", path.display()),
        })?;
        serde_json::from_str(&content).map_err(|e| LauncherError::Parse {
            message: format!("解析资源索引失败 {}: {e}", path.display()),
        })
    }

    pub fn build_download_tasks(
        &self,
        instance_dir: &Path,
        prefer_mirror: bool,
    ) -> Vec<DownloadTask> {
        let mut tasks = Vec::with_capacity(self.objects.len());

        for (name, obj) in &self.objects {
            // 哈希必须是两段十六进制；脏索引直接跳过，不能让启动器 panic
            let Some(prefix) = obj.hash.get(0..2).filter(|p| p.is_ascii()) else {
                continue;
            };
            let dest = instance_dir
                .join("minecraft")
                .join("assets")
                .join("objects")
                .join(prefix)
                .join(&obj.hash);

            let url = format!(
                "https://resources.download.minecraft.net/{prefix}/{}",
                obj.hash
            );
            let sources = crate::download::mirror::build_sources_with(&url, prefer_mirror);

            tasks.push(DownloadTask {
                id: format!("asset-{}", name),
                display_name: name.clone(),
                sources,
                destination: dest,
                checksum: Checksum::Sha1(obj.hash.clone()),
                size_hint: Some(obj.size),
                priority: 3,
                retry_policy: RetryPolicy::default(),
            });
        }

        tasks
    }
}
