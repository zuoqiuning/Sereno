//! 统一错误类型。
//!
//! 放在领域层而不是命令层，因为「有哪些失败模式」是业务事实，不是 IPC 细节：
//! 放在 core 才能被单元测试直接匹配，也避免命令层反过来定义业务错误。
//!
//! 序列化后是**带 `type` 标签的判别联合**，前端可以按 `error.type` 做类型收窄：
//!
//! ```ts
//! type LauncherError =
//!   | { type: 'javaNotFound'; requiredMajor: number }
//!   | { type: 'downloadFailed'; message: string; url: string }
//! ```
//!
//! 命令签名一律用 `Result<T, LauncherError>`，不在任何命令中使用 `unwrap()`——
//! 命令中的 panic 会让前端收到一个没有细节的通用错误，排查成本极高。

use serde::{Deserialize, Serialize};

/// 领域层统一结果别名。
pub type Result<T> = std::result::Result<T, LauncherError>;

/// 启动器的全部失败模式。
///
/// 命名按「用户能做什么」组织，而不是按「哪一层出的问题」组织。
/// 每个变体的 `error` 文案都要包含**用户能执行的下一步**，
/// 而不是只说 `io error`。
///
/// `rename_all` 作用于**变体名**，`rename_all_fields` 作用于**变体内的字段名**——
/// 两者都要有，前端才能拿到 `requiredMajor` 而不是 `required_major`。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LauncherError {
    // ---------------- Java ----------------
    #[error("未找到可用的 Java，需要 {required_major}+ 版本，请在设置中指定 Java 路径")]
    JavaNotFound { required_major: u32 },

    #[error("Java 架构不匹配：{path}（{arch}），当前系统为 {system_arch}")]
    JavaArchMismatch {
        path: String,
        arch: String,
        system_arch: String,
    },

    #[error("Java 下载失败：{message}，可稍后重试或手动安装后指定路径")]
    JavaDownloadFailed { message: String },

    // ---------------- 版本与加载器 ----------------
    #[error("版本 {id} 不存在，可能已被 Mojang 下架")]
    VersionNotFound { id: String },

    #[error("加载器 {loader} {version} 不存在或不受支持")]
    LoaderNotFound { loader: String, version: String },

    #[error("加载器安装失败：{message}")]
    LoaderInstallFailed { message: String },

    // ---------------- 下载 ----------------
    #[error("下载失败：{message}（来源 {url}），可尝试切换镜像源后重试")]
    DownloadFailed { message: String, url: String },

    #[error("文件校验失败：{path}，期望 {expected}，实际 {actual}")]
    ChecksumMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("下载已取消：{task_id}")]
    DownloadCancelled { task_id: String },

    #[error("所有镜像源均不可用：{message}")]
    AllSourcesUnavailable { message: String },

    // ---------------- 实例 ----------------
    #[error("实例不存在：{id}")]
    InstanceNotFound { id: String },

    #[error("实例正在运行，禁止执行破坏性操作：{id}")]
    InstanceBusy { id: String },

    #[error("实例 {id} 已存在同名目录")]
    InstanceAlreadyExists { id: String },

    // ---------------- 账户 ----------------
    #[error("未登录微软账号，请先在「账户」页完成登录")]
    NotAuthenticated,

    #[error("第三方登录失败：{message}")]
    AuthlibAuthFailed { message: String },

    #[error("凭据存储不可用：{message}，请检查系统密钥库权限")]
    CredentialStoreUnavailable { message: String },

    #[error("令牌刷新失败：{message}，请重新登录")]
    TokenRefreshFailed { message: String },

    // ---------------- 资源平台 ----------------
    /// CurseForge 官方接口需要 API Key。
    ///
    /// 官方接口的每个端点都强制要求鉴权请求头，而镜像（MCIM）不需要。
    /// 本项目**不再携带任何 Key**，因此关掉镜像后官方接口必然拒绝请求，
    /// 这时要把「开回镜像」这个下一步说清楚，而不是丢一个裸的 HTTP 码。
    #[error("CurseForge 官方接口需要 API Key，请在设置中开启「优先使用国内镜像」")]
    CurseForgeOfficialKeyRequired,

    #[error("资源 {project_id} 未找到")]
    ResourceNotFound { project_id: String },

    // ---------------- 更新通道 ----------------
    /// 更新通道出错（检查、下载、验签或安装失败）。
    ///
    /// 单独成一个变体而不是复用 `Internal`：前端要能区分
    /// 「更新失败」与「其它内部错误」，给用户不同的引导
    /// （前者提示「稍后重试或手动下载」，后者提示「查看日志」）。
    #[error("更新失败：{message}")]
    UpdateFailed { message: String },

    // ---------------- 通用 ----------------
    #[error("文件读写失败：{message}")]
    Io { message: String },

    #[error("数据格式错误：{message}")]
    Parse { message: String },

    #[error("配置项 {key} 无效：{message}")]
    InvalidConfig { key: String, message: String },

    #[error("操作被取消")]
    Cancelled,

    #[error("{message}")]
    Internal { message: String },
}

