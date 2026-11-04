//! 社区资源搜索：单源分页、双源按页聚合与内存页缓存。
//!
//! **单源**把 `offset` 直接交给站点，能一直翻到最后一页。
//!
//! **双源聚合**按页取窗口：第 N 页同时取两个来源的第 N 个窗口再合并排序。
//! 这样页码可以任意跳转，总数是两个站点的真实总数（十几万条也能显示），
//! 代价是跨页的全局顺序不如「抓完整前缀」严格；但每一页内部都按下载量排序，
//! 且不必为了第 100 页去抓十万条元数据。
//!
//! **缓存**只存在于进程内存（10 分钟），关闭软件自动消失；
//! 前端预取后续页时也是在填充这份缓存。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Duration;

use crate::models::resource::{
    ModProject, ResourceSearchPage, ResourceSearchQuery, ResourceSort, ResourceSource,
};
use crate::resource::{curseforge, modrinth};
use crate::utils::disk_cache::DiskCache;
use crate::utils::ttl_cache::TtlCache;
use crate::{LauncherError, Result};

const DEFAULT_PAGE_SIZE: u32 = 20;
const MAX_PAGE_SIZE: u32 = 50;
const CACHE_TTL: Duration = Duration::from_secs(900);
const TOTALS_TTL: Duration = Duration::from_secs(1800);

static PAGE_CACHE: LazyLock<TtlCache<String, ResourceSearchPage>> =
    LazyLock::new(|| TtlCache::new(CACHE_TTL, 512));
static TOTALS_CACHE: LazyLock<TtlCache<String, Totals>> =
    LazyLock::new(|| TtlCache::new(TOTALS_TTL, 128));
/// 磁盘缓存：跨重启复用最近 15 分钟的搜索页，减少启动 / 重复查询的回源。
static PAGE_DISK: LazyLock<Option<DiskCache>> = LazyLock::new(|| {
    DiskCache::new_with_cap("resource/pages", CACHE_TTL, 512, 32 * 1024 * 1024).ok()
});
static TOTALS_DISK: LazyLock<Option<DiskCache>> = LazyLock::new(|| {
    DiskCache::new_with_cap("resource/totals", TOTALS_TTL, 128, 8 * 1024 * 1024).ok()
});
/// 同一个 key 的并发请求只允许一个真正发起网络请求，其余等待后直接读缓存。
static IN_FLIGHT: LazyLock<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn flight_lock(key: &str) -> Arc<tokio::sync::Mutex<()>> {
    let Ok(mut flights) = IN_FLIGHT.lock() else {
        return Arc::new(tokio::sync::Mutex::new(()));
    };

    // 懒惰清理：只有条目数超过 128 才回收失效的 Weak。
    // 平时多留几百个键不影响正确性（取不到就重建），不值得每次请求都全表扫一遍。
    if flights.len() > 128 {
        flights.retain(|_, weak| weak.strong_count() > 0);
    }
    if let Some(existing) = flights.get(key).and_then(Weak::upgrade) {
        return existing;
    }

    let lock = Arc::new(tokio::sync::Mutex::new(()));
    flights.insert(key.to_string(), Arc::downgrade(&lock));
    lock
}

/// 中文常用叫法 → 站点上的英文项目名。
///
/// 两个站点都只索引英文标题，中文输入直接搜会得到空结果。
/// 只在**整词**命中时替换：把「钠」换成 sodium 是对的，
/// 但对「钠优化」这类组合词做部分替换只会拼出搜不到的查询。
const KEYWORD_ALIASES: &[(&str, &str)] = &[
    ("高清修复", "optifine"),
    ("钠", "sodium"),
    ("锂", "lithium"),
    ("铟", "indium"),
    ("机械动力", "create"),
    ("匠魂", "tinkers construct"),
    ("暮色森林", "twilight forest"),
    ("植物魔法", "botania"),
    ("星辉魔法", "astral sorcery"),
    ("热力膨胀", "thermal expansion"),
    ("我的世界", "minecraft"),
];

