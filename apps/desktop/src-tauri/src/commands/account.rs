//! 账户命令：微软正版登录 / 第三方登录 / 退出 / 离线用户名。

use sereno_core::Result;
use sereno_core::account::{authlib, microsoft, offline, secure_store};
use sereno_core::models::account::{
    AccountState, AuthlibProfile, DeviceCodeInfo, MicrosoftConfig, OfflineProfile, kind,
};

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
    state.update_account_settings(next)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 退出登录：删除凭据、清空会话缓存、切回离线账户。
///
/// 只清掉微软资料，**离线档案列表必须保留**，否则用户建过的档案会全部丢失。
#[tauri::command]
#[specta::specta]
pub async fn logout_account(state: tauri::State<'_, AppState>) -> Result<AccountState> {
    let store = secure_store::default_store();
    microsoft::clear_session(store.as_ref()).await?;

    let mut settings = state.snapshot_settings()?;
    settings.account.microsoft = None;
    settings.account.kind = kind::OFFLINE.to_string();
    offline::ensure(&mut settings);
    state.update_account_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 设置离线用户名（切回离线账户）；同名档案存在时直接切换过去。
#[tauri::command]
#[specta::specta]
pub fn set_offline_account(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let mut settings = state.snapshot_settings()?;
    offline::ensure(&mut settings);

    let trimmed = name.trim();
    let existing = settings
        .account
        .offline_accounts
        .iter()
        .find(|profile| profile.name.eq_ignore_ascii_case(trimmed))
        .map(|profile| profile.id.clone());

    match existing {
        Some(id) => {
            offline::switch(&mut settings, &id)?;
        }
        None => {
            offline::add(&mut settings, trimmed)?;
        }
    }

    state.update_account_settings(settings)?;
    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 列出已保存的离线档案。
///
/// 老配置没有档案列表时，会用当前 `profile.name` 派生一个，
/// 保证升级后至少有一个可用档案。
#[tauri::command]
#[specta::specta]
pub fn list_offline_accounts(state: tauri::State<'_, AppState>) -> Result<Vec<OfflineProfile>> {
    let settings = state.snapshot_settings()?;
    Ok(offline::profiles(&settings))
}

/// 新建离线档案并切换过去。
#[tauri::command]
#[specta::specta]
pub fn add_offline_account(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let mut settings = state.snapshot_settings()?;
    offline::add(&mut settings, &name)?;
    state.update_account_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 切换到某个离线档案。
#[tauri::command]
#[specta::specta]
pub fn switch_offline_account(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let mut settings = state.snapshot_settings()?;
    offline::switch(&mut settings, &id)?;
    state.update_account_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 删除离线档案（至少保留一个）。
#[tauri::command]
#[specta::specta]
pub fn remove_offline_account(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let mut settings = state.snapshot_settings()?;
    offline::remove(&mut settings, &id)?;
    state.update_account_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 切换回已登录过的微软账户（令牌仍在系统密钥库）。
#[tauri::command]
#[specta::specta]
pub async fn activate_microsoft_account(state: tauri::State<'_, AppState>) -> Result<AccountState> {
    let settings = state.snapshot_settings()?;
    if settings.account.microsoft.is_none() {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "microsoft".into(),
            message: "还没有登录过微软账户".into(),
        });
    }

    // 先刷新一次会话，确认密钥库里的令牌仍然有效；失败就不要切过去。
    let client = sereno_core::utils::http::build_client(&settings)?;
    microsoft::session(&client).await?;

    let mut next = settings;
    next.account.kind = kind::MICROSOFT.to_string();
    state.update_account_settings(next)?;

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

/// 第三方登录（authlib-injective / Yggdrasil）。
///
/// 用户提供 Yggdrasil 服务器地址与账号密码，命令用它们换取访问令牌。
///
/// **密码只在这一次请求里出现**：不落盘、不进日志、不进诊断包。
/// 换来的令牌只进系统密钥库，设置文件里只留公开资料。
#[tauri::command]
#[specta::specta]
pub async fn authlib_login(
    api_root: String,
    username: String,
    password: String,
    server_name: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;

    let session = authlib::authenticate(&client, &api_root, &username, &password).await?;

    // 令牌进系统密钥库
    let store = secure_store::default_store();
    authlib::persist_session(store.as_ref(), &session)?;

    // 公开资料进设置，并切到该档案
    let mut next = settings;
    authlib::save_profile(&mut next, &session, server_name.as_deref().unwrap_or(""));
    state.update_account_settings(next)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 切换到指定的第三方档案。
#[tauri::command]
#[specta::specta]
pub fn switch_authlib_account(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let mut settings = state.snapshot_settings()?;
    authlib::switch(&mut settings, &id)?;
    state.update_account_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 退出第三方登录：删掉凭据并移除档案。
#[tauri::command]
#[specta::specta]
pub async fn logout_authlib_account(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<AccountState> {
    let mut settings = state.snapshot_settings()?;

    // 先让服务端作废令牌（失败也不影响本地退出）
    if let Some(profile) = settings
        .account
        .authlib_accounts
        .iter()
        .find(|entry| entry.id == id)
    {
        let client = sereno_core::utils::http::build_client(&settings)?;
        if let Ok(session) = authlib::session(&client, profile).await {
            authlib::invalidate(&client, &profile.api_root, &session.access_token).await?;
        }
        let store = secure_store::default_store();
        authlib::clear_session(store.as_ref(), profile).await?;
    }

    authlib::remove(&mut settings, &id)?;
    state.update_account_settings(settings)?;

    let settings = state.snapshot_settings()?;
    Ok(microsoft::account_state(&settings))
}

/// 列出已登录的第三方档案。
#[tauri::command]
#[specta::specta]
pub fn list_authlib_accounts(state: tauri::State<'_, AppState>) -> Result<Vec<AuthlibProfile>> {
    let settings = state.snapshot_settings()?;
    Ok(authlib::profiles(&settings))
}
