use crate::models::resource::{
    CfFile, CfSearchResponse, ModProject, ResourceKind, ResourceSearchQuery, ResourceSort,
    ResourceSource,
};
use crate::utils::disk_cache::DiskCache;
use crate::utils::http::{MAX_METADATA_BYTES, read_json_limited};
use crate::utils::ttl_cache::TtlCache;
use crate::{LauncherError, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

const CF_API_BASE: &str = "https://api.curseforge.com/v1";
/// MCIM 镜像：路径与官方一致，只是把域名换成 `mod.mcimirror.top/curseforge`。
const CF_MIRROR_BASE: &str = "https://mod.mcimirror.top/curseforge/v1";
/// 文件 CDN 镜像：`edge.forgecdn.net` → `mod.mcimirror.top`。
const CF_CDN_MIRROR: &str = "https://mod.mcimirror.top";
const GAME_ID_MINECRAFT: u32 = 432;
/// CurseForge 单次最多 50 条，超过会被直接拒绝。
pub const CF_PAGE_MAX: u32 = 50;
/// 批量查询文件时单次提交的 ID 上限。
///
/// 官方文档没有写死上限，但一次提交几千个 ID 会让响应体变得巨大
/// （每个文件对象约 1 KB），反而来不及就被网关掐断。50 是稳妥值。
const BATCH_FILE_LIMIT: usize = 50;
/// 批量查询项目时单次提交的 ID 上限。同上。
const BATCH_MOD_LIMIT: usize = 50;
/// 详情页首屏只要最新一批文件，其余分支由「按版本补全」按需取回。
pub const VERSION_FIRST_PAGE_FILES: usize = CF_PAGE_MAX as usize;
/// 更新检查等场景需要完整的文件列表，最多取 200 个（4 页）。
const VERSION_MAX_FILES: usize = 200;

/// 两次请求**发起**之间的最小间隔。
///
/// 旧实现是一把「串行闸门」：后一个请求要等前一个**完全结束**才能开始，
/// 于是「逐个游戏版本补全分支」会退化成纯串行等待（十来个版本就是好几秒）。
/// 现在只约束发起间隔，在途请求可以重叠，再配合在途上限，
/// 既把等待压下去，又不会瞬间打满 CF 配额。
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(120);
/// 同时在途的请求上限。
const MAX_IN_FLIGHT: usize = 4;

/// 搜索结果缓存：翻页与重复下载时命中缓存，避免重复消耗配额。
const SEARCH_CACHE_TTL: Duration = Duration::from_secs(900);
/// 版本列表缓存：版本列表几乎不变，缓存久一点，重复打开详情页可以秒回。
const VERSION_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_RETRIES: u32 = 4;

static SEARCH_CACHE: LazyLock<TtlCache<String, CfSearchResponse>> =
    LazyLock::new(|| TtlCache::new(SEARCH_CACHE_TTL, 300));
static VERSION_CACHE: LazyLock<TtlCache<String, Vec<CfFile>>> =
    LazyLock::new(|| TtlCache::new(VERSION_CACHE_TTL, 256));
static VERSION_DISK: LazyLock<Option<DiskCache>> = LazyLock::new(|| {
    DiskCache::new_with_cap(
        "resource/versions/curseforge",
        VERSION_CACHE_TTL,
        256,
        48 * 1024 * 1024,
    )
    .ok()
});
/// 在途请求的许可：同时最多 MAX_IN_FLIGHT 个请求。
static REQUEST_SLOTS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(MAX_IN_FLIGHT)));
/// 上一次请求的发起时刻，用来保证发起间隔。
static LAST_REQUEST_START: LazyLock<tokio::sync::Mutex<Instant>> =
    LazyLock::new(|| tokio::sync::Mutex::new(Instant::now() - MIN_REQUEST_INTERVAL));