/// 把完全匹配的中文关键词换成英文项目名，其它输入原样返回。
fn translate_keyword(query: &str) -> String {
    let trimmed = query.trim();
    KEYWORD_ALIASES
        .iter()
        .find(|(chinese, _)| *chinese == trimmed)
        .map_or_else(|| query.to_string(), |(_, english)| english.to_string())
}

pub async fn search(
    client: &reqwest::Client,
    query: &ResourceSearchQuery,
) -> Result<ResourceSearchPage> {
    let page = query.page.max(1);
    let page_size = if query.page_size == 0 {
        DEFAULT_PAGE_SIZE
    } else {
        query.page_size.clamp(1, MAX_PAGE_SIZE)
    };

    let mut normalized = query.clone();
    normalized.page = page;
    normalized.page_size = page_size;
    normalized.query = translate_keyword(&query.query);
    let cache_key = format!(
        "v3:{}",
        serde_json::to_string(&normalized).unwrap_or_default()
    );

    if let Some(cached) = PAGE_CACHE.get(&cache_key) {
        return Ok(cached);
    }
    if let Some(cached) = PAGE_DISK
        .as_ref()
        .and_then(|cache| cache.get::<ResourceSearchPage>(&cache_key))
    {
        PAGE_CACHE.insert(cache_key.clone(), cached.clone());
        return Ok(cached);
    }

    // 预取与用户翻页可能同时请求同一页；只让第一个真正走网络。
    let flight = flight_lock(&cache_key);
    let _guard = flight.lock().await;
    if let Some(cached) = PAGE_CACHE.get(&cache_key) {
        return Ok(cached);
    }

    let result = match normalized.source {
        ResourceSource::Modrinth => search_modrinth(client, &normalized, page, page_size).await,
        ResourceSource::Curseforge => search_curseforge(client, &normalized, page, page_size).await,
        ResourceSource::Aggregate => search_aggregate(client, &normalized, page, page_size).await,
    };

    if let Ok(page_data) = &result {
        PAGE_CACHE.insert(cache_key.clone(), page_data.clone());
        if let Some(cache) = PAGE_DISK.as_ref() {
            cache.insert(&cache_key, page_data);
        }
    }
    result
}

async fn search_modrinth(
    client: &reqwest::Client,
    query: &ResourceSearchQuery,
    page: u32,
    page_size: u32,
) -> Result<ResourceSearchPage> {
    let offset = (page - 1) * page_size;
    let result = modrinth::search(client, query, offset, page_size).await?;
    let page_count = result.total.div_ceil(page_size).max(1);
    Ok(ResourceSearchPage {
        items: result.items,
        total: result.total,
        page_count,
        page,
        page_size,
        has_more: page < page_count,
        warnings: vec![],
    })
}

async fn search_curseforge(
    client: &reqwest::Client,
    query: &ResourceSearchQuery,
    page: u32,
    page_size: u32,
) -> Result<ResourceSearchPage> {
    let offset = (page - 1) * page_size;
    let result = curseforge::search(client, query, offset, page_size).await?;
    let page_count = result.total.div_ceil(page_size).max(1);
    Ok(ResourceSearchPage {
        items: result.items,
        total: result.total,
        page_count,
        page,
        page_size,
        has_more: page < page_count,
        warnings: vec![],
    })
}

/// 两个来源的全量命中数。缓存 30 分钟，翻页时不用重复探测。
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
struct Totals {
    mr: u32,
    cf: u32,
}

struct TotalsResult {
    totals: Totals,
    mr_error: Option<LauncherError>,
    cf_error: Option<LauncherError>,
    any_success: bool,
}

fn totals_key(query: &ResourceSearchQuery) -> String {
    let mut key = query.clone();
    key.source = ResourceSource::Modrinth;
    key.page = 1;
    key.page_size = 1;
    format!("v3:{}", serde_json::to_string(&key).unwrap_or_default())
}

