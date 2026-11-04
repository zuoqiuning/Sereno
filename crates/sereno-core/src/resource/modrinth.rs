use crate::models::resource::{
    ModDependency, ModFile, ModHashes, ModProject, ModVersion, ResourceKind, ResourceSearchQuery,
    ResourceSort, ResourceSource,
};
use crate::utils::disk_cache::DiskCache;
use crate::utils::http::{MAX_METADATA_BYTES, read_json_limited};
use crate::utils::ttl_cache::TtlCache;
use crate::{LauncherError, Result};
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
    #[serde(default)]
    total_hits: u32,
}

#[derive(Deserialize)]
struct SearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    icon_url: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    follows: u64,
    author: String,
    #[serde(default)]
    date_modified: Option<String>,
}

/// 单页结果。`total` 是站点给出的全量命中数，用于分页。
pub struct SearchPage {
    pub items: Vec<ModProject>,
    pub total: u32,
}

/// 版本列表缓存：同一项目在筛选条件不变时重复下载/查看版本无需再请求。
static VERSION_CACHE: LazyLock<TtlCache<String, Vec<ModVersion>>> =
    LazyLock::new(|| TtlCache::new(Duration::from_secs(86400), 256));
static VERSION_DISK: LazyLock<Option<DiskCache>> = LazyLock::new(|| {
    DiskCache::new_with_cap(
        "resource/versions/modrinth",
        Duration::from_secs(86400),
        256,
        48 * 1024 * 1024,
    )
    .ok()
});

/// Modrinth 没有 Key，但有按 IP 的速率限制。429 / 5xx 时短暂退避，
/// 避免把一次瞬时限流直接变成页面报错。
pub(crate) async fn get_with_retry(
    client: &reqwest::Client,
    url: &str,
) -> Result<reqwest::Response> {
    for attempt in 0..3 {
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| LauncherError::DownloadFailed {
                message: format!("请求 Modrinth 失败: {e}"),
                url: url.to_string(),
            })?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
            || resp.status().is_server_error()
        {
            if attempt == 2 {
                return Ok(resp);
            }
            let wait = resp
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_else(|| 2u64.pow(attempt + 1))
                .min(30);
            sleep(Duration::from_secs(wait)).await;
            continue;
        }

        return Ok(resp);
    }

    Err(LauncherError::DownloadFailed {
        message: "Modrinth 请求重试次数已用尽".into(),
        url: url.to_string(),
    })
}

/// 请求一个 Modrinth API 地址：**镜像优先，官方兜底**。
///
/// 镜像（MCIM）在国内快得多，也不用管官方 API 的匿名配额；
/// 但它有同步延迟，偶尔会缺刚发布的版本，甚至临时不可用。
/// 因此镜像失败或返回 404 时自动切到官方地址。
///
/// 只有**镜像自己返回的 404** 才回退——网络错误会先重试，
/// 因为镜像抽风时官方往往也不通，重试没有意义。
pub(crate) async fn get_mirrored(client: &reqwest::Client, url: &str) -> Result<reqwest::Response> {
    if !crate::resource::curseforge::mirror_enabled() {
        return get_with_retry(client, url).await;
    }

    let Some(mirror_url) = crate::resource::mcim::modrinth_api_mirror(url) else {
        return get_with_retry(client, url).await;
    };

    match get_with_retry(client, &mirror_url).await {
        Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
            // 镜像还没同步到——用官方再试一次。
            tracing::debug!("MCIM 镜像缺少该资源（404），回落到官方：{url}");
            get_with_retry(client, url).await
        }
        Ok(resp) => Ok(resp),
        Err(error) => {
            // 镜像整个不可用（DNS/超时）时降级到官方。
            tracing::warn!("MCIM 镜像不可用，回落到官方：{mirror_url}（{error}）");
            get_with_retry(client, url).await
        }
    }
}

