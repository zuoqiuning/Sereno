//! 账户相关的数据契约。

use serde::{Deserialize, Serialize};

/// 账户类型。
pub mod kind {
    /// 离线账户（用户名本地派生 UUID）。
    pub const OFFLINE: &str = "offline";
    /// 微软正版账户。
    pub const MICROSOFT: &str = "microsoft";
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
    /// `offline` / `microsoft`。
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
}

/// 微软登录配置（客户端 ID 是否就绪）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftConfig {
    /// 是否已配置真实客户端 ID。
    pub configured: bool,
    /// 预留的占位值，便于界面上提示「待替换」。
    pub placeholder: String,
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
