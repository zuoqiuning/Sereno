//! MCIM 国内镜像的地址映射。
//!
//! ## 是什么
//!
//! [MCIM](https://www.mcimirror.top)（Minecraft Mod Info Mirror）是
//! 面向中国大陆用户的 Modrinth / CurseForge 元数据与文件镜像。
//! 官方规则是「**换个域名就行**」：
//!
//! | 官方 | MCIM 镜像 |
//! | :--- | :--- |
//! | `api.modrinth.com` | `mod.mcimirror.top/modrinth` |
//! | `cdn.modrinth.com` | `mod.mcimirror.top` |
//! | `api.curseforge.com` | `mod.mcimirror.top/curseforge` |
//! | `edge.forgecdn.net` | `mod.mcimirror.top` |
//!
//! ## 为什么优先用镜像
//!
//! - **快**：元数据与文件都在国内，延迟比直连官方低一个数量级；
//! - **稳**：官方 API 对匿名请求有配额，镜像替我们挡掉了；
//! - **不需要 API Key**：CurseForge 官方接口强制要 Key，
//!   镜像由服务端统一处理额度——**这是本项目能完全免 Key 的前提**。
//!   （官方接口因此在关掉镜像后不可用，这是刻意的取舍。）
//!
//! ## 兜底
//!
//! 镜像可能因为同步延迟而缺某个刚发布的版本，也可能临时不可用。
//! 因此每个地址都保留官方版本作为第二源：
//! 下载器本来就支持多源回退（见 `download/mirror.rs`），
//! 镜像失败会自动切到官方。
//!
//! **注意**：镜像同步有延迟，**刚发布的 Mod 可能查不到**。
//! 这是刻意的取舍——对国内用户来说，「等几分钟能下到」远好过
//! 「等超时也下不到」。

/// MCIM 镜像的根域名。
pub const MCIM_BASE: &str = "https://mod.mcimirror.top";

/// Modrinth API 的官方根地址。
pub const MODRINTH_API_OFFICIAL: &str = "https://api.modrinth.com";

/// Modrinth API 的镜像根地址。
pub const MODRINTH_API_MIRROR: &str = "https://mod.mcimirror.top/modrinth";

/// Modrinth 文件 CDN 的官方根地址。
pub const MODRINTH_CDN_OFFICIAL: &str = "https://cdn.modrinth.com";

/// CurseForge API 的官方根地址。
pub const CF_API_OFFICIAL: &str = "https://api.curseforge.com/v1";

/// CurseForge API 的镜像根地址。
pub const CF_API_MIRROR: &str = "https://mod.mcimirror.top/curseforge/v1";

/// CurseForge 文件 CDN 的官方根地址。
pub const CF_CDN_OFFICIAL: &str = "https://edge.forgecdn.net";

/// 赞助 MCIM 的地址——鸣谢页与「特别鸣谢」用它。
pub const MCIM_SPONSOR_URL: &str = "https://www.mcimirror.top/sponsor";

/// 把 Modrinth API 地址换成镜像地址。
///
/// 返回 `None` 表示这个地址不是 Modrinth API，调用方应保留原值。
pub fn modrinth_api_mirror(url: &str) -> Option<String> {
    url.strip_prefix(MODRINTH_API_OFFICIAL)
        .map(|rest| format!("{MODRINTH_API_MIRROR}{rest}"))
}

/// 把 Modrinth 文件地址换成镜像地址。
pub fn modrinth_cdn_mirror(url: &str) -> Option<String> {
    url.strip_prefix(MODRINTH_CDN_OFFICIAL)
        .map(|rest| format!("{MCIM_BASE}{rest}"))
}

/// 把 CurseForge API 地址换成镜像地址。
pub fn cf_api_mirror(url: &str) -> Option<String> {
    url.strip_prefix(CF_API_OFFICIAL)
        .map(|rest| format!("{CF_API_MIRROR}{rest}"))
}

/// 把 CurseForge 文件地址换成镜像地址。
///
/// MCIM 只代理 `edge.forgecdn.net`；`mediafilez` 那个域名不在代理范围内，
/// 保持原样（它本身也能直连）。
pub fn cf_cdn_mirror(url: &str) -> Option<String> {
    url.strip_prefix(CF_CDN_OFFICIAL)
        .map(|rest| format!("{MCIM_BASE}{rest}"))
}

/// 判断这个地址是不是 MCIM 镜像。
pub fn is_mcim(url: &str) -> bool {
    url.contains("mcimirror.top")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modrinth_api_to_mirror() {
        assert_eq!(
            modrinth_api_mirror("https://api.modrinth.com/v2/search?limit=10").unwrap(),
            "https://mod.mcimirror.top/modrinth/v2/search?limit=10"
        );
    }

    #[test]
    fn maps_modrinth_cdn_to_mirror() {
        assert_eq!(
            modrinth_cdn_mirror("https://cdn.modrinth.com/data/abc/1/x.jar").unwrap(),
            "https://mod.mcimirror.top/data/abc/1/x.jar"
        );
    }

    #[test]
    fn maps_curseforge_to_mirror() {
        assert_eq!(
            cf_api_mirror("https://api.curseforge.com/v1/mods/123").unwrap(),
            "https://mod.mcimirror.top/curseforge/v1/mods/123"
        );
        assert_eq!(
            cf_cdn_mirror("https://edge.forgecdn.net/files/1/2/x.jar").unwrap(),
            "https://mod.mcimirror.top/files/1/2/x.jar"
        );
    }

    #[test]
    fn leaves_foreign_urls_untouched() {
        assert!(modrinth_api_mirror("https://example.com/x").is_none());
        assert!(modrinth_cdn_mirror("https://example.com/x").is_none());
        assert!(cf_api_mirror("https://example.com/x").is_none());
        assert!(cf_cdn_mirror("https://example.com/x").is_none());
    }

    /// `mediafilez.forgecdn.net` 不在 MCIM 的代理范围内，必须原样保留。
    #[test]
    fn does_not_touch_mediafilez() {
        let url = "https://mediafilez.forgecdn.net/files/1/2/x.jar";
        assert!(cf_cdn_mirror(url).is_none());
    }

    #[test]
    fn recognizes_mcim_urls() {
        assert!(is_mcim("https://mod.mcimirror.top/modrinth/v2/search"));
        assert!(!is_mcim("https://api.modrinth.com/v2/search"));
    }
}