/// 探测两个来源的总命中数。只取 1 条元数据，代价很小；
/// 探测到之后同一组筛选条件的翻页都按比例分配窗口，不必再探测。
async fn resolve_totals(client: &reqwest::Client, query: &ResourceSearchQuery) -> TotalsResult {
    let key = totals_key(query);
    if let Some(totals) = TOTALS_CACHE.get(&key) {
        return TotalsResult {
            totals,
            mr_error: None,
            cf_error: None,
            any_success: true,
        };
    }
    if let Some(totals) = TOTALS_DISK
        .as_ref()
        .and_then(|cache| cache.get::<Totals>(&key))
    {
        TOTALS_CACHE.insert(key.clone(), totals);
        return TotalsResult {
            totals,
            mr_error: None,
            cf_error: None,
            any_success: true,
        };
    }

    // 不同页可能同时探测同一组总数，这里同样只放行一个请求。
    let flight = flight_lock(&format!("totals:{key}"));
    let _guard = flight.lock().await;
    if let Some(totals) = TOTALS_CACHE.get(&key) {
        return TotalsResult {
            totals,
            mr_error: None,
            cf_error: None,
            any_success: true,
        };
    }

    let mr_future = modrinth::search(client, query, 0, 1);
    let cf_future = async {
        // 官方接口需要 API Key，而本项目不再携带：官方模式下 CF 一律不可用，
        // 不必发一次注定被拒的请求。具体提示由 `source_warnings` 统一给出。
        if !curseforge::mirror_enabled() {
            return Ok(None);
        }
        curseforge::search(client, query, 0, 1).await.map(Some)
    };
    let (mr_result, cf_result) = tokio::join!(mr_future, cf_future);

    let mut result = TotalsResult {
        totals: Totals { mr: 0, cf: 0 },
        mr_error: None,
        cf_error: None,
        any_success: false,
    };
    let mut mr_ok = false;
    let mut cf_ok = false;

    match mr_result {
        Ok(page) => {
            mr_ok = true;
            result.any_success = true;
            result.totals.mr = page.total;
        }
        Err(error) => result.mr_error = Some(error),
    }
    match cf_result {
        Ok(Some(page)) => {
            cf_ok = true;
            result.any_success = true;
            result.totals.cf = page.total;
        }
        Ok(None) => {}
        Err(error) => result.cf_error = Some(error),
    }

    // 两个来源都成功（或官方模式本就不可用）时才缓存总数：
    // 否则一次网络抖动会让接下来 10 分钟的聚合结果永久缺一边。
    if mr_ok && (cf_ok || !curseforge::mirror_enabled()) {
        TOTALS_CACHE.insert(key.clone(), result.totals);
        if let Some(cache) = TOTALS_DISK.as_ref() {
            cache.insert(&key, &result.totals);
        }
    }

    result
}

