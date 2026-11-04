//! 统一的 `reqwest::Client` 构建。

use crate::models::settings::AppSettings;
use crate::{LauncherError, Result};
use reqwest::{Url, redirect::Policy};
use std::time::Duration;

pub const MAX_METADATA_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn build_client(settings: &AppSettings) -> Result<reqwest::Client> {
    let http = &settings.http;
    // CurseForge 的接口域名与文件 CDN 是否走 MCIM 镜像，由「镜像优先」开关决定。
    crate::resource::curseforge::set_mirror_enabled(settings.download.prefer_mirror);
    let mut builder = reqwest::Client::builder()
        .user_agent(settings.user_agent())
        .https_only(true)
        .connect_timeout(Duration::from_secs(http.connect_timeout_secs))
        .timeout(Duration::from_secs(http.request_timeout_secs))
        .read_timeout(Duration::from_secs(http.read_timeout_secs))
        .pool_max_idle_per_host(64)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(60))
        .redirect(redirect_policy(http.max_redirects as usize));

    if let Some(proxy) = http
        .proxy
        .as_ref()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
    {
        let proxy = reqwest::Proxy::all(proxy).map_err(|e| LauncherError::InvalidConfig {
            key: "http.proxy".into(),
            message: format!("代理地址无效: {e}"),
        })?;
        builder = builder.proxy(proxy);
    }

    builder.build().map_err(|e| LauncherError::Internal {
        message: format!("构建 HTTP 客户端失败: {e}"),
    })
}

fn redirect_policy(max_redirects: usize) -> Policy {
    Policy::custom(move |attempt| {
        if attempt.previous().len() > max_redirects {
            return attempt.error("重定向次数超过上限");
        }

        let previous = attempt.previous().last();
        let next = attempt.url();

        if let Some(previous) = previous {
            let prev_host = previous.host_str().unwrap_or("");
            let next_host = next.host_str().unwrap_or("");

            // 只要起点或终点是以下公开资源域名，无脑放行。
            // 它们在任何跳转中都不会被附加 Authorization 等鉴权头。
            let is_public = |h: &str| {
                h.contains("bmcl")
                    || h.contains("mojang.com")
                    || h.contains("minecraft.net")
                    || h.contains("minecraftforge.net")
                    || h.contains("fabricmc.net")
                    || h.contains("cernet.edu.cn")
                    || h.contains("qlu.edu.cn")
                    || h.contains("aliyun.com")
                    || h.contains("moiu.cn")
                    || h.contains("mocdn.net")
                    || h.contains("curseforge.com")
                    || h.contains("forgecdn.net")
                    || h.contains("mcimirror.top")
                    || h.contains("modrinth.com")
                    || h.contains("cdn.modrinth.com")
            };

            // 只要**跳转链中起点是公开资源域名**，后续跳转到 BMCL CDN
            // （如 7.749333.xyz → *.minio.749333.xyz）也应放行。
            // 这些请求不带 Authorization，跟踪整条链比只看上一跳安全且实用。
            let started_public = attempt
                .previous()
                .iter()
                .any(|url| is_public(url.host_str().unwrap_or("")));

            if is_public(prev_host) || is_public(next_host) || started_public {
                return attempt.follow();
            }

            if !same_origin(previous, next) {
                tracing::warn!(
                    from = %previous,
                    to = %next,
                    "拒绝跟随跨域重定向（无法在重定向中剥离 Authorization 头）"
                );
                return attempt.stop();
            }
        }

        attempt.follow()
    })
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

pub async fn read_limited(mut response: reqwest::Response, max_bytes: u64) -> Result<Vec<u8>> {
    if let Some(len) = response.content_length()
        && len > max_bytes
    {
        return Err(LauncherError::Parse {
            message: format!("响应体过大（{len} > {max_bytes} 字节），已拒绝"),
        });
    }

    let mut buffer = Vec::new();
    while let Some(part) = response.chunk().await.map_err(|e| LauncherError::Io {
        message: format!("读取响应体失败: {e}"),
    })? {
        if buffer.len() as u64 + part.len() as u64 > max_bytes {
            return Err(LauncherError::Parse {
                message: format!("响应体超过上限 {max_bytes} 字节，已中断"),
            });
        }
        buffer.extend_from_slice(&part);
    }

    Ok(buffer)
}

pub async fn read_json_limited<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    max_bytes: u64,
) -> Result<T> {
    let bytes = read_limited(response, max_bytes).await?;
    serde_json::from_slice(&bytes).map_err(|e| LauncherError::Parse {
        message: format!("解析 JSON 响应失败: {e}"),
    })
}
