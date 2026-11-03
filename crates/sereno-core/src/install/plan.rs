//! 从 `VersionDetail` 构建下载计划。

use std::path::Path;

use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::models::version::VersionDetail;
use crate::version::artifact::maven_relative_path;
use crate::version::rules::{OsContext, applies_to_client, filter_libraries};
use crate::{LauncherError, Result};

/// 构建结果：把 asset index 单独拿出来，方便安装命令「先下小索引 → 再一次性算全部任务」。
pub struct BaseInstallPlan {
    /// asset index 下载任务（单文件，体量小）。
    pub index_task: Option<DownloadTask>,
    /// 其它基础任务：client jar + libraries + natives + 日志配置（不含 index）。
    pub tasks: Vec<DownloadTask>,
    /// index 的 id，用来定位已下载的索引文件。
    pub asset_index_id: Option<String>,
}

pub fn build_base_plan(
    version: &VersionDetail,
    instance_dir: &Path,
    prefer_mirror: bool,
) -> Result<BaseInstallPlan> {
    let mut tasks = Vec::new();
    let os_ctx = OsContext::current();
    let sources = |url: &str| crate::download::mirror::build_sources_with(url, prefer_mirror);

    // 1. 客户端主 jar
    if let Some(client) = version.client_download() {
        let dest = instance_dir
            .join("versions")
            .join(&version.id)
            .join(format!("{}.jar", version.id));
        tasks.push(DownloadTask {
            id: "client-jar".to_string(),
            display_name: format!("{}.jar", version.id),
            sources: sources(&client.url),
            destination: dest,
            checksum: Checksum::Sha1(client.sha1.clone()),
            size_hint: Some(client.size as u64),
            priority: 9,
            retry_policy: RetryPolicy::default(),
        });
    } else {
        return Err(LauncherError::Parse {
            message: format!("版本 {} 缺少客户端下载信息", version.id),
        });
    }

    // 2. Libraries（含 natives、以及只有 Maven 地址的加载器依赖）
    for lib in filter_libraries(&version.libraries, &os_ctx)
        .into_iter()
        .filter(|l| applies_to_client(l))
    {
        let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());

        if let Some(artifact) = artifact {
            if let Some(relative) = artifact
                .path
                .clone()
                .or_else(|| maven_relative_path(&lib.name))
            {
                let dest = instance_dir.join("libraries").join(&relative);
                tasks.push(DownloadTask {
                    id: format!("lib-{}", lib.name),
                    display_name: file_name_of(&relative),
                    sources: sources(&artifact.url),
                    destination: dest,
                    checksum: Checksum::Sha1(artifact.sha1.clone()),
                    size_hint: Some(artifact.size as u64),
                    priority: 7,
                    retry_policy: RetryPolicy::default(),
                });
            }
        } else if let Some(base_url) = &lib.url {
            // 加载器注入的库常常只有仓库地址与坐标，没有 downloads 段
            if let Some(relative) = maven_relative_path(&lib.name) {
                let url = format!("{}/{}", base_url.trim_end_matches('/'), relative);
                let dest = instance_dir.join("libraries").join(&relative);
                tasks.push(DownloadTask {
                    id: format!("lib-{}", lib.name),
                    display_name: file_name_of(&relative),
                    sources: sources(&url),
                    destination: dest,
                    // 没有官方哈希，只能依赖大小未知的弱校验
                    checksum: Checksum::None,
                    size_hint: None,
                    priority: 7,
                    retry_policy: RetryPolicy::default(),
                });
            }
        }

        // 2.1 旧版 natives（通过 classifiers 声明的）
        if let Some(classifiers) = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()) {
            let natives_key = os_ctx.natives_classifier();
            if let Some(native_artifact) = classifiers.get(natives_key) {
                // 坐标通常是 `group:artifact:version`，补上 classifier 才能推导路径
                let coordinate = if lib.name.split(':').count() == 3 {
                    format!("{}:{natives_key}", lib.name)
                } else {
                    lib.name.clone()
                };
                if let Some(relative) = native_artifact
                    .path
                    .clone()
                    .or_else(|| maven_relative_path(&coordinate))
                {
                    let dest = instance_dir.join("libraries").join(&relative);
                    tasks.push(DownloadTask {
                        id: format!("native-{}", lib.name),
                        display_name: file_name_of(&relative),
                        sources: sources(&native_artifact.url),
                        destination: dest,
                        checksum: Checksum::Sha1(native_artifact.sha1.clone()),
                        size_hint: Some(native_artifact.size as u64),
                        priority: 7,
                        retry_policy: RetryPolicy::default(),
                    });
                }
            }
        }
    }

    // 3. log4j 配置：版本 JSON 声明了文件，但实际内容需要单独下载
    if let Some(file) = version
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
        .and_then(|c| c.file.as_ref())
    {
        let dest = instance_dir
            .join("minecraft")
            .join("assets")
            .join("log_configs")
            .join(&file.id);
        tasks.push(DownloadTask {
            id: "log-config".to_string(),
            display_name: file.id.clone(),
            sources: sources(&file.url),
            destination: dest,
            checksum: Checksum::Sha1(file.sha1.clone()),
            size_hint: Some(file.size as u64),
            priority: 5,
            retry_policy: RetryPolicy::default(),
        });
    }

    // 4. asset index（单独返回）
    let mut index_task = None;
    let mut asset_index_id = None;
    if let Some(asset_index) = &version.asset_index {
        asset_index_id = Some(asset_index.id.clone());
        let dest = instance_dir
            .join("minecraft")
            .join("assets")
            .join("indexes")
            .join(format!("{}.json", asset_index.id));

        index_task = Some(DownloadTask {
            id: "asset-index".to_string(),
            display_name: format!("{}.json", asset_index.id),
            sources: sources(&asset_index.url),
            destination: dest,
            checksum: Checksum::Sha1(asset_index.sha1.clone()),
            size_hint: Some(asset_index.size as u64),
            priority: 8,
            retry_policy: RetryPolicy::default(),
        });
    }

    Ok(BaseInstallPlan {
        index_task,
        tasks,
        asset_index_id,
    })
}

fn file_name_of(relative: &str) -> String {
    relative.rsplit('/').next().unwrap_or(relative).to_string()
}
