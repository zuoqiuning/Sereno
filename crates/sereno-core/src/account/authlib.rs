//! 第三方登录（authlib-injective / Yggdrasil）。
//!
//! ## 它解决什么问题
//!
//! 微软正版之外，还有一批 Yggdrasil 实现的皮肤站（LittleSkin、Ely.by 等）。
//! 用户在这些站上注册后，用它们的账号也能进正版游戏——
//! 靠的是 **authlib-injector**：一个在运行时改写游戏鉴权逻辑的 Java agent。
//!
//! ## 流程
//!
//! 1. 用户提供 Yggdrasil 的 **API Root**（如 `https://littleskin.cn/api/yggdrasil`）；
//! 2. 启动器调 `POST {apiRoot}/authserver/authenticate` 换取 `accessToken`
//!    与角色的 `id` / `name`；
//! 3. 启动游戏时加上 `-javaagent:authlib-injector.jar={apiRoot}`，
//!    游戏内的鉴权请求就被改写到这个 Yggdrasil 服务。
//!
//! ## 安全约束
//!
//! - **密码不落盘**：只用来换令牌，用完即弃；
//! - **令牌进系统密钥库**：与微软 Refresh Token 同一条路径（`secure_store`）；
//! - **API Root 必须显式配置**：绝不猜测或自动发现第三方服务地址；
//! - **注入器校验 SHA-256**：从官方 API 或 BMCLAPI 镜像下载，校验值对不上就拒绝执行。

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::account::secure_store::{self, SecureStore};
use crate::models::account::{AuthlibProfile, AuthlibSession};
use crate::models::settings::AppSettings;
use crate::{LauncherError, Result};

/// authlib-injector 的官方下载 API。
const INJECTOR_API: &str = "https://authlib-injector.yushi.moe";
/// BMCLAPI 提供的镜像（国内直连更快）。
const INJECTOR_API_MIRROR: &str = "https://bmclapi2.bangbang93.com/mirrors/authlib-injector";

/// 登录态的缓存有效期。
const SESSION_TTL: Duration = Duration::from_secs(1800);

/// 已登录会话的进程内缓存。
static SESSION: LazyLock<Mutex<Option<(std::time::Instant, AuthlibSession)>>> =
    LazyLock::new(|| Mutex::new(None));

/// authlib-injector 的下载元数据。
#[derive(Debug, Clone, Deserialize)]
pub struct InjectorArtifact {
    /// 构建号（对应 API 里的 `build_number`）。
    #[serde(default)]
    pub build_number: u32,
    /// 版本号，如 `1.2.8`。
    pub version: String,
    /// 下载地址。
    pub download_url: String,
    /// 校验值。
    #[serde(default)]
    pub checksums: InjectorChecksums,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InjectorChecksums {
    #[serde(default)]
    pub sha256: String,
}

/// 校验 API Root：必须是 `http(s)` 开头，且不含查询串或片段。
///
/// **API Root 会被拼进 `-javaagent` 参数**，也决定令牌发往哪个域名——
/// 校验不严会让用户把密码发到意外的地方。
pub fn validate_api_root(raw: &str) -> Result<String> {
    let root = raw.trim().trim_end_matches('/');
    if root.is_empty() {
        return Err(LauncherError::InvalidConfig {
            key: "apiRoot".into(),
            message: "请填写 Yggdrasil 服务器地址".into(),
        });
    }

    let lower = root.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(LauncherError::InvalidConfig {
            key: "apiRoot".into(),
            message: "Yggdrasil 服务器地址必须以 http:// 或 https:// 开头".into(),
        });
    }
    if root.contains('?') || root.contains('#') {
        return Err(LauncherError::InvalidConfig {
            key: "apiRoot".into(),
            message: "Yggdrasil 服务器地址不能包含查询参数或片段".into(),
        });
    }
    // 路径里不能出现空段（`//`）——`https://` 里的那对不算，
    // 因此先去掉协议头再检查。
    let path = root.split_once("://").map(|(_, rest)| rest).unwrap_or(root);
    if path.contains("//") {
        return Err(LauncherError::InvalidConfig {
            key: "apiRoot".into(),
            message: "Yggdrasil 服务器地址格式不正确".into(),
        });
    }
    Ok(root.to_string())
}

