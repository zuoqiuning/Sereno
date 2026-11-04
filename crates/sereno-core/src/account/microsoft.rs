//! 微软正版登录（Microsoft Account → Xbox Live → Minecraft）。
//!
//! ## 为什么用设备码流程
//!
//! 桌面启动器需要一条**不需要内嵌浏览器、也不需要处理本地回调端口**的登录路径。
//! 设备码（Device Code）流程正好满足：启动器拿到一段短码，用户在任意浏览器里
//! 输入并授权，启动器轮询取回令牌。它不需要在应用里存放客户端密钥，
//! 也不会因为 127.0.0.1 回调端口被占用而失败。
//!
//! ## 令牌链路
//!
//! ```text
//! MSA access token
//!   → Xbox Live (user.auth.xboxlive.com)   → XBL token + uhs
//!   → XSTS        (xsts.auth.xboxlive.com) → XSTS token
//!   → Minecraft   (login_with_xbox)        → Minecraft access token
//!   → /minecraft/profile                   → 游戏 ID + UUID
//! ```
//!
//! ## 客户端 ID
//!
//! 微软要求每个启动器使用自己的 Azure 应用（客户端 ID）。
//! 这里**预留**了常量与环境变量两种来源：正式发布时把真实 ID 写进
//! [`CLIENT_ID_PLACEHOLDER`]，开发时也可以用 `SERENO_MS_CLIENT_ID` 覆盖。
//! 未配置时登录入口会明确提示，而不是发一个注定失败的请求。

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::time::sleep;

use crate::Result;
use crate::account::secure_store::{self, SecureStore, keys};
use crate::models::account::{AccountState, DeviceCodeInfo, MicrosoftProfile, kind};
use crate::models::settings::AppSettings;
use crate::{LauncherError, Result as CoreResult};

const AUTH_BASE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPE: &str = "XboxLive.signin offline_access";
const XBOX_USER_AUTH: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XBOX_XSTS: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

/// 应用还没通过 Mojang 审核时，Minecraft API 会拒绝签发游戏令牌。
const MC_API_PENDING: &str = "Minecraft API 权限正在审批中，请耐心等待，或暂时使用离线模式。";

/// 微软返回的「应用未注册 / 未批准」错误；不同端点文案略有差异。
fn is_pending_registration(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("invalid app registration")
        || lower.contains("application is not approved")
        || lower.contains("app registration is not approved")
}

/// **预留的客户端 ID**。
///
/// 发布前替换成在 Azure 门户注册的「移动和桌面应用程序」客户端 ID，
/// 并确保该应用配置了 `XboxLive.signin` 与 `offline_access` 权限。
pub const CLIENT_ID_PLACEHOLDER: &str = "00000000-0000-0000-0000-000000000000";

/// 微软登录用的客户端 ID：环境变量优先，其次使用预留常量。
pub fn client_id() -> String {
    std::env::var("SERENO_MS_CLIENT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| CLIENT_ID_PLACEHOLDER.to_string())
}

/// 客户端 ID 是否已配置（占位符视为未配置）。
pub fn is_configured() -> bool {
    let id = client_id();
    id != CLIENT_ID_PLACEHOLDER && !id.is_empty()
}

/// 一次登录得到的会话（Minecraft 侧）。
///
/// **不实现 `Debug`**：结构体里有访问令牌，一次不经意的 `?session`
/// 就会把令牌写进日志。需要诊断时打印 name/uuid 两个字段即可。
#[derive(Clone)]
pub struct MsSession {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    /// Xbox 用户 ID（XUID）。`userType=msa` 时必须传给游戏。
    pub xuid: String,
}

/// 进程内会话缓存，避免每次启动都重跑整条令牌链路。
///
/// 有效期内直接用缓存；过期（提前 5 分钟）重新刷新。
static SESSION: LazyLock<tokio::sync::Mutex<Option<(Instant, MsSession)>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));

/// 用户关闭登录弹窗时置位，轮询在下一轮退出。
static CANCEL_LOGIN: AtomicBool = AtomicBool::new(false);