/// 429 后的全局冷却。冷却期间聚合搜索直接跳过 CF，避免每个请求都苦等重试。
static COOLDOWN_UNTIL: LazyLock<std::sync::Mutex<Option<Instant>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));
/// 是否走 MCIM 国内镜像（由 `build_client` 按设置同步）。
static MIRROR_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_mirror_enabled(enabled: bool) {
    MIRROR_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn mirror_enabled() -> bool {
    MIRROR_ENABLED.load(Ordering::Relaxed)
}

/// 测试专用的镜像开关切换。
///
/// 镜像开关是进程级全局状态，而 Rust 的测试默认并行执行——
/// 直接在用例里改它会让其它并行用例读到意料之外的值。
/// 这里用一把锁把「改全局」的用例串起来，并在结束后恢复原值。
#[cfg(test)]
pub(crate) fn with_mirror_enabled(enabled: bool, f: impl FnOnce()) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let original = mirror_enabled();
    set_mirror_enabled(enabled);
    f();
    set_mirror_enabled(original);
    drop(guard);
}

/// 当前使用的 CurseForge 接口根地址。
///
/// 镜像模式（默认）走 MCIM，**不需要 API Key**；
/// 关掉镜像则回落到官方接口，而官方接口强制要求 API Key——
/// 本项目不再携带任何 Key，因此官方模式必然被官方拒绝，
/// 具体表现由 [`request`] 统一翻译成「请开回镜像」的提示。
pub fn api_base() -> &'static str {
    if mirror_enabled() {
        CF_MIRROR_BASE
    } else {
        CF_API_BASE
    }
}

/// 把文件 CDN 换成镜像地址；其他域名（例如 `mediafilez`）保持原样。
pub fn mirror_file_url(url: &str) -> String {
    if !mirror_enabled() {
        return url.to_string();
    }
    match url.strip_prefix("https://edge.forgecdn.net/") {
        Some(rest) => format!("{CF_CDN_MIRROR}/{rest}"),
        None => url.to_string(),
    }
}

pub fn in_cooldown() -> bool {
    COOLDOWN_UNTIL
        .lock()
        .map(|until| until.is_some_and(|time| time > Instant::now()))
        .unwrap_or(false)
}

fn mark_cooldown(seconds: u64) {
    if let Ok(mut until) = COOLDOWN_UNTIL.lock() {
        let target = Instant::now() + Duration::from_secs(seconds.max(30));
        if until.is_none_or(|current| current < target) {
            *until = Some(target);
        }
    }
}