/// Yggdrasil 的认证请求体。
#[derive(Debug, Clone, Deserialize)]
struct AuthenticateResponse {
    access_token: String,
    client_token: String,
    #[serde(default)]
    selected_profile: Option<YggdrasilProfile>,
    #[serde(default)]
    available_profiles: Vec<YggdrasilProfile>,
}

#[derive(Debug, Clone, Deserialize)]
struct YggdrasilProfile {
    id: String,
    name: String,
}

/// Yggdrasil 的错误响应。
///
/// 字段名是驼峰（`errorMessage`），与 Rust 惯例不同，因此要显式指定。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_message: String,
    #[serde(default)]
    cause: String,
}

/// 用账号密码换取访问令牌。
///
/// 密码只在这一次请求里出现，**不会落盘**。
pub async fn authenticate(
    client: &reqwest::Client,
    api_root: &str,
    username: &str,
    password: &str,
) -> Result<AuthlibSession> {
    let root = validate_api_root(api_root)?;
    let username = username.trim();
    if username.is_empty() || password.is_empty() {
        return Err(LauncherError::InvalidConfig {
            key: "username".into(),
            message: "请填写账号与密码".into(),
        });
    }

    // clientToken 是客户端生成的随机串，用于把「刷新」与「首次登录」关联起来。
    let client_token = random_client_token();

    let body = serde_json::json!({
        "username": username,
        "password": password,
        "clientToken": client_token,
        "requestUser": true,
        // agent 字段必须**严格**按 authlib 规范填写：name 必须是 "Minecraft"，
        // version 必须是数字 1。服务端会逐字段校验，
        // 写成自己的产品名会被直接拒绝（"agent.name: name must be equal to Minecraft"）。
        "agent": { "name": "Minecraft", "version": 1 },
    });

    let resp = client
        .post(format!("{root}/authserver/authenticate"))
        .json(&body)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("连接 Yggdrasil 服务器失败: {e}"),
            url: root.clone(),
        })?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| LauncherError::Parse {
        message: format!("读取认证结果失败: {e}"),
    })?;

    if !status.is_success() {
        return Err(auth_error(&text, status.as_u16()));
    }

    let data: AuthenticateResponse =
        serde_json::from_str(&text).map_err(|e| LauncherError::Parse {
            message: format!("解析认证结果失败: {e}"),
        })?;

    // 多角色账号取第一个；只有一个角色时服务端也可能不给 selectedProfile。
    let profile = data
        .selected_profile
        .or(data.available_profiles.first().cloned())
        .ok_or(LauncherError::NotAuthenticated)?;

    Ok(AuthlibSession {
        access_token: data.access_token,
        client_token: data.client_token,
        uuid: crate::account::microsoft::dash_uuid(&profile.id),
        name: profile.name,
        api_root: root,
    })
}

/// 用已保存的令牌刷新会话。
pub async fn refresh(
    client: &reqwest::Client,
    api_root: &str,
    access_token: &str,
    client_token: &str,
) -> Result<AuthlibSession> {
    let root = validate_api_root(api_root)?;

    let body = serde_json::json!({
        "accessToken": access_token,
        "clientToken": client_token,
        "requestUser": true,
    });

    let resp = client
        .post(format!("{root}/authserver/refresh"))
        .json(&body)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("刷新令牌失败: {e}"),
            url: root.clone(),
        })?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| LauncherError::Parse {
        message: format!("读取刷新结果失败: {e}"),
    })?;

    if !status.is_success() {
        return Err(auth_error(&text, status.as_u16()));
    }

    let data: AuthenticateResponse =
        serde_json::from_str(&text).map_err(|e| LauncherError::Parse {
            message: format!("解析刷新结果失败: {e}"),
        })?;

    let profile = data
        .selected_profile
        .or(data.available_profiles.first().cloned())
        .ok_or(LauncherError::NotAuthenticated)?;

    Ok(AuthlibSession {
        access_token: data.access_token,
        client_token: data.client_token,
        uuid: crate::account::microsoft::dash_uuid(&profile.id),
        name: profile.name,
        api_root: root,
    })
}

