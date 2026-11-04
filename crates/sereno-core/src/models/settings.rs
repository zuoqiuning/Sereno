//! 应用设置的数据契约。

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    #[serde(default)]
    pub window: WindowState,

    #[serde(default)]
    pub java: JavaSettings,

    /// 离线账户信息（微软登录接入前，先用它填启动参数）。
    #[serde(default)]
    pub profile: ProfileSettings,

    /// 当前账户（离线 / 微软）。
    #[serde(default)]
    pub account: AccountSettings,

    #[serde(default)]
    pub download: DownloadSettings,

    /// 启动器自身的更新设置。
    #[serde(default)]
    pub update: UpdateSettings,

    #[serde(default)]
    pub logging: LoggingSettings,

    #[serde(default)]
    pub http: HttpSettings,

    #[serde(default = "default_theme")]
    pub theme: String,

    /// 崩溃（非零退出码）时自动重启游戏。
    ///
    /// 默认关闭：部分整合包就是要求「崩了以后改配置再开」；
    /// 前端实现带次数上限（单实例单会话最多 3 次），防止无限崩溃循环。
    #[serde(default)]
    pub auto_restart_on_crash: bool,

    #[serde(default)]
    pub instances_root: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            window: WindowState::default(),
            java: JavaSettings::default(),
            profile: ProfileSettings::default(),
            account: AccountSettings::default(),
            download: DownloadSettings::default(),
            update: UpdateSettings::default(),
            logging: LoggingSettings::default(),
            http: HttpSettings::default(),
            theme: default_theme(),
            auto_restart_on_crash: false,
            instances_root: None,
        }
    }
}

fn default_theme() -> String {
    "light".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HttpSettings {
    #[specta(type = specta_typescript::Number)]
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,

    #[specta(type = specta_typescript::Number)]
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,

    #[specta(type = specta_typescript::Number)]
    #[serde(default = "default_read_timeout")]
    pub read_timeout_secs: u64,

    #[serde(default = "default_max_redirects")]
    pub max_redirects: u32,

    #[serde(default)]
    pub proxy: Option<String>,
}

fn default_connect_timeout() -> u64 {
    4
}
fn default_request_timeout() -> u64 {
    120
}
fn default_read_timeout() -> u64 {
    30
}
fn default_max_redirects() -> u32 {
    5
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            connect_timeout_secs: default_connect_timeout(),
            request_timeout_secs: default_request_timeout(),
            read_timeout_secs: default_read_timeout(),
            max_redirects: default_max_redirects(),
            proxy: None,
        }
    }
}

impl AppSettings {
    /// 全局 User-Agent。
    ///
    /// 镜像站会做 UA 白名单，格式必须是 `Name/Version`；
    /// 括号里带项目地址，方便管理员识别来源。
    pub fn user_agent(&self) -> String {
        format!(
            "Sereno/{} (+https://github.com/zuoqiuning/Sereno)",
            crate::APP_VERSION
        )
    }

    /// 把越界/脏配置修正到合法范围。
    ///
    /// **读到与写入前都要过一遍**：旧配置或手改文件里可能是 0 / 999，
    /// 0 会让下载器的信号量永远拿不到许可，直接表现为安装卡死。
    pub fn normalized(mut self) -> Self {
        // 上限 64：与 PCL 默认水平对齐。更高会触发镜像限流，
        // 得到的不是更快而是更多 429/503。
        self.download.max_concurrent = self.download.max_concurrent.clamp(1, 64);
        self.download.retry_times = self.download.retry_times.clamp(0, 5);
        // 上限 100 MB/s：再高就没有意义的，还容易把 u64 运算边界撑爆
        if let Some(kbps) = self.download.speed_limit_kbps {
            self.download.speed_limit_kbps = if kbps == 0 {
                None
            } else {
                Some(kbps.clamp(16, 100 * 1024))
            };
        }

        if let Some(mb) = self.java.default_memory_mb {
            self.java.default_memory_mb = Some(mb.clamp(512, 32768));
        }

        let name: String = self
            .profile
            .name
            .trim()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(16)
            .collect();
        self.profile.name = if name.len() < 3 {
            default_profile_name()
        } else {
            name
        };

        self
    }
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaSettings {
    #[serde(default)]
    pub custom_path: Option<String>,
    #[serde(default)]
    pub default_memory_mb: Option<u32>,
}

/// 离线账户设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSettings {
    /// 游戏内显示的用户名（3–16 位字母/数字/下划线）。
    #[serde(default = "default_profile_name")]
    pub name: String,
}

