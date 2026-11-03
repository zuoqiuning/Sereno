//! 单个版本详情 JSON 的解析。
//!
//! 版本详情里除了依赖库，还有主类、资源索引、下载条目、Java 版本要求等信息。
//! 这些字段**大部分是可选的**——不同年代的版本 JSON 字段差别很大，
//! 缺失的一律按 `None` 处理，不要让老版本把解析搞崩。

use crate::models::version::{DownloadInfo, VersionDetail, VersionManifest};
use crate::utils::http::{MAX_METADATA_BYTES, read_limited};
use crate::version::inherits::{MAX_INHERIT_DEPTH, merge_inherits, resolve_with_async};
use crate::{LauncherError, Result};

/// 版本 id 会被拼进目录名，必须先挡住路径分隔符。
fn validate_version_id(id: &str) -> Result<()> {
    if id.is_empty() || id.contains(['/', '\\', ':']) || id.contains("..") {
        return Err(LauncherError::InvalidConfig {
            key: "versionId".into(),
            message: format!("版本 ID 非法: {id}"),
        });
    }
    Ok(())
}

/// 本地版本 JSON 的固定位置：`<instance>/versions/<id>/<id>.json`。
pub fn detail_file(instance_dir: &std::path::Path, id: &str) -> Result<std::path::PathBuf> {
    validate_version_id(id)?;
    Ok(instance_dir
        .join("versions")
        .join(id)
        .join(format!("{id}.json")))
}

/// 读取本地版本 JSON。不存在返回 `Ok(None)`，损坏则报错（不静默重建）。
pub fn read_local(instance_dir: &std::path::Path, id: &str) -> Result<Option<VersionDetail>> {
    let path = detail_file(instance_dir, id)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| LauncherError::Io {
        message: format!("读取版本 JSON 失败 {}: {e}", path.display()),
    })?;
    parse_detail(&raw)
        .map(Some)
        .map_err(|e| LauncherError::Parse {
            message: format!("解析版本 JSON 失败 {}: {e}", path.display()),
        })
}

/// 把（已合并的）版本 JSON 写进实例目录，供离线启动与加载器安装复用。
pub fn save_local(instance_dir: &std::path::Path, detail: &VersionDetail) -> Result<()> {
    let path = detail_file(instance_dir, &detail.id)?;
    let json = serde_json::to_string_pretty(detail).map_err(|e| LauncherError::Parse {
        message: format!("序列化版本 JSON 失败: {e}"),
    })?;
    crate::utils::fs::atomic_write_str(&path, &json)
}

/// 解析单个版本详情（不合并继承链）。
pub fn parse_detail(json: &str) -> Result<VersionDetail> {
    serde_json::from_str(json).map_err(|e| LauncherError::Parse {
        message: format!("解析版本详情失败：{e}"),
    })
}

/// 从网络拉取并解析单个版本详情。
pub async fn fetch_detail(client: &reqwest::Client, url: &str) -> Result<VersionDetail> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("请求版本详情失败：{e}"),
            url: url.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("版本详情服务返回 HTTP {}", response.status()),
            url: url.to_string(),
        });
    }

    let bytes = read_limited(response, MAX_METADATA_BYTES).await?;
    let text = String::from_utf8(bytes).map_err(|e| LauncherError::Parse {
        message: format!("版本详情不是合法的 UTF-8：{e}"),
    })?;

    parse_detail(&text)
}

/// 拉取指定版本的详情，并把 `inheritsFrom` 继承链一并合并。
///
/// 步骤：清单里查版本号 → 拿 URL → 拉详情 → 沿继承链向上取父版本 → 从根往下合并。
/// 返回的是一个**完整**的版本，可以直接拿去算 classpath。
pub async fn resolve_detail(
    client: &reqwest::Client,
    manifest: &VersionManifest,
    id: &str,
) -> Result<VersionDetail> {
    let summary = manifest
        .find(id)
        .ok_or_else(|| LauncherError::VersionNotFound { id: id.to_string() })?;

    let detail = fetch_detail(client, &summary.url).await?;

    resolve_with_async(&detail, |parent_id| async move {
        let parent_summary =
            manifest
                .find(&parent_id)
                .ok_or_else(|| LauncherError::VersionNotFound {
                    id: parent_id.clone(),
                })?;
        fetch_detail(client, &parent_summary.url).await
    })
    .await
}

/// 为某个实例解析一个完整的版本：**本地优先，缺失才联网并缓存**。
///
/// 这是启动/加载器安装唯一应该使用的入口：
/// - 原版版本 JSON 在安装时已写入 `versions/<id>/<id>.json`，断网也能启动；
/// - Forge/NeoForge 安装器写出的版本 JSON 带 `inheritsFrom`，
///   父版本同样从本地读取，逐层合并；
/// - 本地缺父版本时才走网络，并把结果缓存下来，下次离线可用。
pub async fn resolve_for_instance(
    client: &reqwest::Client,
    manifest: Option<&VersionManifest>,
    instance_dir: &std::path::Path,
    id: &str,
) -> Result<VersionDetail> {
    resolve_local_chain(client, manifest, instance_dir, id, 0).await
}