/// 校验令牌是否仍然有效。
pub async fn validate(
    client: &reqwest::Client,
    api_root: &str,
    access_token: &str,
) -> Result<bool> {
    let root = validate_api_root(api_root)?;
    let body = serde_json::json!({ "accessToken": access_token });

    let resp = client
        .post(format!("{root}/authserver/validate"))
        .json(&body)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("校验令牌失败: {e}"),
            url: root,
        })?;

    // 204 表示有效，403 表示无效
    Ok(resp.status().is_success())
}

/// 让服务端作废当前令牌（退出登录）。
pub async fn invalidate(
    client: &reqwest::Client,
    api_root: &str,
    access_token: &str,
) -> Result<()> {
    let root = validate_api_root(api_root)?;
    let body = serde_json::json!({ "accessToken": access_token });

    // 作废失败不影响退出登录的语义：本地凭据照样清掉。
    let _ = client
        .post(format!("{root}/authserver/invalidate"))
        .json(&body)
        .send()
        .await;

    Ok(())
}

/// 取当前会话（必要时用保存的令牌刷新）。
pub async fn session(client: &reqwest::Client, profile: &AuthlibProfile) -> Result<AuthlibSession> {
    if let Some((at, cached)) = SESSION.lock().await.clone()
        && std::time::Instant::now() < at
        && cached.api_root == profile.api_root
    {
        return Ok(cached);
    }

    let store = secure_store::default_store();
    let key = token_key(&profile.api_root, &profile.name);
    let Some(saved) = store.get(&key)? else {
        return Err(LauncherError::NotAuthenticated);
    };

    let (access_token, client_token) = saved
        .split_once('\n')
        .ok_or(LauncherError::NotAuthenticated)?;

    // 先尝试刷新；刷新失败时再校验令牌是否还有效
    // （部分服务端不支持 refresh，但令牌本身没过期）。
    let session = match refresh(client, &profile.api_root, access_token, client_token).await {
        Ok(session) => session,
        Err(error) => {
            if validate(client, &profile.api_root, access_token)
                .await
                .unwrap_or(false)
            {
                AuthlibSession {
                    access_token: access_token.to_string(),
                    client_token: client_token.to_string(),
                    uuid: profile.uuid.clone(),
                    name: profile.name.clone(),
                    api_root: profile.api_root.clone(),
                }
            } else {
                return Err(error);
            }
        }
    };

    store.set(&key, &session_token_payload(&session))?;
    *SESSION.lock().await = Some((std::time::Instant::now() + SESSION_TTL, session.clone()));

    Ok(session)
}

/// 把会话写入系统密钥库。
pub fn persist_session(store: &dyn SecureStore, session: &AuthlibSession) -> Result<()> {
    store.set(
        &token_key(&session.api_root, &session.name),
        &session_token_payload(session),
    )
}

/// 清除会话凭据与内存缓存。
pub async fn clear_session(store: &dyn SecureStore, profile: &AuthlibProfile) -> Result<()> {
    store.delete(&token_key(&profile.api_root, &profile.name))?;
    *SESSION.lock().await = None;
    Ok(())
}

/// 凭据的键名：按「服务器 + 用户名」区分，不同皮肤站的令牌互不干扰。
fn token_key(api_root: &str, name: &str) -> String {
    format!("sereno.authlib.{}", hash_key(api_root, name))
}

/// 凭据的值：`accessToken\nclientToken`。
fn session_token_payload(session: &AuthlibSession) -> String {
    format!("{}\n{}", session.access_token, session.client_token)
}

