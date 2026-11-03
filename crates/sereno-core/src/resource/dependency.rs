//! 资源依赖解析：把 Modrinth / CurseForge 的依赖 ID 换成可展示的名称与图标。
//!
//! 版本接口只返回依赖的项目 ID（CurseForge 是数字 modId），前端需要名称才能展示
//! 「前置：Fabric API」这种信息，所以这里做一次批量解析。

use serde::Deserialize;
use std::collections::BTreeSet;

use crate::models::resource::{ResourceDependencyInfo, ResourceSource};
use crate::utils::http::{MAX_METADATA_BYTES, read_json_limited};
use crate::{LauncherError, Result};

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const CF_API: &str = "https://api.curseforge.com/v1";
const MAX_DEPENDENCIES: usize = 100;

#[derive(Deserialize)]
struct MrProject {
    id: String,
    title: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Deserialize)]
struct CfLogo {
    thumbnail_url: String,
}

#[derive(Deserialize)]
struct CfProject {
    id: u32,
    name: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    date_modified: Option<String>,
    #[serde(default)]
    logo: Option<CfLogo>,
}

#[derive(Deserialize)]
struct CfProjectList {
    #[serde(default)]
    data: Vec<CfProject>,
}

/// 批量解析依赖项目；无法识别的 ID 会被跳过，由前端显示为「未知前置」。
pub async fn resolve(
    client: &reqwest::Client,
    source: ResourceSource,
    api_keys: &[String],
    ids: &[String],
) -> Result<Vec<ResourceDependencyInfo>> {
    let ids: Vec<String> = ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(MAX_DEPENDENCIES)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    match source {
        ResourceSource::Modrinth => resolve_modrinth(client, &ids).await,
        ResourceSource::Curseforge => resolve_curseforge(client, api_keys, &ids).await,
        ResourceSource::Aggregate => Err(LauncherError::InvalidConfig {
            key: "source".into(),
            message: "依赖解析需要指定具体来源".into(),
        }),
    }
}

async fn resolve_modrinth(
    client: &reqwest::Client,
    ids: &[String],
) -> Result<Vec<ResourceDependencyInfo>> {
    let ids_json = serde_json::to_string(ids).map_err(|e| LauncherError::Internal {
        message: format!("序列化依赖 ID 失败: {e}"),
    })?;
    let url = format!(
        "{MODRINTH_API}/projects?ids={}",
        urlencoding::encode(&ids_json)
    );
    let resp = crate::resource::modrinth::get_with_retry(client, &url).await?;
    if !resp.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("Modrinth 项目接口返回 HTTP {}", resp.status()),
            url,
        });
    }

    let projects: Vec<MrProject> = read_json_limited(resp, MAX_METADATA_BYTES).await?;
    Ok(projects
        .into_iter()
        .map(|project| ResourceDependencyInfo {
            id: project.id,
            name: project.title,
            icon_url: project.icon_url,
            description: project.description,
            downloads: project.downloads,
            updated_at: project.updated_at,
            source: ResourceSource::Modrinth,
        })
        .collect())
}

async fn resolve_curseforge(
    client: &reqwest::Client,
    api_keys: &[String],
    ids: &[String],
) -> Result<Vec<ResourceDependencyInfo>> {
    let mod_ids: Vec<u32> = ids.iter().filter_map(|id| id.parse().ok()).collect();
    if mod_ids.is_empty() {
        return Ok(Vec::new());
    }

    let body = serde_json::json!({ "modIds": mod_ids });
    let data: CfProjectList =
        crate::resource::local::cf_post_json(client, &format!("{CF_API}/mods"), &body, api_keys)
            .await?;

    Ok(data
        .data
        .into_iter()
        .map(|project| ResourceDependencyInfo {
            id: project.id.to_string(),
            name: project.name,
            icon_url: project.logo.map(|logo| logo.thumbnail_url),
            description: project.summary,
            downloads: project.download_count,
            updated_at: project.date_modified,
            source: ResourceSource::Curseforge,
        })
        .collect())
}