/// 取消正在进行的设备码轮询（关弹窗 / 点取消时调用）。
pub fn cancel_login() {
    CANCEL_LOGIN.store(true, Ordering::SeqCst);
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct XboxResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
    /// XUID；个别账号/接口版本可能不返回。
    #[serde(default)]
    xid: Option<String>,
}

#[derive(Deserialize)]
struct MinecraftTokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct MinecraftProfileResponse {
    id: String,
    name: String,
}

/// 开始设备码登录，返回给用户展示的短码。
pub async fn start_device_code(client: &reqwest::Client) -> Result<DeviceCodeInfo> {
    let id = require_client_id()?;
    // 新一轮登录开始，清掉上一次的取消标记
    CANCEL_LOGIN.store(false, Ordering::SeqCst);

    let resp = post_form(
        client,
        &format!("{AUTH_BASE}/devicecode"),
        &[("client_id", id.as_str()), ("scope", SCOPE)],
    )
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(LauncherError::Internal {
            message: format!("微软登录初始化失败（HTTP {status}）：{}", body_tail(&text)),
        });
    }

    let data: DeviceCodeResponse = resp.json().await.map_err(|e| LauncherError::Parse {
        message: format!("解析设备码失败: {e}"),
    })?;

    Ok(DeviceCodeInfo {
        user_code: data.user_code,
        verification_uri: data.verification_uri,
        expires_in: data.expires_in.min(u32::MAX as u64) as u32,
        interval: data.interval.max(1).min(u32::MAX as u64) as u32,
        device_code: data.device_code,
    })
}