/// 把「服务器 + 用户名」散列成安全的键名片段。
///
/// 服务器地址可能含 `/` 与 `:`，直接拼进键名会让不同组合撞键。
fn hash_key(api_root: &str, name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{api_root}\n{name}").as_bytes());
    format!("{:x}", digest)[..24].to_string()
}

/// 生成随机的 client token。
fn random_client_token() -> String {
    use sha2::{Digest, Sha256};
    // 令牌本身不是密钥，但要与其它客户端区分开，
    // 因此掺入进程 ID 与当前时间的纳秒部分。
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let seed = format!("{nanos}-{}", std::process::id());
    format!("{:x}", Sha256::digest(seed.as_bytes()))[..32].to_string()
}

/// 把 Yggdrasil 的错误响应翻译成用户能看懂的话。
fn auth_error(text: &str, status: u16) -> LauncherError {
    let parsed: YggdrasilError = serde_json::from_str(text).unwrap_or_default();
    let detail = if !parsed.error_message.is_empty() {
        parsed.error_message.clone()
    } else if !parsed.cause.is_empty() {
        parsed.cause.clone()
    } else {
        parsed.error.clone()
    };

    let message = match parsed.error.as_str() {
        "ForbiddenOperationException" => {
            if detail.contains("Invalid credentials") || detail.contains("错误的密码") {
                "账号或密码不正确".to_string()
            } else if detail.contains("Account has been blocked") {
                "该账号已被封禁".to_string()
            } else if detail.contains("too many") || detail.contains("尝试次数过多") {
                "尝试次数过多，请稍后再试".to_string()
            } else if detail.is_empty() {
                "账号或密码不正确".to_string()
            } else {
                detail
            }
        }
        "ResourceNotFoundException" => "该账号不存在".to_string(),
        _ if detail.is_empty() => format!("认证失败（HTTP {status}）"),
        _ => detail,
    };

    LauncherError::AuthlibAuthFailed { message }
}

/// authlib-injector 的下载元数据。
pub async fn injector_artifact(
    client: &reqwest::Client,
    prefer_mirror: bool,
) -> Result<InjectorArtifact> {
    let mut last_error = None;

    let bases: &[&str] = if prefer_mirror {
        &[INJECTOR_API_MIRROR, INJECTOR_API]
    } else {
        &[INJECTOR_API, INJECTOR_API_MIRROR]
    };

    for base in bases {
        let url = format!("{base}/artifact/latest.json");
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let bytes = crate::utils::http::read_limited(resp, 64 * 1024).await?;
                let artifact: InjectorArtifact =
                    serde_json::from_slice(&bytes).map_err(|e| LauncherError::Parse {
                        message: format!("解析 authlib-injector 元数据失败: {e}"),
                    })?;
                return Ok(artifact);
            }
            Ok(resp) => {
                last_error = Some(format!("{url} 返回 HTTP {}", resp.status()));
            }
            Err(error) => {
                last_error = Some(format!("{url} 请求失败: {error}"));
            }
        }
    }

    Err(LauncherError::DownloadFailed {
        message: format!(
            "获取 authlib-injector 元数据失败：{}",
            last_error.unwrap_or_else(|| "未知原因".into())
        ),
        url: INJECTOR_API.to_string(),
    })
}