fn default_profile_name() -> String {
    "Player".to_string()
}

impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            name: default_profile_name(),
        }
    }
}

/// 当前账户设置。
///
/// 只保存**公开资料**（游戏 ID、UUID）与账户类型；
/// 微软的 Refresh Token 在系统密钥库里，两者故意分开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountSettings {
    /// `offline` / `microsoft` / `authlib`。
    #[serde(default = "default_account_kind")]
    pub kind: String,
    /// 微软资料；离线账户为 null。
    #[serde(default)]
    pub microsoft: Option<crate::models::account::MicrosoftProfile>,
    /// 已保存的离线档案；空列表时由 `profile.name` 派生。
    #[serde(default)]
    pub offline_accounts: Vec<crate::models::account::OfflineProfile>,
    /// 当前选中的离线档案 ID。
    #[serde(default)]
    pub active_offline_id: Option<String>,
    /// 已登录过的第三方（Yggdrasil）档案。
    ///
    /// 这里只存公开资料；访问令牌在系统密钥库里。
    #[serde(default)]
    pub authlib_accounts: Vec<crate::models::account::AuthlibProfile>,
    /// 当前选中的第三方档案 ID。
    #[serde(default)]
    pub active_authlib_id: Option<String>,
}

fn default_account_kind() -> String {
    crate::models::account::kind::OFFLINE.to_string()
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            kind: default_account_kind(),
            microsoft: None,
            offline_accounts: Vec::new(),
            active_offline_id: None,
            authlib_accounts: Vec::new(),
            active_authlib_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSettings {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    #[serde(default = "default_true")]
    pub prefer_mirror: bool,
    #[serde(default = "default_retry_times")]
    pub retry_times: u32,
    /// 全局限速（KB/s）。`None` 或 0 表示不限速。
    #[serde(default)]
    pub speed_limit_kbps: Option<u32>,
}

/// 启动器自身的更新设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    /// 是否在启动时自动检查更新。
    ///
    /// 默认开启：检查只发一次 HTTP 请求（端点返回 404 也很快），
    /// 不影响启动速度。关闭后只能手动检查。
    #[serde(default = "default_true")]
    pub auto_check: bool,

    /// 上次检查到的新版本号；为空表示已是最新或还没查过。
    ///
    /// 记下来是为了在用户下次打开时仍能提示「有新版本」——
    /// 只在检查那一刻弹一次 Toast 的话，错过就没了。
    #[serde(default)]
    pub latest_version: Option<String>,

    /// 上次检查的时间戳（秒）。用于界面上显示「上次检查于……」。
    ///
    /// 映射成 JS `number`：时间戳到 2^53 秒（约 2.8 亿年）都不会丢精度。
    #[specta(type = specta_typescript::Number)]
    #[serde(default)]
    pub last_checked_at: Option<u64>,
}

fn default_max_concurrent() -> u32 {
    32
}
fn default_true() -> bool {
    true
}
fn default_retry_times() -> u32 {
    3
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            prefer_mirror: default_true(),
            retry_times: default_retry_times(),
            speed_limit_kbps: None,
        }
    }
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: default_true(),
            latest_version: None,
            last_checked_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LoggingSettings {
    #[serde(default)]
    pub verbose: bool,
    #[serde(default = "default_true")]
    pub auto_language_chinese: bool,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            verbose: false,
            auto_language_chinese: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = AppSettings::default();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.download.max_concurrent, 32);
        assert_eq!(s.theme, "light");
        assert!(s.logging.auto_language_chinese);
    }

    /// 镜像站按 `Name/Version` 做 UA 白名单，且要求带来源地址。
    #[test]
    fn user_agent_follows_mirror_registration() {
        let ua = AppSettings::default().user_agent();
        assert!(ua.starts_with(&format!("Sereno/{}", crate::APP_VERSION)));
        assert!(ua.contains("https://github.com/zuoqiuning/Sereno"));
    }
}