/// 轮询设备码结果，成功后完成整条 Xbox / Minecraft 链路。
///
/// 失败模式都翻译成用户能理解的文案：超时、用户拒绝、账号未开通 Xbox。
pub async fn complete_login(
    client: &reqwest::Client,
    device_code: &str,
    interval: u32,
    expires_in: u32,
) -> Result<(MicrosoftProfile, String)> {
    let id = require_client_id()?;
    let deadline = Instant::now() + Duration::from_secs(expires_in.min(900) as u64);
    let mut interval = interval.max(1) as u64;

    loop {
        if CANCEL_LOGIN.load(Ordering::SeqCst) {
            CANCEL_LOGIN.store(false, Ordering::SeqCst);
            return Err(LauncherError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(LauncherError::Internal {
                message: "登录超时：请在浏览器里完成授权后重试".into(),
            });
        }

        sleep(Duration::from_secs(interval)).await;

        if CANCEL_LOGIN.load(Ordering::SeqCst) {
            CANCEL_LOGIN.store(false, Ordering::SeqCst);
            return Err(LauncherError::Cancelled);
        }

        let resp = post_form(
            client,
            &format!("{AUTH_BASE}/token"),
            &[
                ("client_id", id.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
            ],
        )
        .await?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| LauncherError::Parse {
            message: format!("读取登录结果失败: {e}"),
        })?;
        let data: TokenResponse =
            serde_json::from_str(&text).map_err(|e| LauncherError::Parse {
                message: format!(
                    "解析登录结果失败（HTTP {status}）: {e}；响应：{}",
                    body_tail(&text)
                ),
            })?;

        // 非 2xx 但没有 error 字段（例如代理返回的 HTML）也要按失败处理，
        // 否则会误报成「返回了空令牌」。
        let outcome = if !status.is_success() && data.error.is_none() {
            PollOutcome::Failed
        } else {
            poll_outcome(data.error.as_deref())
        };

        match outcome {
            // 用户还没在浏览器里点完授权，继续等
            PollOutcome::Authorized => {
                let access = data.access_token.ok_or_else(|| LauncherError::Internal {
                    message: "微软登录返回了空令牌".into(),
                })?;
                let refresh =
                    data.refresh_token
                        .ok_or_else(|| LauncherError::TokenRefreshFailed {
                            message: "微软没有返回长期令牌，请确认应用已申请 offline_access".into(),
                        })?;
                let profile = login_profile(client, &access).await?;
                return Ok((profile, refresh));
            }
            PollOutcome::Pending => continue,
            PollOutcome::SlowDown => {
                interval += 5;
                continue;
            }
            PollOutcome::Declined => {
                return Err(LauncherError::Internal {
                    message: "登录已取消：你在浏览器里拒绝了授权".into(),
                });
            }
            PollOutcome::Expired => {
                return Err(LauncherError::Internal {
                    message: "登录码已过期，请重新发起登录".into(),
                });
            }
            PollOutcome::Failed => {
                let code = data
                    .error
                    .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
                let description = data.error_description.unwrap_or_default();
                let hint = aad_hint(&text).unwrap_or("");
                let raw = body_tail(&text);

                tracing::warn!(
                    status = %status,
                    code = %code,
                    response = %crate::utils::redact::redact_line(&raw),
                    "微软令牌兑换失败"
                );

                let mut message = format!("微软登录失败（{code}）");
                if description.is_empty() {
                    message.push_str(&format!("；响应：{raw}"));
                } else {
                    message.push_str(&format!("：{description}"));
                }
                if !hint.is_empty() {
                    message.push_str(&format!("。{hint}"));
                }
                return Err(LauncherError::Internal { message });
            }
        }
    }
}

/// 设备码轮询的一步结果。拆成纯函数便于单测，不依赖网络。
#[derive(Debug, PartialEq, Eq)]
enum PollOutcome {
    Authorized,
    Pending,
    SlowDown,
    Declined,
    Expired,
    Failed,
}

fn poll_outcome(error: Option<&str>) -> PollOutcome {
    match error {
        None => PollOutcome::Authorized,
        Some("authorization_pending") => PollOutcome::Pending,
        Some("slow_down") => PollOutcome::SlowDown,
        Some("authorization_declined") => PollOutcome::Declined,
        Some("expired_token") => PollOutcome::Expired,
        Some(_) => PollOutcome::Failed,
    }
}

/// 响应体截断：错误页可能是整页 HTML，不能原样塞进界面。
fn body_tail(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = compact.chars().take(300).collect();
    if compact.chars().count() > 300 {
        format!("{short}…")
    } else {
        short
    }
}

/// 把常见的 AADSTS 错误码翻译成用户能执行的下一步。
///
/// 这些错误 90% 出在 Azure 应用注册上，不是代码 bug；
/// 直接把原始错误码丢给用户，他只会看到一串英文。
fn aad_hint(text: &str) -> Option<&'static str> {
    if text.contains("AADSTS7000218") || text.contains("client_assertion") {
        Some(
            "该 Azure 应用未启用「允许公共客户端流」。请到 Azure 门户 → Authentication → \
             高级设置，把「允许公共客户端流」设为「是」，保存后重新登录",
        )
    } else if text.contains("AADSTS700016") || text.contains("AADSTS70001") {
        Some(
            "客户端 ID 与应用注册不匹配：确认 .env 里的 ID 与 Azure 门户一致，\
             且该应用的受支持账户类型包含「个人 Microsoft 账户」",
        )
    } else if text.contains("AADSTS50020") {
        Some("当前登录账号类型与应用注册的账户类型不匹配，请换用个人 Microsoft 账户登录")
    } else if text.contains("AADSTS65004") {
        Some("你在浏览器里取消了授权，请重新发起登录")
    } else if text.contains("AADSTS70002") {
        Some("应用注册的权限配置不完整：需要 XboxLive.signin 与 offline_access，请检查 API 权限")
    } else if text.contains("invalid_grant") {
        Some("设备码已过期或已被使用，请重新发起登录")
    } else {
        None
    }
}

/// 从微软 Access Token 一路走到 Minecraft 资料。
async fn login_profile(client: &reqwest::Client, msa_access: &str) -> Result<MicrosoftProfile> {
    let session = minecraft_session(client, msa_access).await?;

    *SESSION.lock().await = Some((
        Instant::now() + Duration::from_secs(session_cache_secs(session.expires_in)),
        MsSession {
            name: session.profile.name.clone(),
            uuid: session.profile.uuid.clone(),
            access_token: session.access_token,
            xuid: session.xuid,
        },
    ));

    Ok(session.profile)
}

