//! 账户命令：微软正版登录 / 退出 / 离线用户名。

use sereno_core::Result;
use sereno_core::account::microsoft;
use sereno_core::account::secure_store;
use sereno_core::models::account::{AccountState, DeviceCodeInfo, MicrosoftConfig, kind};

use crate::state::AppState;

/// 当前登录态。
#[tauri::command]
#[specta::specta]
pub fn get_account_state(state: tauri::State<'_, AppState>) -> Result<AccountState> {
    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 微软登录第一步：申请设备码，前端把短码与链接展示给用户。
#[tauri::command]
#[specta::specta]
pub async fn start_microsoft_login(state: tauri::State<'_, AppState>) -> Result<DeviceCodeInfo> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    microsoft::start_device_code(&client).await
}

/// 微软登录第二步：轮询授权结果，成功后保存令牌并切到微软账户。
///
/// 这一步会阻塞到用户在浏览器里完成授权（或超时），前端用 loading 态承载。
#[tauri::command]
#[specta::specta]
pub async fn complete_microsoft_login(
    device_code: String,
    interval: u32,
    expires_in: u32,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;

    let (profile, refresh_token) =
        microsoft::complete_login(&client, &device_code, interval, expires_in).await?;

    // 凭据只进系统密钥库，绝不写设置文件
    let store = secure_store::default_store();
    microsoft::persist_refresh_token(store.as_ref(), &refresh_token)?;

    let mut next = settings;
    next.account.kind = kind::MICROSOFT.to_string();
    next.account.microsoft = Some(profile);
    state.update_settings(next)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 退出登录：删除凭据、清空会话缓存、切回离线账户。
#[tauri::command]
#[specta::specta]
pub async fn logout_account(state: tauri::State<'_, AppState>) -> Result<AccountState> {
    let store = secure_store::default_store();
    microsoft::clear_session(store.as_ref()).await?;

    let mut settings = state.snapshot_settings()?;
    settings.account = sereno_core::models::settings::AccountSettings::default();
    state.update_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 设置离线用户名（切回离线账户）。
#[tauri::command]
#[specta::specta]
pub fn set_offline_account(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let mut settings = state.snapshot_settings()?;
    settings.profile.name = name;
    settings.account.kind = kind::OFFLINE.to_string();
    // 保留微软资料但不再使用，方便用户下次快速切回正版
    state.update_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 诊断用：当前是否配置了微软客户端 ID。
///
/// 单独给前端一个命令而不是塞进设置，避免把 Azure 配置混进用户可见设置项。
#[tauri::command]
#[specta::specta]
pub fn get_microsoft_config() -> Result<MicrosoftConfig> {
    Ok(MicrosoftConfig {
        configured: microsoft::is_configured(),
        placeholder: microsoft::CLIENT_ID_PLACEHOLDER.to_string(),
    })
}

/// 取消正在进行的设备码轮询（关闭登录弹窗时调用）。
#[tauri::command]
#[specta::specta]
pub fn cancel_microsoft_login() -> Result<()> {
    microsoft::cancel_login();
    Ok(())
}

/// 打开微软设备码授权页。
///
/// 地址来自微软接口返回值，但仍做域名白名单校验：
/// 前端永远不能借这个命令打开任意链接。
#[tauri::command]
#[specta::specta]
pub fn open_microsoft_login(url: String) -> Result<()> {
    const ALLOWED: &[&str] = &[
        "https://microsoft.com/",
        "https://www.microsoft.com/",
        "https://login.microsoftonline.com/",
        "https://login.live.com/",
    ];
    if !ALLOWED.iter().any(|prefix| url.starts_with(prefix)) {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "url".into(),
            message: format!("不允许打开非微软登录地址: {url}"),
        });
    }

    opener::open(&url).map_err(|e| sereno_core::LauncherError::Io {
        message: format!("打开浏览器失败: {e}"),
    })
}

/// 启动时预热：如果当前是微软账户，提前刷新令牌。
///
/// 让「点启动」这一步不用再等三次网络往返；失败不阻塞启动，下一次启动会重试。
#[tauri::command]
#[specta::specta]
pub async fn warm_account_session(state: tauri::State<'_, AppState>) -> Result<()> {
    let settings = state.snapshot_settings()?;
    if settings.account.kind != kind::MICROSOFT {
        return Ok(());
    }

    let client = sereno_core::utils::http::build_client(&settings)?;
    if let Err(error) = microsoft::session(&client).await {
        tracing::warn!(%error, "微软会话预热失败，启动时会重试");
    }
    Ok(())
}