/// 搜索 Modrinth。`offset` 是绝对偏移（第 N 页 = (N-1) * page_size）。
pub async fn search(
    client: &reqwest::Client,
    filters: &ResourceSearchQuery,
    offset: u32,
    limit: u32,
) -> Result<SearchPage> {
    // Modrinth 单次最多 100 条，limit=0 会 400。
    let limit = limit.clamp(1, 100);
    let index = match filters.sort {
        ResourceSort::Downloads => "downloads",
        ResourceSort::Updated => "updated",
    };
    let mut url =
        format!("https://api.modrinth.com/v2/search?limit={limit}&offset={offset}&index={index}");

    if !filters.query.trim().is_empty() {
        url.push_str(&format!("&query={}", urlencoding::encode(&filters.query)));
    }

    // facets: [["project_type:mod"], ["versions:1.21.1"], ["categories:fabric"]]
    let mut facets = vec![format!(
        r#"["project_type:{}"]"#,
        match filters.kind {
            ResourceKind::Mod => "mod",
            ResourceKind::Modpack => "modpack",
            ResourceKind::Resourcepack => "resourcepack",
            ResourceKind::Shader => "shader",
            ResourceKind::Datapack => "datapack",
        }
    )];
    if !filters.mc_version.is_empty() {
        facets.push(format!(r#"["versions:{}"]"#, filters.mc_version));
    }
    // 加载器筛选只对 Mod 有意义；整合包在 Modrinth 上没有统一的 loader 分类。
    if filters.kind == ResourceKind::Mod
        && !filters.loader.is_empty()
        && filters.loader != "vanilla"
    {
        facets.push(format!(r#"["categories:{}"]"#, filters.loader));
    }
    url.push_str(&format!(
        "&facets={}",
        urlencoding::encode(&format!("[{}]", facets.join(",")))
    ));

    let resp = get_mirrored(client, &url).await?;

    if !resp.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("Modrinth 返回 HTTP {}", resp.status()),
            url,
        });
    }

    let data: SearchResponse = read_json_limited(resp, MAX_METADATA_BYTES).await?;
    Ok(SearchPage {
        items: data
            .hits
            .into_iter()
            .map(|h| ModProject {
                id: h.project_id,
                slug: h.slug,
                title: h.title,
                description: h.description,
                icon_url: h.icon_url,
                downloads: h.downloads,
                follows: h.follows,
                author: h.author,
                source: ResourceSource::Modrinth,
                updated_at: h.date_modified,
            })
            .collect(),
        total: data.total_hits,
    })
}

/// 依赖安装：按项目 + 实例版本/加载器挑一个可下载文件。
///
/// `version_id` 是 Modrinth 依赖里给出的精确版本；没有时退化为该条件下最新版本。
pub async fn dependency_file(
    client: &reqwest::Client,
    project_id: &str,
    version_id: Option<&str>,
    mc_version: &str,
    loader: &str,
) -> Result<Option<ModFile>> {
    let versions = get_versions(client, project_id, mc_version, loader).await?;
    let picked = version_id
        .and_then(|id| versions.iter().find(|version| version.id == id))
        .or_else(|| versions.first());
    Ok(picked.and_then(|version| {
        version
            .files
            .iter()
            .find(|file| file.primary)
            .or_else(|| version.files.first())
            .cloned()
    }))
}

/// 一次查询多个游戏版本的版本列表。
///
/// **注意**：Modrinth 的 `game_versions` 多值过滤并不可靠——实测传 12 个版本号
/// 会直接退化成全量（7 MB / 10 秒），比逐个查还慢。所以这里逐个版本并发查询，
/// 再合并去重：每个版本的响应只有它自己的那些构建，量小得多。
///
/// `versions` 为空表示不过滤（拉全量，通常很慢，只在必要时用）。
pub async fn get_versions_for(
    client: &reqwest::Client,
    project_id: &str,
    versions: &[String],
    loader: &str,
) -> Result<Vec<ModVersion>> {
    if versions.is_empty() {
        return get_versions(client, project_id, "", loader).await;
    }

    let queries = versions
        .iter()
        .filter(|version| !version.trim().is_empty())
        .map(|version| get_versions(client, project_id, version, loader));

    let results = futures_util::future::join_all(queries).await;

    // 同一个版本可能命中多个游戏版本条件，按 id 去重。
    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();
    for result in results {
        let Ok(batch) = result else { continue };
        for version in batch {
            if seen.insert(version.id.clone()) {
                merged.push(version);
            }
        }
    }

    Ok(merged)
}

/// 取某个项目的版本列表（可按单个游戏版本 / 加载器过滤）。
pub async fn get_versions(
    client: &reqwest::Client,
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Result<Vec<ModVersion>> {
    let mut url = format!("https://api.modrinth.com/v2/project/{}/version", project_id);
    let mut query = vec![];
    if !mc_version.is_empty() {
        query.push(format!(
            "game_versions=[\"{}\"]",
            mc_version.replace('"', "")
        ));
    }
    if !loader.is_empty() && loader != "vanilla" {
        query.push(format!("loaders=[\"{}\"]", loader));
    }
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query.join("&"));
    }

    if let Some(cached) = VERSION_CACHE.get(&url) {
        return Ok(cached);
    }
    if let Some(cached) = VERSION_DISK
        .as_ref()
        .and_then(|cache| cache.get::<Vec<ModVersion>>(&url))
    {
        VERSION_CACHE.insert(url.clone(), cached.clone());
        return Ok(cached);
    }

    let resp = get_mirrored(client, &url).await?;

    if !resp.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("Modrinth 版本接口返回 HTTP {}", resp.status()),
            url,
        });
    }

    #[derive(Deserialize)]
    struct ApiVersion {
        id: String,
        name: String,
        version_number: String,
        game_versions: Vec<String>,
        loaders: Vec<String>,
        files: Vec<ApiFile>,
        #[serde(default)]
        downloads: u64,
        #[serde(default)]
        date_published: Option<String>,
        #[serde(default)]
        version_type: Option<String>,
        #[serde(default)]
        dependencies: Vec<ApiDependency>,
    }

    #[derive(Deserialize)]
    struct ApiDependency {
        project_id: String,
        #[serde(default)]
        version_id: Option<String>,
        #[serde(default)]
        dependency_type: Option<String>,
    }

    #[derive(Deserialize)]
    struct ApiFile {
        url: String,
        filename: String,
        primary: bool,
        hashes: ApiHashes,
    }

    #[derive(Deserialize)]
    struct ApiHashes {
        sha1: Option<String>,
    }

    let data: Vec<ApiVersion> = read_json_limited(resp, MAX_METADATA_BYTES).await?;
    let versions: Vec<ModVersion> = data
        .into_iter()
        .map(|v| ModVersion {
            id: v.id,
            name: v.name,
            version_number: v.version_number,
            game_versions: v.game_versions,
            loaders: v.loaders,
            files: v
                .files
                .into_iter()
                .map(|f| ModFile {
                    url: f.url,
                    filename: f.filename,
                    primary: f.primary,
                    hashes: ModHashes {
                        sha1: f.hashes.sha1,
                    },
                })
                .collect(),
            downloads: v.downloads,
            date_published: v.date_published,
            version_type: v.version_type,
            dependencies: v
                .dependencies
                .into_iter()
                .map(|dependency| ModDependency {
                    project_id: dependency.project_id,
                    version_id: dependency.version_id,
                    dependency_type: dependency.dependency_type,
                })
                .collect(),
        })
        .collect();

    VERSION_CACHE.insert(url.clone(), versions.clone());
    if let Some(cache) = VERSION_DISK.as_ref() {
        cache.insert(&url, &versions);
    }
    Ok(versions)
}