/// 微软 token → Xbox → XSTS → Minecraft token → 资料。
struct MinecraftSessionInfo {
    access_token: String,
    expires_in: u64,
    profile: MicrosoftProfile,
    xuid: String,
}

/// Xbox Live 用户认证请求体。
///
/// `RpsTicket` 必须是 `d=<微软 access token>`；`RelyingParty` / `TokenType`
/// 是 Xbox Live 的硬性字段，缺失时服务端返回空响应体的 400。
fn xbox_auth_body(msa_access: &str) -> serde_json::Value {
    serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={msa_access}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    })
}

/// XSTS 授权请求体；`RelyingParty` / `TokenType` 为文档要求字段。
fn xsts_body(xbl_token: &str) -> serde_json::Value {
    serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token],
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    })
}

async fn minecraft_session(
    client: &reqwest::Client,
    msa_access: &str,
) -> Result<MinecraftSessionInfo> {
    // 1. Xbox Live
    let xbox: XboxResponse = post_json(client, XBOX_USER_AUTH, &xbox_auth_body(msa_access)).await?;
    let claims = xbox
        .display_claims
        .xui
        .first()
        .ok_or_else(|| LauncherError::Internal {
            message: "Xbox 认证没有返回用户标识".into(),
        })?;
    let uhs = claims.uhs.clone();
    let xuid = claims.xid.clone().unwrap_or_default();

    // 2. XSTS（账号没有 Xbox 档案 / 未成年限制时这里会失败）
    //
    // `RelyingParty` 与 `TokenType` 是文档要求的字段：Minecraft 服务的
    // relying party 固定为 `rp://api.minecraftservices.com/`。
    let xsts: XboxResponse = post_json(client, XBOX_XSTS, &xsts_body(&xbox.token)).await?;

    // 3. Minecraft 登录
    let mc: MinecraftTokenResponse = post_json(
        client,
        MC_LOGIN,
        &serde_json::json!({
            "identityToken": format!("XBL3.0 x={};{}", uhs, xsts.token),
        }),
    )
    .await?;

    // 4. 资料（顺便验证是否拥有游戏；没有游戏时这里返回 404）
    let resp = client
        .get(MC_PROFILE)
        .bearer_auth(&mc.access_token)
        .send()
        .await
        .map_err(|e| LauncherError::Internal {
            message: format!("获取 Minecraft 资料失败: {e}"),
        })?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();

        if status == reqwest::StatusCode::FORBIDDEN || is_pending_registration(&text) {
            return Err(LauncherError::Internal {
                message: format!("{MC_API_PENDING}（HTTP {status}）"),
            });
        }
        if status.as_u16() == 404 {
            return Err(LauncherError::Internal {
                message: "该微软账号未拥有 Minecraft Java 版，请先在官网购买或确认使用的账号"
                    .into(),
            });
        }
        return Err(LauncherError::Internal {
            message: format!(
                "获取 Minecraft 资料失败（HTTP {status}）：{}",
                body_tail(&text)
            ),
        });
    }

    let data: MinecraftProfileResponse = resp.json().await.map_err(|e| LauncherError::Parse {
        message: format!("解析 Minecraft 资料失败: {e}"),
    })?;

    let uuid = dash_uuid(&data.id);
    Ok(MinecraftSessionInfo {
        access_token: mc.access_token,
        expires_in: mc.expires_in.max(1),
        profile: MicrosoftProfile {
            name: data.name,
            uuid,
        },
        xuid,
    })
}

/// 会话缓存有效期：比真实过期时间提前 5 分钟。
///
/// 这样「取 token → 组参数 → 启动」的过程中不会突然过期；
/// 极短的令牌（不足 5 分钟）至少保留 60 秒，避免缓存变成负数。
fn session_cache_secs(expires_in: u64) -> u64 {
    expires_in.saturating_sub(300).max(60)
}