/// 让所有 CurseForge 请求按节奏发出。
///
/// 只约束「发起间隔」：请求本身允许重叠，这样批量补全多个游戏版本时
/// 不会退化成一次接一次的串行等待。在途请求的数量由 `REQUEST_SLOTS` 限制。
async fn throttle() {
    let wait = {
        let mut last = LAST_REQUEST_START.lock().await;
        let now = Instant::now();
        let earliest = *last + MIN_REQUEST_INTERVAL;
        *last = now;
        earliest.saturating_duration_since(now)
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

/// 带节流与 429 退避的请求封装。
///
/// `body` 为 `None` 时发 GET，否则发 JSON POST。
/// `local` 模块的指纹/批量查询也走这里，避免维护第二套重试规则。
///
/// **不带任何鉴权头**：镜像不需要 Key，官方接口需要 Key 而本项目不再携带，
/// 因此官方模式只会在被拒绝时给出明确提示（见 [`official_key_required`]）。
pub(crate) async fn request(
    client: &reqwest::Client,
    url: &str,
    body: Option<&serde_json::Value>,
) -> Result<reqwest::Response> {
    // 全局冷却期间直接失败：连在途许可都不占，才算真的“冷却”。
    let mirror = mirror_enabled();
    if !mirror && in_cooldown() {
        return Err(LauncherError::DownloadFailed {
            message: "CurseForge 正在限流冷却，请稍后重试或改用 Modrinth".into(),
            url: url.to_string(),
        });
    }

    // 在途请求许可：同时最多 MAX_IN_FLIGHT 个，避免瞬间打满配额。
    let _permit = REQUEST_SLOTS
        .acquire()
        .await
        .map_err(|_| LauncherError::DownloadFailed {
            message: "CurseForge 请求通道已关闭".into(),
            url: url.to_string(),
        })?;

    for attempt in 1..=MAX_RETRIES {
        throttle().await;

        let builder = match body {
            Some(body) => client.post(url).json(body),
            None => client.get(url),
        };
        let resp = builder
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| LauncherError::DownloadFailed {
                message: format!("CurseForge 请求失败: {e}"),
                url: url.to_string(),
            })?;

        // 官方接口没有 Key 一定被拒（401/403）。翻译成「开回镜像」的提示，
        // 而不是把一个用户看不懂的状态码丢出去。
        if !mirror && is_auth_rejected(resp.status()) {
            return Err(official_key_required());
        }

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let wait_secs = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(|secs| secs.min(60))
                .unwrap_or(60);

            // 立刻进入全局冷却：聚合搜索会改为只显示 Modrinth，
            // 不再让每个页面都等待完整的重试链。
            mark_cooldown(wait_secs);

            return Err(LauncherError::DownloadFailed {
                message: format!(
                    "CurseForge 暂时限流，已进入 {wait_secs} 秒冷却，请稍后重试或切换 Modrinth"
                ),
                url: url.to_string(),
            });
        }

        if resp.status().is_server_error() {
            if attempt == MAX_RETRIES {
                return Ok(resp);
            }
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
            continue;
        }

        return Ok(resp);
    }

    Err(LauncherError::DownloadFailed {
        message: "CurseForge 请求重试次数已用尽".into(),
        url: url.to_string(),
    })
}

/// 官方接口在缺少 API Key 时的拒绝状态码。
fn is_auth_rejected(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
}

/// 「官方接口要 Key」的统一错误。
///
/// 官方 CF 接口强制要求鉴权请求头，而本项目**不再携带任何 Key**——
/// 打进安装包的凭据等于公开凭据。关掉镜像后官方接口必然 401/403，
/// 因此这里把「请开回镜像」这个可执行的下一步说清楚。
fn official_key_required() -> LauncherError {
    LauncherError::CurseForgeOfficialKeyRequired
}

/// GET 版本的简写。
async fn request_with_retry(client: &reqwest::Client, url: &str) -> Result<reqwest::Response> {
    request(client, url, None).await
}

/// 单页搜索结果。
pub struct SearchPage {
    pub items: Vec<ModProject>,
    pub total: u32,
}