async fn resolve_local_chain(
    client: &reqwest::Client,
    manifest: Option<&VersionManifest>,
    instance_dir: &std::path::Path,
    id: &str,
    depth: usize,
) -> Result<VersionDetail> {
    if depth > MAX_INHERIT_DEPTH {
        return Err(LauncherError::Parse {
            message: format!("版本 {id} 的继承链超过 {MAX_INHERIT_DEPTH} 层，疑似循环引用"),
        });
    }

    if let Some(local) = read_local(instance_dir, id)? {
        let Some(parent_id) = local.inherits_from.clone() else {
            return Ok(local);
        };
        let parent = Box::pin(resolve_local_chain(
            client,
            manifest,
            instance_dir,
            &parent_id,
            depth + 1,
        ))
        .await?;
        return Ok(merge_inherits(&parent, &local));
    }

    // 只有本地缺文件时才联网：完整实例在断网状态下也能启动。
    let fetched;
    let manifest = match manifest {
        Some(m) => m,
        None => {
            fetched = crate::version::manifest::fetch_manifest(client).await?;
            &fetched
        }
    };

    let detail = resolve_detail(client, manifest, id).await?;
    save_local(instance_dir, &detail)?;
    Ok(detail)
}

/// 版本详情的便捷访问。
impl VersionDetail {
    /// 该版本要求的 Java 主版本号。
    ///
    /// 权威来源就是版本 JSON 里的 `javaVersion.majorVersion`——
    /// **不要**在代码里写「1.17 → 16」这种映射表，
    /// 加载器版本会通过 `inheritsFrom` 继承这个字段，映射表在快照版本、
    /// 整合包自带 JRE 上都会失准。
    pub fn required_java_major(&self) -> Option<u32> {
        self.java_version.as_ref().map(|j| j.major_version)
    }

    /// 客户端 jar 的下载条目。
    pub fn client_download(&self) -> Option<&DownloadInfo> {
        self.downloads.as_ref().map(|d| &d.client)
    }

    /// 是否声明了继承关系。
    pub fn has_inherits(&self) -> bool {
        self.inherits_from.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DETAIL: &str = r#"{
        "id": "1.21.1",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "minimumLauncherVersion": 21,
        "releaseTime": "2024-08-08T12:00:00+00:00",
        "time": "2024-08-08T12:00:00+00:00",
        "assets": "12",
        "assetIndex": {
            "id": "12",
            "sha1": "asset-sha",
            "size": 1234,
            "totalSize": 5678,
            "url": "https://example.invalid/asset-index.json"
        },
        "downloads": {
            "client": { "sha1": "client-sha", "size": 100, "url": "https://example.invalid/client.jar" },
            "server": { "sha1": "server-sha", "size": 200, "url": "https://example.invalid/server.jar" }
        },
        "libraries": [
            {
                "name": "org.lwjgl:lwjgl:3.3.3",
                "downloads": { "artifact": { "sha1": "x", "size": 1, "url": "https://example.invalid/lwjgl.jar" } },
                "rules": [ { "action": "allow", "os": { "name": "windows" } } ]
            }
        ],
        "logging": {
            "client": { "argument": "-Dlog4j.configurationFile=${path}", "file": { "id": "client-logging", "sha1": "s", "size": 1, "url": "https://example.invalid/log.xml" } }
        },
        "javaVersion": { "component": "java-runtime-gamma", "majorVersion": 21 }
    }"#;

    #[test]
    fn parses_detail() {
        let detail = parse_detail(DETAIL).unwrap();
        assert_eq!(detail.id, "1.21.1");
        assert_eq!(
            detail.main_class.as_deref(),
            Some("net.minecraft.client.main.Main")
        );
        assert_eq!(detail.libraries.len(), 1);
        assert_eq!(detail.required_java_major(), Some(21));
        assert_eq!(detail.client_download().unwrap().size, 100);
        assert!(detail.asset_index.is_some());
        assert!(detail.logging.is_some());
        assert!(!detail.has_inherits());
    }

    #[test]
    fn minimal_detail_is_accepted() {
        // 只有一个 id 与 type 也算合法——老版本与加载器注入的版本可能非常精简
        let detail = parse_detail(r#"{"id":"x","type":"release"}"#).unwrap();
        assert_eq!(detail.id, "x");
        assert!(detail.libraries.is_empty());
        assert_eq!(detail.required_java_major(), None);
        assert!(detail.client_download().is_none());
    }

    #[test]
    fn inherited_detail_parses() {
        let detail =
            parse_detail(r#"{"id":"child","type":"release","inheritsFrom":"parent"}"#).unwrap();
        assert_eq!(detail.inherits_from.as_deref(), Some("parent"));
        assert!(detail.has_inherits());
    }

    #[test]
    fn malformed_detail_is_reported() {
        let err = parse_detail("{ oops").unwrap_err();
        assert_eq!(err.code(), "parse");
    }
}
