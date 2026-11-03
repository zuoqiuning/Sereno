use serde::{Deserialize, Serialize};

/// 资源来源。`aggregate` 只出现在搜索请求里，条目本身只会是后两者之一。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ResourceSource {
    Aggregate,
    Modrinth,
    Curseforge,
}

/// 资源种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Mod,
    Modpack,
    Resourcepack,
    Shader,
    Datapack,
}

/// 排序方式：两个站点都只支持「下载量」与「更新时间」的原生排序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ResourceSort {
    Downloads,
    Updated,
}

/// 一次资源搜索请求。`page` 从 1 开始，`mc_version` / `loader` 为空表示不筛选。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSearchQuery {
    #[serde(default)]
    pub query: String,
    pub source: ResourceSource,
    pub kind: ResourceKind,
    #[serde(default)]
    pub mc_version: String,
    #[serde(default)]
    pub loader: String,
    pub sort: ResourceSort,
    pub page: u32,
    pub page_size: u32,
}

/// 一页搜索结果。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSearchPage {
    pub items: Vec<ModProject>,
    pub total: u32,
    /// 按当前每页条数算出的总页数，至少为 1，前端据此做页码跳转。
    pub page_count: u32,
    pub page: u32,
    pub page_size: u32,
    pub has_more: bool,
    /// 聚合时某个源失败/未配置的提示，前端直接用一条弱化文案展示。
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModProject {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub downloads: u64,
    #[specta(type = specta_typescript::Number)]
    pub follows: u64,
    pub author: String,
    pub source: ResourceSource,
    /// ISO 8601 更新时间，按更新时间排序与展示用；缺省表示站点没给。
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModFile>,
    #[specta(type = specta_typescript::Number)]
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub date_published: Option<String>,
    /// Modrinth 的 version_type：release / beta / alpha。
    #[serde(default)]
    pub version_type: Option<String>,
    /// 该版本声明的依赖关系（Modrinth 只给 project_id，名称需另行解析）。
    #[serde(default)]
    pub dependencies: Vec<ModDependency>,
}

/// Modrinth 版本依赖：`required` 才是必须安装的前置。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModDependency {
    pub project_id: String,
    #[serde(default)]
    pub version_id: Option<String>,
    /// required / optional / incompatible / embedded。
    #[serde(default)]
    pub dependency_type: Option<String>,
}

/// CurseForge 文件依赖：relationType = 3 表示必须安装。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfDependency {
    pub mod_id: u32,
    #[serde(default)]
    pub relation_type: u32,
}

/// 解析后的依赖项目：前端直接展示名称、图标与介绍。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDependencyInfo {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
    /// 一句话介绍。
    pub description: Option<String>,
    /// 下载量。
    #[specta(type = specta_typescript::Number)]
    pub downloads: u64,
    /// 最后更新时间。
    pub updated_at: Option<String>,
    /// 来源平台。
    pub source: ResourceSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub hashes: ModHashes,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModHashes {
    pub sha1: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub summary: String,
    pub logo: Option<CfLogo>,
    #[specta(type = specta_typescript::Number)]
    pub download_count: u64,
    pub authors: Vec<CfAuthor>,
    #[serde(default)]
    pub date_modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfLogo {
    pub thumbnail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfAuthor {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfSearchResponse {
    pub data: Vec<CfMod>,
    pub pagination: CfPagination,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfPagination {
    pub index: u32,
    pub page_size: u32,
    pub result_count: u32,
    pub total_count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    pub id: u32,
    pub display_name: String,
    pub file_name: String,
    pub download_url: Option<String>,
    #[specta(type = specta_typescript::Number)]
    #[serde(default)]
    pub download_count: u64,
    /// 同时包含游戏版本与加载器标签，如 `["1.21.1", "NeoForge"]`。
    #[serde(default)]
    pub game_versions: Vec<String>,
    /// 1 = 正式版，2 = 测试版，3 = 早期测试版。
    #[serde(default)]
    pub release_type: u32,
    #[serde(default)]
    pub file_date: Option<String>,
    /// 文件依赖：relationType = 3 表示必须安装。
    #[serde(default)]
    pub dependencies: Vec<CfDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CfFilesResponse {
    pub data: Vec<CfFile>,
}

/// 本地资源种类（实例目录里的 mods / resourcepacks / shaderpacks / datapacks）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LocalResourceKind {
    Mod,
    Resourcepack,
    Shader,
    Datapack,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalResource {
    pub kind: LocalResourceKind,
    /// 磁盘上的真实文件名（禁用时带 `.disabled` 后缀）。
    pub file_name: String,
    /// 去掉扩展名与 `.disabled` 后的展示名。
    pub display_name: String,
    pub enabled: bool,
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
}

/// 本地文件匹配到的远端项目信息，用于在实例资源列表里显示图标 / 名称 / 介绍。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalResourceMeta {
    pub file_name: String,
    pub title: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub source: ResourceSource,
}

/// 一次本地资源扫描：元数据 + 可更新版本。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalResourceScan {
    pub metadata: Vec<LocalResourceMeta>,
    pub updates: Vec<LocalResourceUpdate>,
}

/// 检查更新后返回的可更新版本。前端按文件对应展示。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalResourceUpdate {
    pub file_name: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub version_name: String,
    pub download_url: String,
    pub new_file_name: String,
    pub sha1: Option<String>,
    pub source: ResourceSource,
}