/// 搜索 CurseForge。`offset` 是绝对偏移，`limit` 超过 50 会被压到 50。
pub async fn search(
    client: &reqwest::Client,
    filters: &ResourceSearchQuery,
    offset: u32,
    limit: u32,
) -> Result<SearchPage> {
    let page_size = limit.clamp(1, CF_PAGE_MAX);
    // classId: Mod = 6，整合包 = 4471，资源包 = 12，光影 = 6552，数据包 = 6945；
    // sortField: 下载量 = 6，更新时间 = 3。
    let class_id = match filters.kind {
        ResourceKind::Mod => 6,
        ResourceKind::Modpack => 4471,
        ResourceKind::Resourcepack => 12,
        ResourceKind::Shader => 6552,
        ResourceKind::Datapack => 6945,
    };
    let sort_field = match filters.sort {
        ResourceSort::Downloads => 6,
        ResourceSort::Updated => 3,
    };
    let mut url = format!(
        "{}/mods/search?gameId={GAME_ID_MINECRAFT}&classId={class_id}&sortField={sort_field}&sortOrder=desc&pageSize={page_size}",
        api_base()
    );
    if offset > 0 {
        url.push_str(&format!("&index={offset}"));
    }
    if !filters.mc_version.is_empty() {
        url.push_str(&format!(
            "&gameVersion={}",
            urlencoding::encode(&filters.mc_version)
        ));
    }
    if !filters.query.trim().is_empty() {
        url.push_str(&format!(
            "&searchFilter={}",
            urlencoding::encode(&filters.query)
        ));
    }
    // 加载器筛选只对 Mod 生效：整合包的 loader 字段不是可靠的筛选维度。
    if filters.kind == ResourceKind::Mod
        && !filters.loader.is_empty()
        && filters.loader != "vanilla"
    {
        let loader_type = match filters.loader.as_str() {
            "forge" => Some("1"),
            "fabric" => Some("4"),
            "neoforge" => Some("6"),
            "quilt" => Some("5"),
            _ => None,
        };
        if let Some(loader_type) = loader_type {
            url.push_str(&format!("&modLoaderType={loader_type}"));
        }
    }

    if let Some(cached) = SEARCH_CACHE.get(&url) {
        return Ok(map_page(cached));
    }
    if !mirror_enabled() && in_cooldown() {
        return Err(LauncherError::DownloadFailed {
            message: "CurseForge 正在限流冷却，请稍后重试或切换 Modrinth".into(),
            url: url.clone(),
        });
    }

    let resp = request_with_retry(client, &url).await?;

    if !resp.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("CurseForge 搜索返回 HTTP {}", resp.status()),
            url,
        });
    }

    let data: CfSearchResponse = read_json_limited(resp, MAX_METADATA_BYTES).await?;
    SEARCH_CACHE.insert(url, data.clone());
    Ok(map_page(data))
}

fn map_page(data: CfSearchResponse) -> SearchPage {
    let total = data.pagination.total_count;
    SearchPage {
        items: data
            .data
            .into_iter()
            .map(|m| ModProject {
                id: m.id.to_string(),
                slug: m.slug,
                title: m.name,
                description: m.summary,
                icon_url: m.logo.map(|l| l.thumbnail_url),
                downloads: m.download_count,
                follows: 0,
                author: m
                    .authors
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Unknown".into()),
                source: ResourceSource::Curseforge,
                updated_at: m.date_modified,
            })
            .collect(),
        total,
    }
}

/// 依赖安装：在目标实例的游戏版本 / 加载器条件下挑一个可下载文件。
pub async fn dependency_file(
    client: &reqwest::Client,
    mod_id: u32,
    mc_version: &str,
    loader: &str,
) -> Result<Option<CfFile>> {
    let files = get_versions(client, mod_id, mc_version, loader, VERSION_MAX_FILES).await?;
    Ok(files.into_iter().find(|file| file.download_url.is_some()))
}

/// 批量查询多个游戏版本的文件列表（用于补全版本分支）。
///
/// CurseForge 的文件列表按**上传时间**排序，只查一次会漏掉较早发布的分支，
/// 所以补全必须逐个游戏版本去查。这些查询可以并发发起：
/// 节流器只保证「发起间隔」，在途请求数也有上限。
///
/// 任何一个版本查失败都不影响其它版本的结果。
pub async fn get_versions_for(
    client: &reqwest::Client,
    mod_id: u32,
    versions: &[String],
    loader: &str,
) -> Result<Vec<CfFile>> {
    if versions.is_empty() {
        return Ok(Vec::new());
    }

    let queries = versions
        .iter()
        .map(|version| get_versions(client, mod_id, version, loader, VERSION_FIRST_PAGE_FILES));

    let results = futures_util::future::join_all(queries).await;

    // 同一个文件可能同时命中多个版本（文件标签里既有 1.21 也有 1.21.1），按 id 去重。
    let mut seen = std::collections::HashSet::new();
    let mut files = Vec::new();
    for result in results {
        let Ok(batch) = result else { continue };
        for file in batch {
            if seen.insert(file.id) {
                files.push(file);
            }
        }
    }

    Ok(files)
}

