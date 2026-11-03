//! 版本清单（`version_manifest_v2.json`）解析与查询。
//!
//! ## 为什么清单是启动器的第一份数据
//!
//! 启动器做任何事之前都得先知道「有哪些版本可装」——清单是唯一的权威来源。
//! 因此这里的解析**必须容错**：一份坏掉的清单不该让整个应用起不来，
//! 但也不该悄悄放过异常数据。
//!
//! ## 数据来源
//!
//! 官方地址：<https://launchermeta.mojang.com/mc/game/version_manifest_v2.json>
//! 国内网络下可能很慢或超时，**镜像策略**（见《下载器与镜像策略》）
//! 负责改写 URL，本模块只管解析。

use crate::models::version::{VersionManifest, VersionSummary, VersionType};
use crate::utils::http::{MAX_METADATA_BYTES, read_limited};
use crate::{LauncherError, Result};

/// Mojang 官方版本清单地址。
pub const VERSION_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

/// 解析版本清单 JSON。
///
/// 失败时返回 [`LauncherError::Parse`]，错误文案里带上原因——
/// 「清单解析失败」这种话对用户没有任何帮助。
pub fn parse_manifest(json: &str) -> Result<VersionManifest> {
    serde_json::from_str(json).map_err(|e| LauncherError::Parse {
        message: format!("解析版本清单失败：{e}"),
    })
}

/// 从网络拉取并解析版本清单。
///
/// **必须用 [`crate::utils::http::build_client`] 造出来的 client**，
/// 这样超时、重定向策略、UA、代理都统一。
pub async fn fetch_manifest(client: &reqwest::Client) -> Result<VersionManifest> {
    let response = client.get(VERSION_MANIFEST_URL).send().await.map_err(|e| {
        LauncherError::DownloadFailed {
            message: format!("请求版本清单失败：{e}"),
            url: VERSION_MANIFEST_URL.to_string(),
        }
    })?;

    if !response.status().is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("版本清单服务返回 HTTP {}", response.status()),
            url: VERSION_MANIFEST_URL.to_string(),
        });
    }

    let bytes = read_limited(response, MAX_METADATA_BYTES).await?;
    let text = String::from_utf8(bytes).map_err(|e| LauncherError::Parse {
        message: format!("版本清单不是合法的 UTF-8：{e}"),
    })?;

    parse_manifest(&text)
}

/// 版本清单的查询方法。
///
/// 放在 `version` 域而不是 `models` 域：`models/` 只定义数据、不含行为。
/// Rust 允许在同一个 crate 的任意模块里给类型加固有方法，
/// 因此约定不会被破坏。
impl VersionManifest {
    /// 按版本号查找摘要。
    pub fn find(&self, id: &str) -> Option<&VersionSummary> {
        self.versions.iter().find(|v| v.id == id)
    }

    /// 最新正式版的摘要。
    ///
    /// 找不到时回退到 `versions` 里第一个正式版——
    /// 清单的 `latest` 字段偶尔会指向一个已被下架的版本。
    pub fn latest_release(&self) -> Option<&VersionSummary> {
        if let Some(found) = self.find(&self.latest.release) {
            return Some(found);
        }
        self.versions.iter().find(|v| v.version_type.is_release())
    }

    /// 最新快照的摘要。
    pub fn latest_snapshot(&self) -> Option<&VersionSummary> {
        if let Some(found) = self.find(&self.latest.snapshot) {
            return Some(found);
        }
        self.versions.iter().find(|v| v.version_type.is_snapshot())
    }

    /// 按类型筛选版本。
    pub fn filter_by_type(&self, version_type: VersionType) -> Vec<&VersionSummary> {
        self.versions
            .iter()
            .filter(|v| v.version_type == version_type)
            .collect()
    }

    /// 全部正式版（清单本身已按时间倒序，这里保持原序）。
    pub fn releases(&self) -> Vec<&VersionSummary> {
        self.filter_by_type(VersionType::Release)
    }

    /// 全部快照。
    pub fn snapshots(&self) -> Vec<&VersionSummary> {
        self.filter_by_type(VersionType::Snapshot)
    }

    /// 版本总数。
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    /// 清单是否为空。
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "latest": { "release": "1.21.1", "snapshot": "24w33a" },
        "versions": [
            {
                "id": "1.21.1",
                "type": "release",
                "url": "https://example.invalid/1.21.1.json",
                "time": "2024-08-08T12:00:00+00:00",
                "releaseTime": "2024-08-08T12:00:00+00:00",
                "sha1": "aaaa",
                "complianceLevel": 1
            },
            {
                "id": "24w33a",
                "type": "snapshot",
                "url": "https://example.invalid/24w33a.json",
                "time": "2024-08-14T00:00:00+00:00",
                "releaseTime": "2024-08-14T00:00:00+00:00",
                "sha1": "bbbb",
                "complianceLevel": 1
            },
            {
                "id": "b1.7.3",
                "type": "old_beta",
                "url": "https://example.invalid/b1.7.3.json",
                "time": "2012-08-30T00:00:00+00:00",
                "releaseTime": "2012-08-30T00:00:00+00:00",
                "sha1": "cccc"
            }
        ]
    }"#;

    #[test]
    fn parses_manifest() {
        let manifest = parse_manifest(SAMPLE).unwrap();
        assert_eq!(manifest.len(), 3);
        assert_eq!(manifest.latest.release, "1.21.1");
        assert_eq!(manifest.latest.snapshot, "24w33a");
    }

    #[test]
    fn finds_by_id_and_type() {
        let manifest = parse_manifest(SAMPLE).unwrap();

        assert_eq!(
            manifest.find("1.21.1").unwrap().version_type,
            VersionType::Release
        );
        assert_eq!(
            manifest.find("24w33a").unwrap().version_type,
            VersionType::Snapshot
        );
        assert_eq!(
            manifest.find("b1.7.3").unwrap().version_type,
            VersionType::OldBeta
        );
        assert!(manifest.find("nope").is_none());
    }

    #[test]
    fn separates_releases_and_snapshots() {
        let manifest = parse_manifest(SAMPLE).unwrap();
        assert_eq!(manifest.releases().len(), 1);
        assert_eq!(manifest.snapshots().len(), 1);
    }

    #[test]
    fn latest_falls_back_when_pointer_is_dangling() {
        let mut manifest = parse_manifest(SAMPLE).unwrap();
        // 模拟 latest 指向一个已被下架的版本
        manifest.latest.release = "9.9.9".into();

        let latest = manifest.latest_release().expect("应回退到列表里的正式版");
        assert_eq!(latest.id, "1.21.1");
    }

    #[test]
    fn unknown_version_type_does_not_break_parsing() {
        let json = r#"{
            "latest": { "release": "x", "snapshot": "y" },
            "versions": [
                { "id": "x", "type": "brand_new_kind", "url": "u", "time": "t", "releaseTime": "t", "sha1": "s" }
            ]
        }"#;

        let manifest = parse_manifest(json).expect("未知类型不该让解析失败");
        assert_eq!(
            manifest.find("x").unwrap().version_type,
            VersionType::Unknown
        );
    }

    #[test]
    fn malformed_json_is_reported() {
        let err = parse_manifest("{ not json").unwrap_err();
        assert_eq!(err.code(), "parse");
    }
}
