//! Fabric / Quilt 这类「Meta + profile JSON」加载器的通用安装流程。
//!
//! 两家 API 结构几乎一致：先取 loader 版本，再取 profile JSON，
//! 下载 profile 里的 libraries，最后把 profile 写成 `versions/<id>/<id>.json`。

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::download::counter::SessionCounter;
use crate::download::queue::DownloadQueue;
use crate::download::session::DownloadSession;
use crate::download::sink::NoopSink;
use crate::download::task::{Checksum, DownloadTask, RetryPolicy};
use crate::models::instance::LoaderInfo;
use crate::models::settings::AppSettings;
use crate::models::version::VersionDetail;
use crate::utils::http::MAX_METADATA_BYTES;
use crate::version::artifact::maven_relative_path;
use crate::{LauncherError, Result};

pub struct MetaLoaderSpec {
    pub loader_type: &'static str,
    pub label: &'static str,
    /// 版本目录前缀，例如 `fabric-loader` / `quilt-loader`。
    pub id_prefix: &'static str,
    /// Meta API 根路径，例如 `https://meta.fabricmc.net/v2`。
    pub meta_base: &'static str,
    /// 可选镜像根路径（BMCLAPI 的 fabric-meta）。
    pub mirror_base: Option<&'static str>,
}

pub const FABRIC: MetaLoaderSpec = MetaLoaderSpec {
    loader_type: "fabric",
    label: "Fabric",
    id_prefix: "fabric-loader",
    meta_base: "https://meta.fabricmc.net/v2",
    mirror_base: Some("https://bmclapi2.bangbang93.com/fabric-meta/v2"),
};

pub const QUILT: MetaLoaderSpec = MetaLoaderSpec {
    loader_type: "quilt",
    label: "Quilt",
    id_prefix: "quilt-loader",
    meta_base: "https://meta.quiltmc.org/v3",
    mirror_base: None,
};

fn ordered_urls(official: String, mirror: Option<String>, prefer_mirror: bool) -> Vec<String> {
    match mirror {
        Some(mirror) if prefer_mirror => vec![mirror, official],
        Some(mirror) => vec![official, mirror],
        None => vec![official],
    }
}

pub async fn install_meta(
    client: &reqwest::Client,
    settings: &AppSettings,
    instance_dir: &Path,
    mc_version: &str,
    specified_version: Option<&str>,
    spec: &MetaLoaderSpec,
) -> Result<LoaderInfo> {
    let prefer_mirror = settings.download.prefer_mirror;
    let loader_version = match specified_version {
        Some(version) => version.to_string(),
        None => latest_loader_version(client, mc_version, prefer_mirror, spec).await?,
    };

    let profile_text =
        fetch_profile(client, mc_version, &loader_version, prefer_mirror, spec).await?;
    let detail: VersionDetail =
        serde_json::from_str(&profile_text).map_err(|e| LauncherError::Parse {
            message: format!("解析 {} Profile 失败: {e}", spec.label),
        })?;

    let tasks = build_library_tasks(&detail, instance_dir, prefer_mirror);
    let counter = Arc::new(SessionCounter::new(
        format!("{}-install", spec.loader_type),
        "loader",
    ));
    counter.add_total(
        tasks.len() as u32,
        tasks.iter().filter_map(|task| task.size_hint).sum(),
    );

    let queue = Arc::new(DownloadQueue::new(client.clone(), 8));
    DownloadSession::new(format!("{}-install", spec.loader_type), queue)
        .run_with_counter(tasks, Arc::new(NoopSink), counter)
        .await?;

    let id = format!("{}-{loader_version}-{mc_version}", spec.id_prefix);
    let version_dir = instance_dir.join("versions").join(&id);
    crate::utils::paths::ensure_dir(&version_dir)?;
    crate::utils::fs::atomic_write_str(&version_dir.join(format!("{id}.json")), &profile_text)?;

    Ok(LoaderInfo {
        r#type: spec.loader_type.to_string(),
        version: Some(loader_version),
    })
}

async fn latest_loader_version(
    client: &reqwest::Client,
    mc_version: &str,
    prefer_mirror: bool,
    spec: &MetaLoaderSpec,
) -> Result<String> {
    let official = format!("{}/versions/loader/{mc_version}", spec.meta_base);
    let mirror = spec
        .mirror_base
        .map(|base| format!("{base}/versions/loader/{mc_version}"));
    let urls = ordered_urls(official, mirror, prefer_mirror);

    for url in urls {
        let Ok(response) = client.get(&url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(json) =
            crate::utils::http::read_json_limited::<Value>(response, MAX_METADATA_BYTES).await
        else {
            continue;
        };

        if let Some(version) = json
            .as_array()
            .and_then(|array| array.first())
            .and_then(|value| value.get("loader"))
            .and_then(|loader| loader.get("version"))
            .and_then(|value| value.as_str())
        {
            return Ok(version.to_string());
        }
    }

    Err(LauncherError::DownloadFailed {
        message: format!("无法获取 {} 版本列表，官方与镜像均失败", spec.label),
        url: format!("{}/versions/loader/{mc_version}", spec.meta_base),
    })
}

async fn fetch_profile(
    client: &reqwest::Client,
    mc_version: &str,
    loader_version: &str,
    prefer_mirror: bool,
    spec: &MetaLoaderSpec,
) -> Result<String> {
    let official = format!(
        "{}/versions/loader/{mc_version}/{loader_version}/profile/json",
        spec.meta_base
    );
    let mirror = spec
        .mirror_base
        .map(|base| format!("{base}/versions/loader/{mc_version}/{loader_version}/profile/json"));
    let urls = ordered_urls(official, mirror, prefer_mirror);

    for url in urls {
        let Ok(response) = client.get(&url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        if let Ok(text) = response.text().await
            && text.contains("libraries")
        {
            return Ok(text);
        }
    }

    Err(LauncherError::DownloadFailed {
        message: format!("无法获取 {} Profile，官方与镜像均失败", spec.label),
        url: spec.meta_base.to_string(),
    })
}

fn build_library_tasks(
    detail: &VersionDetail,
    instance_dir: &Path,
    prefer_mirror: bool,
) -> Vec<DownloadTask> {
    let mut tasks = Vec::new();

    for library in &detail.libraries {
        let artifact = library.downloads.as_ref().and_then(|d| d.artifact.as_ref());

        let (url, checksum, size, relative) = if let Some(artifact) = artifact {
            let Some(relative) = artifact
                .path
                .clone()
                .or_else(|| maven_relative_path(&library.name))
            else {
                continue;
            };
            (
                artifact.url.clone(),
                Checksum::Sha1(artifact.sha1.clone()),
                Some(artifact.size as u64),
                relative,
            )
        } else if let Some(base_url) = &library.url {
            let Some(relative) = maven_relative_path(&library.name) else {
                continue;
            };
            (
                format!("{}/{}", base_url.trim_end_matches('/'), relative),
                Checksum::None,
                None,
                relative,
            )
        } else {
            continue;
        };

        tasks.push(DownloadTask {
            id: format!("{}-lib-{}", detail.id, library.name),
            display_name: relative.rsplit('/').next().unwrap_or(&relative).to_string(),
            sources: crate::download::mirror::build_sources_with(&url, prefer_mirror),
            destination: instance_dir.join("libraries").join(&relative),
            checksum,
            size_hint: size,
            priority: 7,
            retry_policy: RetryPolicy::default(),
        });
    }

    tasks
}