/// 批量查询文件详情。
///
/// **整合包安装必须走这个接口**：CurseForge 的 `manifest.json` 里只有
/// `projectID` / `fileID`，没有下载地址，只能回查 API 才知道文件在哪。
/// 一次查一个文件会让上百个 MOD 的整合包花掉上百个请求（CF 配额按请求数算），
/// 因此这里走官方的批量接口 `POST /v1/mods/files`。
///
/// 单次最多提交 `BATCH_FILE_LIMIT` 个 ID，超出部分自动分批。
pub async fn get_files_by_ids(client: &reqwest::Client, file_ids: &[u32]) -> Result<Vec<CfFile>> {
    let mut result = Vec::new();
    for chunk in file_ids.chunks(BATCH_FILE_LIMIT) {
        let url = format!("{}/mods/files", api_base());
        let body = serde_json::json!({ "fileIds": chunk });
        let resp = request(client, &url, Some(&body)).await?;

        if !resp.status().is_success() {
            return Err(LauncherError::DownloadFailed {
                message: format!("CurseForge 批量查询文件返回 HTTP {}", resp.status()),
                url,
            });
        }

        let data: crate::models::resource::CfFilesResponse =
            read_json_limited(resp, MAX_METADATA_BYTES).await?;
        result.extend(data.data);
    }

    // 镜像模式下 CDN 域名要换成镜像，否则国内直连官方 CDN 会被卡住。
    if mirror_enabled() {
        for file in &mut result {
            if let Some(url) = file.download_url.take() {
                file.download_url = Some(mirror_file_url(&url));
            }
        }
    }

    Ok(result)
}

/// 批量查询项目（只要分类，用于决定文件落到 `mods/` 还是 `resourcepacks/`）。
pub async fn get_mods_by_ids(
    client: &reqwest::Client,
    mod_ids: &[u32],
) -> Result<Vec<crate::models::resource::CfModBrief>> {
    let mut result = Vec::new();
    for chunk in mod_ids.chunks(BATCH_MOD_LIMIT) {
        let url = format!("{}/mods", api_base());
        let body = serde_json::json!({ "modIds": chunk });
        let resp = request(client, &url, Some(&body)).await?;

        if !resp.status().is_success() {
            return Err(LauncherError::DownloadFailed {
                message: format!("CurseForge 批量查询项目返回 HTTP {}", resp.status()),
                url,
            });
        }

        let data: crate::models::resource::CfModsBriefResponse =
            read_json_limited(resp, MAX_METADATA_BYTES).await?;
        result.extend(data.data);
    }

    Ok(result)
}

/// 查询单个项目下的某个文件。
///
/// 整合包 zip 自身的下载地址就靠它拿（整合包在 CF 上也是一个普通文件）。
pub async fn get_file(
    client: &reqwest::Client,
    mod_id: u32,
    file_id: u32,
) -> Result<Option<CfFile>> {
    let url = format!("{}/mods/{mod_id}/files/{file_id}", api_base());
    let resp = request(client, &url, None).await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("CurseForge 查询文件返回 HTTP {}", resp.status()),
            url,
        });
    }

    let data: crate::models::resource::CfFileResponse =
        read_json_limited(resp, MAX_METADATA_BYTES).await?;
    let mut file = data.data;
    if mirror_enabled()
        && let Some(url) = file.download_url.take()
    {
        file.download_url = Some(mirror_file_url(&url));
    }
    Ok(Some(file))
}