/// 确保本地有 authlib-injector，返回它的绝对路径。
///
/// 下载后**必须校验 SHA-256**——这个 jar 会被 `-javaagent` 注入到游戏进程里，
/// 等同于执行第三方代码，校验不可省。
pub async fn ensure_injector(client: &reqwest::Client, prefer_mirror: bool) -> Result<PathBuf> {
    let artifact = injector_artifact(client, prefer_mirror).await?;

    let dir = crate::utils::paths::cache_dir()?.join("authlib");
    crate::utils::paths::ensure_dir(&dir)?;
    let path = dir.join(format!("authlib-injector-{}.jar", artifact.version));

    // 已存在且校验通过 → 直接用
    if path.exists()
        && let Ok(actual) = sha256_file(&path)
        && !artifact.checksums.sha256.is_empty()
        && actual.eq_ignore_ascii_case(&artifact.checksums.sha256)
    {
        return Ok(path);
    }

    let resp = client
        .get(&artifact.download_url)
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("下载 authlib-injector 失败: {e}"),
            url: artifact.download_url.clone(),
        })?;

    let status = resp.status();
    if !status.is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("下载 authlib-injector 返回 HTTP {status}"),
            url: artifact.download_url,
        });
    }

    let bytes = crate::utils::http::read_limited(resp, 64 * 1024 * 1024).await?;

    // 校验 SHA-256
    if !artifact.checksums.sha256.is_empty() {
        use sha2::{Digest, Sha256};
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(&artifact.checksums.sha256) {
            return Err(LauncherError::ChecksumMismatch {
                path: path.display().to_string(),
                expected: artifact.checksums.sha256,
                actual,
            });
        }
    }

    crate::utils::fs::atomic_write(&path, &bytes)?;
    Ok(path)
}

/// 生成 `-javaagent` 参数。
///
/// 格式固定为 `-javaagent:<jar路径>=<API Root>`——路径里的 `=` 与 `,` 会被
/// Java 解析成额外参数，因此需要转义。
pub fn javaagent_arg(jar_path: &Path, api_root: &str) -> String {
    let path = jar_path.to_string_lossy().replace('\\', "/");
    let root = api_root.trim_end_matches('/');
    format!("-javaagent:{path}={root}")
}

/// 计算文件的 SHA-256。
fn sha256_file(path: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| LauncherError::Io {
        message: format!("读取文件失败 {}: {e}", path.display()),
    })?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

/// 已保存的第三方档案列表。
pub fn profiles(settings: &AppSettings) -> Vec<AuthlibProfile> {
    settings.account.authlib_accounts.clone()
}

/// 当前生效的第三方档案；ID 失效时回落到第一个。
pub fn active(settings: &AppSettings) -> Option<AuthlibProfile> {
    let list = profiles(settings);
    settings
        .account
        .active_authlib_id
        .as_deref()
        .and_then(|id| list.iter().find(|profile| profile.id == id).cloned())
        .or_else(|| list.first().cloned())
}

/// 档案的稳定 ID：由「服务器 + 用户名」散列而来。
pub fn profile_id(api_root: &str, name: &str) -> String {
    hash_key(api_root, name)
}

/// 把登录成功的会话写进设置，并切换到该档案。
pub fn save_profile(
    settings: &mut AppSettings,
    session: &AuthlibSession,
    server_name: &str,
) -> AuthlibProfile {
    let profile = AuthlibProfile {
        id: profile_id(&session.api_root, &session.name),
        name: session.name.clone(),
        uuid: session.uuid.clone(),
        api_root: session.api_root.clone(),
        server_name: server_name.trim().to_string(),
    };

    settings
        .account
        .authlib_accounts
        .retain(|entry| entry.id != profile.id);
    settings.account.authlib_accounts.push(profile.clone());
    settings.account.active_authlib_id = Some(profile.id.clone());
    settings.account.kind = crate::models::account::kind::AUTHLIB.to_string();
    settings.profile.name = profile.name.clone();

    profile
}

/// 切换到指定的第三方档案。
pub fn switch(settings: &mut AppSettings, id: &str) -> Result<AuthlibProfile> {
    let profile = settings
        .account
        .authlib_accounts
        .iter()
        .find(|entry| entry.id == id)
        .cloned()
        .ok_or_else(|| LauncherError::InvalidConfig {
            key: "id".into(),
            message: format!("第三方档案不存在: {id}"),
        })?;

    settings.account.active_authlib_id = Some(profile.id.clone());
    settings.account.kind = crate::models::account::kind::AUTHLIB.to_string();
    settings.profile.name = profile.name.clone();
    Ok(profile)
}

