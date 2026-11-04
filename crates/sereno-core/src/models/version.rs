//! 版本相关数据结构。

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Snapshot,
    #[serde(rename = "old_beta")]
    OldBeta,
    #[serde(rename = "old_alpha")]
    OldAlpha,
    #[default]
    Unknown,
}

impl VersionType {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "release" => Self::Release,
            "snapshot" => Self::Snapshot,
            "old_beta" => Self::OldBeta,
            "old_alpha" => Self::OldAlpha,
            _ => Self::Unknown,
        }
    }
    pub fn is_release(self) -> bool {
        matches!(self, Self::Release)
    }
    pub fn is_snapshot(self) -> bool {
        matches!(self, Self::Snapshot)
    }
}

impl<'de> Deserialize<'de> for VersionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw))
    }
}

fn de_size<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = i64::deserialize(deserializer).unwrap_or(0);
    Ok(raw.clamp(0, u32::MAX as i64) as u32)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub url: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub release_time: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub compliance_level: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[specta(type = u32)]
    #[serde(default, deserialize_with = "de_size")]
    pub size: u32,
    /// Mojang 声明的库内相对路径（`group/artifact/version/file.jar`）。
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Downloads {
    pub client: DownloadInfo,
    #[serde(default)]
    pub server: Option<DownloadInfo>,
    /// Mojang 的字段名是 `client_mappings`（snake_case），不能跟着 camelCase 走。
    #[serde(default, rename = "client_mappings")]
    pub client_mappings: Option<DownloadInfo>,
    #[serde(default, rename = "server_mappings")]
    pub server_mappings: Option<DownloadInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub id: String,
    #[serde(default)]
    pub sha1: String,
    #[specta(type = u32)]
    #[serde(default, deserialize_with = "de_size")]
    pub size: u32,
    #[specta(type = u32)]
    #[serde(default, deserialize_with = "de_size")]
    pub total_size: u32,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OsSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsSpec>,
    /// 官方启动器用来控制 demo、快速进入世界、自定义分辨率等参数。
    #[serde(default)]
    pub features: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<DownloadInfo>,
    #[serde(default)]
    pub classifiers: Option<BTreeMap<String, DownloadInfo>>,
}

/// 旧版（≤1.12.2）的 `natives` 声明。
///
/// 官方 JSON 里它是一个 **map**，键是操作系统名，值是构件分类前缀：
///
/// ```json
/// "natives": { "linux": "natives-linux", "osx": "natives-osx", "windows": "natives-windows" }
/// ```
///
/// 历史上这里被写成单字段结构体 `{ classifier: String }`，于是
/// 1.7.10 / 1.8.9 / 1.12.2 这些**整合包最常用**的版本在反序列化时
/// 直接报 `missing field 'classifier'`，整个版本 JSON 解析失败、
/// 游戏装不上。改为 map 之后与官方结构一致。
pub type NativesMap = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Extract {
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
    #[serde(default)]
    pub include: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub name: String,
    /// 只声明 Maven 仓库地址、没有 `downloads.artifact` 的库（部分加载器注入的依赖）。
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub natives: Option<NativesMap>,
    #[serde(default)]
    pub extract: Option<Extract>,
    #[serde(default)]
    pub client: Option<bool>,
    #[serde(default)]
    pub server: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    #[serde(default)]
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingArgument>,
    #[serde(default)]
    pub server: Option<LoggingArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LoggingArgument {
    #[serde(default)]
    pub argument: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub file: Option<LoggingFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LoggingFile {
    pub id: String,
    #[serde(default)]
    pub sha1: String,
    #[specta(type = u32)]
    #[serde(default, deserialize_with = "de_size")]
    pub size: u32,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionDetail {
    pub id: String,
    #[serde(default, rename = "type")]
    pub version_type: VersionType,
    #[serde(default)]
    pub main_class: Option<String>,
    #[serde(default)]
    pub minimum_launcher_version: Option<u32>,
    #[serde(default)]
    pub release_time: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub asset_index: Option<AssetIndex>,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub logging: Option<Logging>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub inherits_from: Option<String>,

    /// 新版启动参数 (1.13+)。
    /// 因为内部包含复杂的动态类型，前端根本不需要，用 String 欺骗 Specta 规避导出报错。
    #[specta(type = String)]
    #[serde(default)]
    pub arguments: Option<serde_json::Value>,

    /// 旧版启动参数 (1.12 及以前)
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,

    /// 该版本应使用的客户端 jar 版本号。
    ///
    /// 加载器版本（Fabric/Forge）自己不带客户端 jar，必须沿用父版本的 jar。
    /// 合并继承链时会把父版本的 id 写进这里，启动器据此拼 classpath。
    #[serde(default)]
    pub jar: Option<String>,
}
