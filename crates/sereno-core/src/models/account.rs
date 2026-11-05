//! 账户相关的数据契约。

use serde::{Deserialize, Serialize};

/// 账户类型。
pub mod kind {
    /// 离线账户（用户名本地派生 UUID）。
    pub const OFFLINE: &str = "offline";
    /// 微软正版账户。
    pub const MICROSOFT: &str = "microsoft";
    /// 第三方 Yggdrasil 账户（authlib-injective）。
    pub const AUTHLIB: &str = "authlib";
}

/// 第三方 Yggdrasil 档案（公开信息，可写进 `settings.json`）。
///
/// **访问令牌不在这里**：它只进系统密钥库，见 `account::secure_store`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthlibProfile {
    /// 稳定 ID（服务器 + 用户名的散列）。
    pub id: String,
    /// 游戏内显示名。
    pub name: String,
    /// 带连字符的 UUID。
    pub uuid: String,
    /// Yggdrasil 服务器的 API Root。
    pub api_root: String,
    /// 服务器显示名（用户自定义，如「LittleSkin」）。
    #[serde(default)]
    pub server_name: String,
}

/// 一次第三方登录的会话结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthlibSession {
    /// 访问令牌（只进密钥库，不写设置）。
    pub access_token: String,
    /// 客户端令牌（刷新会话时用）。
    pub client_token: String,
    /// 角色 UUID。
    pub uuid: String,
    /// 角色名。
    pub name: String,
    /// Yggdrasil 服务器地址。
    pub api_root: String,
}

/// 微软账户资料（公开信息，可写进 `settings.json`）。
///
/// **Refresh Token 不在这里**：凭据只进系统密钥库，见 `account::secure_store`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftProfile {
    /// 游戏内 ID。
    pub name: String,
    /// 带连字符的 UUID。
    pub uuid: String,
}

/// 离线档案。
///
/// UUID 在创建时固化，**不随用户名变化**：Minecraft 用它区分玩家数据，
/// 同一个世界里的物品、进度都挂在 UUID 上。换名字如果重新派生 UUID，
/// 就等于换了一个玩家，这也是「切账号后背包空了」的根源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OfflineProfile {
    /// 稳定 ID（创建时生成，切换/删除都用它）。
    pub id: String,
    /// 游戏内显示名。
    pub name: String,
    /// 启动时传给游戏的 UUID。
    pub uuid: String,
}

/// 当前登录态，前端据此渲染账户卡片。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountState {
    /// `offline` / `microsoft` / `authlib`。
    pub kind: String,
    /// 显示名：微软账户为游戏 ID，离线为当前档案的用户名。
    pub name: String,
    /// 当前账户的 UUID；离线档案也有（启动时直接使用）。
    pub uuid: Option<String>,
    /// 微软登录是否可用（客户端 ID 是否已配置）。
    pub microsoft_available: bool,
    /// 凭据是否只保存在内存中（系统密钥库不可用，重启后需重新登录）。
    pub credential_store_ephemeral: bool,
    /// 已保存的离线档案。
    pub offline_accounts: Vec<OfflineProfile>,
    /// 当前选中的离线档案 ID。
    pub active_offline_id: Option<String>,
    /// 已登录过的微软资料（即使当前切到了离线档案也会保留）。
    pub microsoft_profile: Option<MicrosoftProfile>,
    /// 已登录过的第三方档案。
    pub authlib_profiles: Vec<AuthlibProfile>,
    /// 当前选中的第三方档案 ID。
    pub active_authlib_id: Option<String>,
    /// 当前第三方档案的服务器地址（启动时拼 `-javaagent` 用）。
    pub authlib_api_root: Option<String>,
}

/// 微软登录配置（客户端 ID 是否就绪）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftConfig {
    /// 是否已配置可用的客户端 ID（全零占位视为未配置）。
    pub configured: bool,
}

/// 设备码登录的第一步返回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeInfo {
    /// 用户在浏览器里输入的短码。
    pub user_code: String,
    /// 验证地址（`https://microsoft.com/link`）。
    pub verification_uri: String,
    /// 设备码有效期（秒）。用 u32：设备码最多 15 分钟，且避免 BigInt 前端类型。
    pub expires_in: u32,
    /// 建议轮询间隔（秒）。
    pub interval: u32,
    /// 轮询用的设备码，前端原样回传给 `complete_microsoft_login`。
    pub device_code: String,
}
