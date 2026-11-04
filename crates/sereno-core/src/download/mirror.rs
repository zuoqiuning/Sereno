//! 镜像规则：官方 URL 到 BMCLAPI 的映射。

use crate::download::task::SourceUrl;

const BMCLAPI_BASE: &str = "https://bmclapi2.bangbang93.com";
const ALIYUN_MAVEN: &str = "https://maven.aliyun.com/repository/public";

/// 生成一个任务的多源列表（官方优先）。
pub fn build_sources(official_url: &str) -> Vec<SourceUrl> {
    build_sources_with(official_url, false)
}

/// 生成一个任务的多源列表；`prefer_mirror` 为真时镜像排在前面。
///
/// Maven 资源除了 BMCL，再补一个阿里云公共仓库：
/// 部分第三方库（如 plexus-utils）BMCL 偶发 503，阿里云通常可用。
pub fn build_sources_with(official_url: &str, prefer_mirror: bool) -> Vec<SourceUrl> {
    let official = SourceUrl {
        url: official_url.to_string(),
        is_mirror: false,
    };
    let mirror = rewrite_to_mirror(official_url).map(|url| SourceUrl {
        url,
        is_mirror: true,
    });
    let aliyun = rewrite_to_aliyun(official_url).map(|url| SourceUrl {
        url,
        is_mirror: true,
    });

    if mirror.is_none() && aliyun.is_none() {
        return vec![official];
    }

    let mut sources = Vec::new();
    if prefer_mirror {
        if let Some(mirror) = mirror {
            sources.push(mirror);
        }
        if let Some(aliyun) = aliyun {
            sources.push(aliyun);
        }
        sources.push(official);
    } else {
        sources.push(official);
        if let Some(mirror) = mirror {
            sources.push(mirror);
        }
        if let Some(aliyun) = aliyun {
            sources.push(aliyun);
        }
    }
    sources
}

/// Maven 仓库 → 阿里云公共仓库（路径保持不变）。
fn rewrite_to_aliyun(url: &str) -> Option<String> {
    for prefix in [
        "https://libraries.minecraft.net",
        "https://maven.neoforged.net/releases",
        "https://maven.minecraftforge.net",
        "https://maven.fabricmc.net",
    ] {
        if url.starts_with(prefix) {
            return Some(url.replace(prefix, ALIYUN_MAVEN));
        }
    }
    None
}

/// 根据规则尝试重写为镜像 URL。
fn rewrite_to_mirror(url: &str) -> Option<String> {
    if url.starts_with("https://launchermeta.mojang.com") {
        return Some(url.replace("https://launchermeta.mojang.com", BMCLAPI_BASE));
    }
    if url.starts_with("https://piston-data.mojang.com") {
        return Some(url.replace("https://piston-data.mojang.com", BMCLAPI_BASE));
    }
    if url.starts_with("https://libraries.minecraft.net") {
        return Some(url.replace(
            "https://libraries.minecraft.net",
            &format!("{BMCLAPI_BASE}/maven"),
        ));
    }
    // 补全 Assets 散列资源镜像映射：官方站点 -> BMCLAPI /assets
    if url.starts_with("https://maven.fabricmc.net") {
        return Some(url.replace(
            "https://maven.fabricmc.net",
            &format!("{BMCLAPI_BASE}/maven"),
        ));
    }
    if url.starts_with("https://maven.minecraftforge.net") {
        return Some(url.replace(
            "https://maven.minecraftforge.net",
            &format!("{BMCLAPI_BASE}/maven"),
        ));
    }
    if url.starts_with("https://maven.neoforged.net/releases") {
        return Some(url.replace(
            "https://maven.neoforged.net/releases",
            &format!("{BMCLAPI_BASE}/maven"),
        ));
    }
    if url.starts_with("https://resources.download.minecraft.net") {
        return Some(url.replace(
            "https://resources.download.minecraft.net",
            &format!("{BMCLAPI_BASE}/assets"),
        ));
    }
    // Modrinth 与 CurseForge 的文件 CDN → MCIM 国内镜像。
    //
    // 这两个源的元数据与文件都由 MCIM 代理，
    // 走镜像比直连官方快一个数量级（见 `resource::mcim`）。
    if let Some(mirror) = crate::resource::mcim::modrinth_cdn_mirror(url) {
        return Some(mirror);
    }
    if let Some(mirror) = crate::resource::mcim::cf_cdn_mirror(url) {
        return Some(mirror);
    }
    // 其他 CDN 或第三方源暂不重写
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_launchermeta() {
        let sources =
            build_sources("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json");
        assert_eq!(sources.len(), 2);
        assert_eq!(
            sources[1].url,
            format!("{BMCLAPI_BASE}/mc/game/version_manifest_v2.json")
        );
    }

    #[test]
    fn rewrites_libraries() {
        let sources = build_sources(
            "https://libraries.minecraft.net/com/mojang/brigadier/1.3.10/brigadier-1.3.10.jar",
        );
        assert_eq!(sources.len(), 3);
        assert_eq!(
            sources[1].url,
            format!("{BMCLAPI_BASE}/maven/com/mojang/brigadier/1.3.10/brigadier-1.3.10.jar")
        );
        assert_eq!(
            sources[2].url,
            format!("{ALIYUN_MAVEN}/com/mojang/brigadier/1.3.10/brigadier-1.3.10.jar")
        );
    }

    #[test]
    fn rewrites_resources_assets() {
        let sources = build_sources(
            "https://resources.download.minecraft.net/1f/1f34b0827dc6de72edac0befa329ca6377961558",
        );
        assert_eq!(sources.len(), 2);
        assert!(!sources[0].is_mirror);
        assert_eq!(
            sources[1].url,
            format!("{BMCLAPI_BASE}/assets/1f/1f34b0827dc6de72edac0befa329ca6377961558")
        );
    }

    #[test]
    fn leaves_unknown_urls_alone() {
        let sources = build_sources("https://example.com/file.jar");
        assert_eq!(sources.len(), 1);
        assert!(!sources[0].is_mirror);
    }

    #[test]
    fn prefer_mirror_puts_mirror_first() {
        let sources = build_sources_with("https://piston-data.mojang.com/v1/objects/abc", true);
        assert_eq!(sources.len(), 2);
        assert!(sources[0].is_mirror);
        assert_eq!(sources[0].url, format!("{BMCLAPI_BASE}/v1/objects/abc"));
    }
}