async fn search_aggregate(
    client: &reqwest::Client,
    query: &ResourceSearchQuery,
    page: u32,
    page_size: u32,
) -> Result<ResourceSearchPage> {
    let mut totals_result = resolve_totals(client, query).await;

    // 官方模式（镜像已关闭）下 CF 完全没有 Key 可用，因此 `resolve_totals`
    // 不会去查 CF，CF 一侧的总数恒为 0；真正的提示由 `source_warnings` 给出。

    if !totals_result.any_success {
        return Err(totals_result
            .mr_error
            .or(totals_result.cf_error)
            .unwrap_or_else(|| LauncherError::Internal {
                message: "聚合搜索的两个来源均不可用".into(),
            }));
    }

    let totals = totals_result.totals;
    let total = totals.mr.saturating_add(totals.cf);
    let page_count = total.div_ceil(page_size).max(1);
    if page > page_count {
        return Ok(ResourceSearchPage {
            items: vec![],
            total,
            page_count,
            page,
            page_size,
            has_more: false,
            warnings: source_warnings(&totals_result.mr_error, &totals_result.cf_error),
        });
    }

    // 当前页在「合并后的全量列表」中的区间 [start, end)。
    let start = (page - 1) * page_size;
    let end = (page * page_size).min(total);
    // 按来源占比分配窗口：大来源多取，小来源少取，总和约等于每页条数。
    let (mr_offset, mr_limit) = source_window(total, totals.mr, start, end);
    let (cf_offset, cf_limit) = source_window(total, totals.cf, start, end);

    let mr_future = async {
        if mr_limit == 0 {
            return Ok(None);
        }
        modrinth::search(client, query, mr_offset, mr_limit)
            .await
            .map(Some)
    };
    let cf_future = async {
        if cf_limit == 0 {
            return Ok(None);
        }
        curseforge::search(client, query, cf_offset, cf_limit)
            .await
            .map(Some)
    };
    let (mr_result, cf_result) = tokio::join!(mr_future, cf_future);

    let mut mr_error = totals_result.mr_error.take();
    let mut cf_error = totals_result.cf_error.take();
    let mut items: Vec<ModProject> = Vec::new();

    match mr_result {
        Ok(Some(result)) => items.extend(result.items),
        Ok(None) => {}
        Err(error) => mr_error = Some(error),
    }
    match cf_result {
        Ok(Some(result)) => items.extend(result.items),
        Ok(None) => {}
        Err(error) => cf_error = Some(error),
    }

    if items.is_empty() && mr_error.is_some() && cf_error.is_some() {
        return Err(mr_error.unwrap_or_else(|| LauncherError::Internal {
            message: "聚合搜索的两个来源均不可用".into(),
        }));
    }

    sort_desc(&mut items, query.sort);
    let items: Vec<ModProject> = dedupe(items).into_iter().take(page_size as usize).collect();

    Ok(ResourceSearchPage {
        items,
        total,
        page_count,
        page,
        page_size,
        has_more: page < page_count,
        warnings: source_warnings(&mr_error, &cf_error),
    })
}

/// 把合并后的全局区间按来源总量占比映射到该来源的 offset/limit。
fn source_window(total: u32, own: u32, start: u32, end: u32) -> (u32, u32) {
    if total == 0 || own == 0 || start >= total {
        return (0, 0);
    }

    let total = u64::from(total);
    let own = u64::from(own);
    // start / end 用同一个取整方式，保证相邻页的窗口首尾相接：
    // 既不会跨页重复，也不会漏掉某个条目。
    let offset = (u64::from(start) * own / total) as u32;
    let end = (u64::from(end) * own / total).min(own) as u32;
    let limit = (end.saturating_sub(offset)).min(MAX_PAGE_SIZE);
    (offset.min(end), limit)
}

fn source_warnings(
    mr_error: &Option<LauncherError>,
    cf_error: &Option<LauncherError>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(error) = mr_error {
        warnings.push(format!(
            "Modrinth 暂时不可用（{error}），仅显示 CurseForge 结果"
        ));
    }
    if let Some(error) = cf_error {
        warnings.push(format!(
            "CurseForge 暂时不可用（{error}），仅显示 Modrinth 结果"
        ));
    }
    if !curseforge::mirror_enabled() {
        warnings.push("CurseForge 官方接口需要 API Key，已仅显示 Modrinth 结果；请在设置中开启「优先使用国内镜像」".into());
    }
    warnings
}

fn sort_desc(items: &mut [ModProject], sort: ResourceSort) {
    match sort {
        ResourceSort::Downloads => {
            items.sort_by_key(|item| std::cmp::Reverse(item.downloads));
        }
        ResourceSort::Updated => items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
    }
}

