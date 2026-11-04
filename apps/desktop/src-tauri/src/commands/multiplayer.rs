//! 联机命令：开房 / 加入 / 停止 / 状态。
//!
//! 薄适配：校验参数 → 调 `SessionManager` → 返回状态快照。
//! 会话的状态变化通过**轮询** `get_multiplayer_status` 拉取
//! （1–2 秒一次，负载可忽略）；不引入事件推送，避免为低频状态
//! 再加一条事件通道。

use crate::state::AppState;
use sereno_core::Result;
use sereno_core::multiplayer::MultiplayerStatus;

/// 房主开房。
///
/// `room_code` 可为空：为空时生成新房间码。
/// `mc_port`：游戏世界端口（专用服务端 25565；单人世界「对局域网开放」
/// 后聊天栏显示的随机端口）。
#[tauri::command]
#[specta::specta]
pub async fn host_multiplayer(
    room_code: Option<String>,
    mc_port: u16,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<MultiplayerStatus> {
    let code = match room_code.as_deref() {
        Some(code) if !code.trim().is_empty() => code.to_string(),
        _ => sereno_core::multiplayer::room_code::generate(),
    };

    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let bundled = bundled_kernel_dir(&app);
    let status = state
        .multiplayer
        .host(&client, &code, mc_port, bundled.as_deref())
        .await?;

    tracing::info!(room = %code, mc_port, "联机房间已创建");
    Ok(status)
}

/// 房客加入房间。
///
/// `room_code` 必填（支持小写 / 混淆字符 / 缺分隔符，核心会纠偏）。
#[tauri::command]
#[specta::specta]
pub async fn join_multiplayer(
    room_code: String,
    mc_port: u16,
    tun_mode: Option<bool>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<MultiplayerStatus> {
    if room_code.trim().is_empty() {
        return Err(sereno_core::LauncherError::InvalidConfig {
            key: "roomCode".into(),
            message: "请输入房间码".into(),
        });
    }

    let settings = state.snapshot_settings()?;
    let client = sereno_core::utils::http::build_client(&settings)?;
    let bundled = bundled_kernel_dir(&app);
    state
        .multiplayer
        .guest(
            &client,
            &room_code,
            mc_port,
            bundled.as_deref(),
            tun_mode.unwrap_or(false),
        )
        .await
}

/// 随安装包分发的内核目录（构建期由 `scripts/fetch-easytier.mjs` 生成，
/// 含 easytier-core.exe + wintun.dll 等运行时依赖）。
///
/// 开发模式（未打包）下资源目录里没有这个文件，返回 `None` 走在线下载。
fn bundled_kernel_dir(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager as _;
    app.path()
        .resolve("easytier", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.is_dir() && p.join("easytier-core.exe").is_file())
}

/// 停止当前联机会话。
#[tauri::command]
#[specta::specta]
pub async fn stop_multiplayer(state: tauri::State<'_, AppState>) -> Result<()> {
    state.multiplayer.stop().await
}

/// 自动探测本机游戏世界端口。
///
/// 枚举 Java 进程的监听端口并逐个做 MC 状态探测；命中即返回端口与世界信息。
/// 游戏未开 / 未「对局域网开放」时返回 `None`，前端保持手动输入兜底。
#[tauri::command]
#[specta::specta]
pub async fn detect_world_port() -> Result<Option<sereno_core::multiplayer::DetectedWorld>> {
    sereno_core::multiplayer::detect::detect_world().await
}

/// 查询联机状态（前端 1–2 秒轮询一次）。
#[tauri::command]
#[specta::specta]
pub fn get_multiplayer_status(state: tauri::State<'_, AppState>) -> MultiplayerStatus {
    state.multiplayer.status()
}