/// 用保存的 Refresh Token 刷新出新的 Minecraft 会话（带进程内缓存）。
pub async fn session(client: &reqwest::Client) -> Result<MsSession> {
    if let Some((at, cached)) = SESSION.lock().await.clone()
        && Instant::now() < at
    {
        return Ok(cached);
    }

    let store = secure_store::default_store();
    let refresh = store
        .get(keys::MICROSOFT_REFRESH_TOKEN)?
        .ok_or(LauncherError::NotAuthenticated)?;
    let id = require_client_id()?;

    let (msa_access, new_refresh, _expires_in) = refresh_msa(client, &refresh, &id).await?;
    if let Some(rotated) = new_refresh {
        store.set(keys::MICROSOFT_REFRESH_TOKEN, &rotated)?;
    }

    let session = minecraft_session(client, &msa_access).await?;
    let cached = MsSession {
        name: session.profile.name,
        uuid: session.profile.uuid,
        access_token: session.access_token,
        xuid: session.xuid,
    };
    *SESSION.lock().await = Some((
        Instant::now() + Duration::from_secs(session_cache_secs(session.expires_in)),
        cached.clone(),
    ));

    Ok(cached)
}

/// 用 Refresh Token 换新的微软 Access Token。
async fn refresh_msa(
    client: &reqwest::Client,
    refresh_token: &str,
    client_id: &str,
) -> Result<(String, Option<String>, u64)> {
    let resp = post_form(
        client,
        &format!("{AUTH_BASE}/token"),
        &[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ],
    )
    .await
    .map_err(|e| LauncherError::TokenRefreshFailed {
        message: format!("刷新微软令牌失败: {e}"),
    })?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| LauncherError::Parse {
        message: format!("读取令牌刷新结果失败: {e}"),
    })?;

    if !status.is_success() {
        let hint = aad_hint(&text).unwrap_or("登录状态已失效，请重新登录微软账号");
        return Err(LauncherError::TokenRefreshFailed {
            message: format!("{hint}（HTTP {status}）：{}", body_tail(&text)),
        });
    }

    let data: TokenResponse = serde_json::from_str(&text).map_err(|e| LauncherError::Parse {
        message: format!(
            "解析令牌刷新结果失败（HTTP {status}）: {e}；响应：{}",
            body_tail(&text)
        ),
    })?;

    let access = data
        .access_token
        .ok_or_else(|| LauncherError::TokenRefreshFailed {
            message: "微软没有返回访问令牌，请重新登录".into(),
        })?;

    Ok((access, data.refresh_token, data.expires_in.unwrap_or(3600)))
}

/// 保存 Refresh Token 到系统密钥库，并清空内存会话。
pub fn persist_refresh_token(store: &dyn SecureStore, refresh: &str) -> Result<()> {
    store.set(keys::MICROSOFT_REFRESH_TOKEN, refresh)
}

/// 退出登录：删掉凭据并清空缓存。
pub async fn clear_session(store: &dyn SecureStore) -> Result<()> {
    store.delete(keys::MICROSOFT_REFRESH_TOKEN)?;
    *SESSION.lock().await = None;
    Ok(())
}

/// 当前登录态（读设置里的公开资料 + 是否配置了客户端 ID）。
pub fn account_state(settings: &AppSettings) -> AccountState {
    let microsoft = settings.account.microsoft.as_ref();
    let is_microsoft = settings.account.kind == kind::MICROSOFT;
    let offline = super::offline::active(settings);

    AccountState {
        kind: if is_microsoft {
            kind::MICROSOFT.to_string()
        } else {
            kind::OFFLINE.to_string()
        },
        name: match is_microsoft {
            true => microsoft
                .map(|profile| profile.name.clone())
                .unwrap_or_else(|| offline.name.clone()),
            false => offline.name.clone(),
        },
        uuid: match is_microsoft {
            true => microsoft.map(|profile| profile.uuid.clone()),
            false => Some(offline.uuid.clone()),
        },
        microsoft_available: is_configured(),
        credential_store_ephemeral: secure_store::is_ephemeral(),
        offline_accounts: super::offline::profiles(settings),
        active_offline_id: Some(offline.id),
        microsoft_profile: microsoft.cloned(),
    }
}