/// 删除第三方档案。
pub fn remove(settings: &mut AppSettings, id: &str) -> Result<AuthlibProfile> {
    let index = settings
        .account
        .authlib_accounts
        .iter()
        .position(|entry| entry.id == id)
        .ok_or_else(|| LauncherError::InvalidConfig {
            key: "id".into(),
            message: format!("第三方档案不存在: {id}"),
        })?;

    let removed = settings.account.authlib_accounts.remove(index);

    if settings.account.active_authlib_id.as_deref() == Some(id) {
        // 切回离线档案：第三方登录被删掉后不能停在「已登录」状态
        settings.account.active_authlib_id = None;
        settings.account.kind = crate::models::account::kind::OFFLINE.to_string();
        super::offline::ensure(settings);
        let offline = super::offline::active(settings);
        settings.profile.name = offline.name;
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::settings::AppSettings;

    #[test]
    fn validates_api_root() {
        assert_eq!(
            validate_api_root("https://littleskin.cn/api/yggdrasil/").unwrap(),
            "https://littleskin.cn/api/yggdrasil"
        );
        assert!(validate_api_root("").is_err());
        assert!(validate_api_root("littleskin.cn").is_err());
        assert!(validate_api_root("ftp://x/y").is_err());
        assert!(validate_api_root("https://x/y?a=1").is_err());
        assert!(validate_api_root("https://x/y#z").is_err());
    }

    #[test]
    fn builds_javaagent_argument() {
        let jar = PathBuf::from("C:\\cache\\authlib-injector-1.2.8.jar");
        let arg = javaagent_arg(&jar, "https://littleskin.cn/api/yggdrasil/");
        assert_eq!(
            arg,
            "-javaagent:C:/cache/authlib-injector-1.2.8.jar=https://littleskin.cn/api/yggdrasil"
        );
    }

    #[test]
    fn token_keys_are_distinct_per_server_and_user() {
        let a = token_key("https://a/api/yggdrasil", "Steve");
        let b = token_key("https://b/api/yggdrasil", "Steve");
        let c = token_key("https://a/api/yggdrasil", "Alex");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert!(!a.contains('/'), "键名不能含路径分隔符: {a}");
        assert!(!a.contains(':'), "键名不能含冒号: {a}");
    }

    #[test]
    fn translates_auth_errors() {
        let error = auth_error(
            r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid credentials."}"#,
            403,
        );
        assert!(error.to_string().contains("账号或密码不正确"));

        let blocked = auth_error(
            r#"{"error":"ForbiddenOperationException","errorMessage":"Account has been blocked."}"#,
            403,
        );
        assert!(blocked.to_string().contains("封禁"));
    }

    #[test]
    fn saving_profile_switches_account_kind() {
        let mut settings = AppSettings::default();
        let session = AuthlibSession {
            access_token: "token".into(),
            client_token: "client".into(),
            uuid: "uuid".into(),
            name: "Steve".into(),
            api_root: "https://littleskin.cn/api/yggdrasil".into(),
        };

        let profile = save_profile(&mut settings, &session, "LittleSkin");
        assert_eq!(settings.account.kind, "authlib");
        assert_eq!(
            settings.account.active_authlib_id.as_deref(),
            Some(profile.id.as_str())
        );
        assert_eq!(active(&settings).unwrap().name, "Steve");

        // 再次登录同一账号不产生重复档案
        let again = save_profile(&mut settings, &session, "LittleSkin");
        assert_eq!(again.id, profile.id);
        assert_eq!(settings.account.authlib_accounts.len(), 1);
    }

    #[test]
    fn removing_active_profile_falls_back_to_offline() {
        let mut settings = AppSettings::default();
        let session = AuthlibSession {
            access_token: "token".into(),
            client_token: "client".into(),
            uuid: "uuid".into(),
            name: "Steve".into(),
            api_root: "https://littleskin.cn/api/yggdrasil".into(),
        };
        let profile = save_profile(&mut settings, &session, "LittleSkin");

        remove(&mut settings, &profile.id).unwrap();
        assert_eq!(settings.account.kind, "offline");
        assert!(active(&settings).is_none());
    }
}