impl LauncherError {
    /// 供日志使用的稳定错误码，便于按类别检索。
    pub fn code(&self) -> &'static str {
        match self {
            Self::JavaNotFound { .. } => "java_not_found",
            Self::JavaArchMismatch { .. } => "java_arch_mismatch",
            Self::JavaDownloadFailed { .. } => "java_download_failed",
            Self::VersionNotFound { .. } => "version_not_found",
            Self::LoaderNotFound { .. } => "loader_not_found",
            Self::LoaderInstallFailed { .. } => "loader_install_failed",
            Self::DownloadFailed { .. } => "download_failed",
            Self::ChecksumMismatch { .. } => "checksum_mismatch",
            Self::DownloadCancelled { .. } => "download_cancelled",
            Self::AllSourcesUnavailable { .. } => "all_sources_unavailable",
            Self::InstanceNotFound { .. } => "instance_not_found",
            Self::InstanceBusy { .. } => "instance_busy",
            Self::InstanceAlreadyExists { .. } => "instance_already_exists",
            Self::NotAuthenticated => "not_authenticated",
            Self::AuthlibAuthFailed { .. } => "authlib_auth_failed",
            Self::CredentialStoreUnavailable { .. } => "credential_store_unavailable",
            Self::TokenRefreshFailed { .. } => "token_refresh_failed",
            Self::CurseForgeOfficialKeyRequired => "curseforge_official_key_required",
            Self::ResourceNotFound { .. } => "resource_not_found",
            Self::UpdateFailed { .. } => "update_failed",
            Self::Io { .. } => "io",
            Self::Parse { .. } => "parse",
            Self::InvalidConfig { .. } => "invalid_config",
            Self::Cancelled => "cancelled",
            Self::Internal { .. } => "internal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_discriminated_union() {
        let err = LauncherError::JavaNotFound { required_major: 21 };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["type"], "javaNotFound");
        assert_eq!(json["requiredMajor"], 21);
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(LauncherError::NotAuthenticated.code(), "not_authenticated");
        assert_eq!(
            LauncherError::Io {
                message: "x".into()
            }
            .code(),
            "io"
        );
    }

    /// 官方接口要 Key 的错误必须能被前端按类型识别，
    /// 且文案里带上「开回镜像」这个可执行的下一步。
    #[test]
    fn official_key_required_is_actionable() {
        let err = LauncherError::CurseForgeOfficialKeyRequired;
        assert_eq!(err.code(), "curseforge_official_key_required");

        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["type"], "curseForgeOfficialKeyRequired");

        let text = err.to_string();
        assert!(
            text.contains("优先使用国内镜像"),
            "错误文案要告诉用户怎么修: {text}"
        );
    }
}