/// 取某个项目（或某个游戏版本 / 加载器）的文件列表。
///
/// `max_files` 控制最多取多少个文件：详情页首屏只要最新一批，
/// 更新检查等场景才需要拉满。
pub async fn get_versions(
    client: &reqwest::Client,
    mod_id: u32,
    mc_version: &str,
    loader: &str,
    max_files: usize,
) -> Result<Vec<CfFile>> {
    let mut url = format!("{}/mods/{mod_id}/files?pageSize={CF_PAGE_MAX}", api_base());

    if !mc_version.is_empty() {
        url.push_str(&format!("&gameVersion={}", urlencoding::encode(mc_version)));
    }
    if !loader.is_empty() && loader != "vanilla" {
        let loader_type = match loader {
            "forge" => "1",
            "fabric" => "4",
            "neoforge" => "6",
            "quilt" => "5",
            _ => "",
        };
        if !loader_type.is_empty() {
            url.push_str(&format!("&modLoaderType={loader_type}"));
        }
    }

    if let Some(cached) = VERSION_CACHE.get(&url) {
        return Ok(cached);
    }
    if let Some(cached) = VERSION_DISK
        .as_ref()
        .and_then(|cache| cache.get::<Vec<CfFile>>(&url))
    {
        VERSION_CACHE.insert(url.clone(), cached.clone());
        return Ok(cached);
    }
    if !mirror_enabled() && in_cooldown() {
        return Err(LauncherError::DownloadFailed {
            message: "CurseForge 正在限流冷却，请稍后重试或切换 Modrinth".into(),
            url: url.clone(),
        });
    }

    let mut files = Vec::new();
    let mut offset = 0usize;

    while files.len() < max_files {
        let page_url = format!("{url}&index={offset}");
        let resp = request_with_retry(client, &page_url).await?;

        if !resp.status().is_success() {
            return Err(LauncherError::DownloadFailed {
                message: format!("CurseForge 获取文件列表返回 HTTP {}", resp.status()),
                url: page_url,
            });
        }

        let data: crate::models::resource::CfFilesResponse =
            read_json_limited(resp, MAX_METADATA_BYTES).await?;
        let count = data.data.len();
        if count == 0 {
            break;
        }
        offset += count;
        files.extend(data.data);

        if count < CF_PAGE_MAX as usize {
            break;
        }
    }

    files.truncate(max_files);
    if mirror_enabled() {
        for file in &mut files {
            if let Some(url) = file.download_url.take() {
                file.download_url = Some(mirror_file_url(&url));
            }
        }
    }
    VERSION_CACHE.insert(url.clone(), files.clone());
    if let Some(cache) = VERSION_DISK.as_ref() {
        cache.insert(&url, &files);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 官方模式（镜像关闭）下必须回落到官方接口地址，
    /// 镜像模式则走 MCIM——两条路都要能选。
    #[test]
    fn api_base_switches_between_mirror_and_official() {
        with_mirror_enabled(true, || {
            assert_eq!(api_base(), CF_MIRROR_BASE);
        });
        with_mirror_enabled(false, || {
            assert_eq!(api_base(), CF_API_BASE);
        });
    }

    /// 官方接口缺 Key 时会返回 401/403，这两种状态码都要被识别成
    /// 「官方要 Key」，而不是当成普通网络错误。
    #[test]
    fn auth_rejected_covers_401_and_403() {
        assert!(is_auth_rejected(reqwest::StatusCode::UNAUTHORIZED));
        assert!(is_auth_rejected(reqwest::StatusCode::FORBIDDEN));
        assert!(!is_auth_rejected(reqwest::StatusCode::OK));
        assert!(!is_auth_rejected(reqwest::StatusCode::TOO_MANY_REQUESTS));
    }

    /// 镜像模式下不需要 Key：CDN 地址要换成镜像域名，官方域名原样保留。
    #[test]
    fn mirror_file_url_only_rewrites_in_mirror_mode() {
        with_mirror_enabled(true, || {
            assert_eq!(
                mirror_file_url("https://edge.forgecdn.net/files/1/2/x.jar"),
                "https://mod.mcimirror.top/files/1/2/x.jar"
            );
        });
        with_mirror_enabled(false, || {
            assert_eq!(
                mirror_file_url("https://edge.forgecdn.net/files/1/2/x.jar"),
                "https://edge.forgecdn.net/files/1/2/x.jar"
            );
        });
    }
}