/// 未配置客户端 ID 时统一给出可操作的错误，而不是让微软接口返回 invalid_client。
fn require_client_id() -> CoreResult<String> {
    if !is_configured() {
        return Err(LauncherError::InvalidConfig {
            key: "microsoft.clientId".into(),
            message: "微软登录尚未配置客户端 ID（SERENO_MS_CLIENT_ID），请先在 Azure 注册应用"
                .into(),
        });
    }
    Ok(client_id())
}

/// 给 32 位十六进制 UUID 补上连字符。
pub fn dash_uuid(raw: &str) -> String {
    let hex: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return raw.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// 发送 `application/x-www-form-urlencoded` POST。
///
/// 不用 `reqwest` 的 `form()`：它需要额外 feature，而这里只需要拼两三个字段，
/// 手动编码反而更透明，也避免多引一个依赖特性。
async fn post_form(
    client: &reqwest::Client,
    url: &str,
    pairs: &[(&str, &str)],
) -> Result<reqwest::Response> {
    let body = pairs
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| LauncherError::Internal {
            message: format!("请求 {url} 失败: {e}"),
        })
}

/// 发送 JSON POST 并解析响应；非 2xx 时尽量提取结构化错误。
async fn post_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<T> {
    // dev 下把请求体打进日志（令牌字段已在写出层脱敏），方便排查 400。
    tracing::debug!(
        url,
        body = %crate::utils::redact::redact_line(
            &serde_json::to_string(body).unwrap_or_default()
        ),
        "Xbox / Minecraft 认证请求"
    );

    let resp = client
        .post(url)
        .header("Accept", "application/json")
        // Xbox Live 的认证接口（user.auth / XSTS）要求带契约版本；
        // 缺这个头会返回一个没有响应体的 400，极难排查。
        .header("x-xbl-contract-version", "1")
        .json(body)
        .send()
        .await
        .map_err(|e| LauncherError::Internal {
            message: format!("请求 {url} 失败: {e}"),
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let raw = body_tail(&text);
        // 各步骤常见错误：XSTS 的 2148xxx 系列、Minecraft 的 403 权限申请
        let hint = if text.contains("2148916233") {
            "该微软账号没有 Xbox 档案，请先在 xbox.com 登录一次以创建档案"
        } else if text.contains("2148916238") {
            "该账号是未成年人账号，需要家长在 Xbox 家庭中同意"
        } else if text.contains("2148916227") {
            "该账号已被 Xbox Live 封禁"
        } else if (status == reqwest::StatusCode::FORBIDDEN || is_pending_registration(&text))
            && url.contains("api.minecraftservices.com")
        {
            MC_API_PENDING
        } else if status == reqwest::StatusCode::BAD_REQUEST && text.trim().is_empty() {
            "Xbox 返回了空响应体的 400。通常是访问令牌未包含 XboxLive.signin 权限或已失效：\
             请重新登录；若反复出现，请检查 Azure 应用的 API 权限里是否有 XboxLive.signin，\
             并确认请求带上了 x-xbl-contract-version: 1"
        } else {
            "账号校验未通过"
        };

        tracing::warn!(
            url,
            status = %status,
            response = %crate::utils::redact::redact_line(&raw),
            "Xbox / Minecraft 认证请求失败"
        );

        return Err(LauncherError::Internal {
            message: format!("{hint}（HTTP {status}，{url}）：{raw}"),
        });
    }

    resp.json().await.map_err(|e| LauncherError::Parse {
        message: format!("解析 {url} 的响应失败: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_uuid() {
        assert_eq!(
            dash_uuid("1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d"),
            "1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d"
        );
        assert_eq!(dash_uuid("not-a-uuid"), "not-a-uuid");
    }

    #[test]
    fn parses_xuid_from_display_claims() {
        let json = r#"{
            "Token": "xbl",
            "DisplayClaims": {"xui": [{"uhs": "12345", "xid": "2535411234567890"}]}
        }"#;
        let response: XboxResponse = serde_json::from_str(json).unwrap();
        let claims = response.display_claims.xui.first().unwrap();
        assert_eq!(claims.uhs, "12345");
        assert_eq!(claims.xid.as_deref(), Some("2535411234567890"));
    }

    #[test]
    fn xbox_auth_body_uses_d_prefixed_ticket() {
        let body = xbox_auth_body("msa-token");
        assert_eq!(body["Properties"]["RpsTicket"], "d=msa-token");
        assert_eq!(body["RelyingParty"], "http://auth.xboxlive.com");
        assert_eq!(body["TokenType"], "JWT");
        assert!(
            body.get("RpsTicket").is_none(),
            "RpsTicket 只应出现在 Properties 里"
        );
    }

    #[test]
    fn xsts_body_declares_minecraft_relying_party() {
        let body = xsts_body("xbl-token");
        assert_eq!(body["Properties"]["SandboxId"], "RETAIL");
        assert_eq!(body["Properties"]["UserTokens"][0], "xbl-token");
        assert_eq!(body["RelyingParty"], "rp://api.minecraftservices.com/");
        assert_eq!(body["TokenType"], "JWT");
    }

    #[test]
    fn recognizes_pending_app_registration() {
        assert!(is_pending_registration("Invalid app registration"));
        assert!(is_pending_registration(
            r#"{"errorMessage":"Application is not approved"}"#
        ));
        assert!(!is_pending_registration("Forbidden"));
    }

    #[test]
    fn explains_common_azure_errors() {
        assert!(
            aad_hint("AADSTS7000218: client_assertion required")
                .unwrap()
                .contains("公共客户端流")
        );
        assert!(
            aad_hint("AADSTS700016: application not found")
                .unwrap()
                .contains("客户端 ID")
        );
        assert!(
            aad_hint("AADSTS65004: user declined")
                .unwrap()
                .contains("取消")
        );
        assert!(aad_hint("some random error").is_none());
    }

    #[test]
    fn truncates_long_error_bodies() {
        assert_eq!(body_tail("line1\nline2"), "line1 line2");
        let long = "x".repeat(400);
        let tail = body_tail(&long);
        assert!(tail.ends_with('…'));
        assert_eq!(tail.chars().count(), 301);
    }

    #[test]
    fn classifies_device_code_poll_errors() {
        assert_eq!(poll_outcome(None), PollOutcome::Authorized);
        assert_eq!(
            poll_outcome(Some("authorization_pending")),
            PollOutcome::Pending
        );
        assert_eq!(poll_outcome(Some("slow_down")), PollOutcome::SlowDown);
        assert_eq!(
            poll_outcome(Some("authorization_declined")),
            PollOutcome::Declined
        );
        assert_eq!(poll_outcome(Some("expired_token")), PollOutcome::Expired);
        assert_eq!(poll_outcome(Some("invalid_client")), PollOutcome::Failed);
    }

    #[test]
    fn session_cache_leaves_five_minutes_of_margin() {
        assert_eq!(session_cache_secs(3600), 3300);
        // 极短令牌至少缓存 60 秒，不能变成负数
        assert_eq!(session_cache_secs(120), 60);
    }

    #[test]
    fn placeholder_means_not_configured() {
        // 占位符必须被视为「未配置」，否则界面会放出注定失败的登录按钮
        assert!(!is_configured());
    }

    #[test]
    fn state_reflects_settings() {
        let mut settings = AppSettings::default();
        let state = account_state(&settings);
        assert_eq!(state.kind, kind::OFFLINE);
        assert_eq!(state.name, "Player");
        // 离线档案也会固化 UUID，启动时直接使用，不再按名字临时派生
        let expected = crate::launch::args::offline_uuid("Player");
        assert_eq!(state.uuid.as_deref(), Some(expected.as_str()));
        assert_eq!(state.offline_accounts.len(), 1);

        settings.account.kind = kind::MICROSOFT.to_string();
        settings.account.microsoft = Some(MicrosoftProfile {
            name: "Steve".into(),
            uuid: "8667ba71-b85a-4004-af54-457a9734eed7".into(),
        });
        let state = account_state(&settings);
        assert_eq!(state.name, "Steve");
        assert_eq!(
            state.uuid.as_deref(),
            Some("8667ba71-b85a-4004-af54-457a9734eed7")
        );
        // 离线档案仍然保留，切回来不用重建
        assert_eq!(state.offline_accounts.len(), 1);
    }
}
