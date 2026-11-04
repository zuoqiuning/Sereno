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

    /// 为 1.6 及更早版本生成 `assets/virtual/legacy/`。
    ///
    /// 这些版本的游戏不是从 `assets/objects/<hash>` 里按哈希取文件，
    /// 而是直接读 `assets/virtual/legacy/<名字>`。缺了这一步，老版本
    /// 能进游戏但**没有声音、没有语言文件**——用户很难把症状和"资源没铺开"联系起来。
    ///
    /// 优先硬链接（同盘零拷贝），失败退回复制；已存在的文件跳过。
    pub fn materialize_virtual_legacy(&self, instance_dir: &Path) -> Result<u32> {
        let assets = instance_dir.join("minecraft").join("assets");
        let objects = assets.join("objects");
        let virtual_dir = assets.join("virtual").join("legacy");

        let mut created = 0u32;
        for (name, object) in &self.objects {
            let Some(prefix) = object.hash.get(0..2) else {
                continue;
            };
            let source = objects.join(prefix).join(&object.hash);
            if !source.is_file() {
                continue;
            }

            // 索引里的 name 是相对路径，必须按归档规则校验，防止 `..`
            let Ok(relative) = crate::utils::archive::validate_entry_path(name) else {
                tracing::warn!(name, "legacy 资源名非法，已跳过");
                continue;
            };
            let destination = virtual_dir.join(&relative);
            if destination.is_file() {
                continue;
            }
            if let Some(parent) = destination.parent() {
                crate::utils::paths::ensure_dir(parent)?;
            }

            if std::fs::hard_link(&source, &destination).is_err() {
                std::fs::copy(&source, &destination).map_err(|e| LauncherError::Io {
                    message: format!("生成 legacy 资源失败 {}: {e}", destination.display()),
                })?;
            }
            created += 1;
        }

        if created > 0 {
            tracing::info!(created, "已生成 legacy 虚拟资源目录");
        }
        Ok(created)
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