/// 同一工程在 Modrinth / CurseForge 上标题或 slug 基本一致。
/// 按「归一化后的标题 / slug」去重，保留排序后更靠前（下载量更高）的那一条。
fn dedupe(items: Vec<ModProject>) -> Vec<ModProject> {
    let mut seen = HashSet::new();
    let mut unique = Vec::with_capacity(items.len());

    for item in items {
        let title = normalize(&item.title);
        let slug = normalize(&item.slug);
        let duplicated = (!title.is_empty() && seen.contains(&title))
            || (!slug.is_empty() && seen.contains(&slug));
        if duplicated {
            continue;
        }
        if !title.is_empty() {
            seen.insert(title);
        }
        if !slug.is_empty() {
            seen.insert(slug);
        }
        unique.push(item);
    }

    unique
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(source: ResourceSource, title: &str, slug: &str, downloads: u64) -> ModProject {
        ModProject {
            id: slug.to_string(),
            slug: slug.to_string(),
            title: title.to_string(),
            description: String::new(),
            icon_url: None,
            downloads,
            follows: 0,
            author: "author".into(),
            source,
            updated_at: None,
        }
    }

    #[test]
    fn dedupe_keeps_the_more_downloaded_copy() {
        let mut items = vec![
            project(
                ResourceSource::Curseforge,
                "Fabric API",
                "fabric-api",
                200_000_000,
            ),
            project(
                ResourceSource::Modrinth,
                "Fabric API",
                "fabric-api",
                295_000_000,
            ),
            project(ResourceSource::Modrinth, "Sodium", "sodium", 150_000_000),
        ];
        sort_desc(&mut items, ResourceSort::Downloads);
        let unique = dedupe(items);

        assert_eq!(unique.len(), 2);
        assert_eq!(unique[0].source, ResourceSource::Modrinth);
        assert_eq!(unique[0].slug, "fabric-api");
    }

    #[test]
    fn source_windows_cover_each_source_without_gaps() {
        // 一个来源占全量的 40%，每页 20 条；它的窗口应当连续推进到它自己的末尾。
        let total = 100;
        let own = 40;
        let page_size = 20;
        let mut expected_offset = 0;

        for page in 1..=5 {
            let start = (page - 1) * page_size;
            let end = (page * page_size).min(total);
            let (offset, limit) = source_window(total, own, start, end);
            assert_eq!(offset, expected_offset, "第 {page} 页出现空洞");
            expected_offset += limit;
        }

        assert_eq!(expected_offset, own);
    }

    #[test]
    fn chinese_keyword_is_translated_only_as_a_whole_word() {
        assert_eq!(translate_keyword("钠"), "sodium");
        assert_eq!(translate_keyword(" 机械动力 "), "create");
        assert_eq!(
            translate_keyword("钠优化"),
            "钠优化",
            "组合词不做部分替换，否则会拼出搜不到的查询"
        );
        assert_eq!(translate_keyword("sodium"), "sodium");
    }

    #[test]
    fn updated_sort_puts_newest_first() {
        let mut items = vec![
            ModProject {
                updated_at: Some("2024-01-01T00:00:00Z".into()),
                ..project(ResourceSource::Modrinth, "A", "a", 1)
            },
            ModProject {
                updated_at: Some("2025-01-01T00:00:00Z".into()),
                ..project(ResourceSource::Modrinth, "B", "b", 2)
            },
        ];
        sort_desc(&mut items, ResourceSort::Updated);

        assert_eq!(items[0].title, "B");
    }

    /// 官方模式（镜像关闭）下 CF 一律不可用，必须明确告诉用户
    /// 「开回镜像」，而不是只显示 Modrinth 结果却不说原因。
    #[test]
    fn official_mode_warns_to_enable_mirror() {
        curseforge::with_mirror_enabled(false, || {
            let warnings = source_warnings(&None, &None);
            assert!(
                warnings.iter().any(|w| w.contains("优先使用国内镜像")),
                "官方模式缺少开启镜像的提示: {warnings:?}"
            );
        });

        curseforge::with_mirror_enabled(true, || {
            assert!(source_warnings(&None, &None).is_empty());
        });
    }

    /// 单个来源出错时，警告要说清是哪个来源挂了、还剩哪个来源可用。
    #[test]
    fn source_warnings_name_the_failing_source() {
        let warnings = source_warnings(
            &None,
            &Some(LauncherError::DownloadFailed {
                message: "挂了".into(),
                url: "https://api.curseforge.com/v1".into(),
            }),
        );
        assert!(
            warnings.iter().any(|w| w.contains("CurseForge")),
            "警告里应指出 CurseForge 不可用: {warnings:?}"
        );
    }
}
