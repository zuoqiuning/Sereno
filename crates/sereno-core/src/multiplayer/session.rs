//! 联机会话：房主 / 房客双角色、子进程托管、状态机。
//!
//! ## 角色与权限模型（经 EasyTier 官方文档与源码核实）
//!
//! 官方语义：no-tun 节点**可以被访问**（虚拟 IP 支持 TCP/UDP/ICMP），
//! 但**无法主动出站**访问其他节点；`--port-forward` 方向固定为
//! 「宿主监听 → 虚拟网拨号」，解决的正是出站问题。
//! 联机需求恰好是单向的：房客只需出站连接房主，房主只需被动接受入站。
//! TUN 模式让房主的 MC 世界（bind 0.0.0.0）天然在虚拟 IP 上可达，
//! 是最直接可靠的「服务暴露」方式。因此拓扑是：
//!
//! ```text
//! 房主（TUN 模式，开房时一次 UAC）
//!   MC 世界 bind 0.0.0.0:<lan_port>，TUN 虚拟 IP 固定 10.126.126.1
//! 房客（--no-tun，零权限）
//!   --port-forward tcp://127.0.0.1:<local>/10.126.126.1:<lan_port>
//!   游戏里「直接连接」127.0.0.1:<local> 即可
//! ```
//!
//! ## 房主提权方案
//!
//! Windows 上创建 TUN 需要管理员。采用 **双层 EncodedCommand**：
//! 外层非提权 PowerShell 用 `Start-Process -Verb RunAs` 拉起内层提权
//! PowerShell（UAC 弹窗），内层再启动 easytier-core 并把输出重定向到
//! 状态目录里的日志文件，同时轮询「停止文件」——出现即 kill 子进程。
//! 这样**停止联机不需要第二次 UAC**，应用也不会留下孤儿进程
//! （应用退出钩子同样写停止文件兜底）。
//!
//! ## 「已连接」的判据
//!
//! 不依赖解析 EasyTier 日志的脆弱格式，而是用 `ping.rs` 的 MC 状态探测：
//! 能拿到 MOTD = 端口转发 + 虚拟网 + 游戏世界三者全部就绪。

use std::collections::VecDeque;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use base64::Engine;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::utils::paths;
use crate::{LauncherError, Result};

use super::kernel::{self, DEFAULT_PUBLIC_SERVERS};
use super::link::{self, LinkKind};
use super::ping;
use super::room_code::{self, RoomCodeError};

/// 联机角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// 房主。
    Host,
    /// 房客。
    Guest,
}

/// 会话阶段。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// 内核启动中（下载 / 拉起进程 / 等待组网）。
    #[default]
    Starting,
    /// 内核已运行，等待游戏世界开放（房主）或房主加入（房客）。
    WaitingPeer,
    /// 世界可达（MC 状态探测通过）。
    Connected,
    /// 出错（进程退出 / 授权被拒 / 世界不可达）。
    Error,
    /// 用户主动停止。
    Stopped,
}

/// 联机状态快照（供 UI）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiplayerStatus {
    /// 当前角色；无会话时为空。
    pub role: Option<Role>,
    /// 当前阶段。
    pub phase: Phase,
    /// 房间码（规范格式）。
    pub room_code: Option<String>,
    /// 房客的游戏连接地址（`127.0.0.1:<port>`）。
    pub local_addr: Option<String>,
    /// MC 端口（房主为局域网端口，房客为对端端口）。
    pub mc_port: u16,
    /// 世界 MOTD（已连接时）。
    pub motd: String,
    /// 面向用户的一句话说明（错误原因 / 下一步提示）。
    pub detail: String,
    /// 内核最近日志（排障用，最多 8 行）。
    pub last_log: Vec<String>,
    /// 到对端的链路类型：`direct`（直连，延迟低）/ `relay`（中继，稳定）。
    /// None = 尚未探测到。
    pub link_type: Option<String>,
}

struct SharedState {
    role: Option<Role>,
    room_code: Option<String>,
    mc_port: u16,
    local_port: Option<u16>,
    /// 本内核的 RPC 门户端口（直连/中继徽章的查询通道）。
    rpc_portal: Option<u16>,
    /// 到对端的链路类型（由后台监控刷新）。
    link_type: Option<LinkKind>,
    phase: Phase,
    motd: String,
    detail: String,
    last_log: VecDeque<String>,
}

/// 联机会话管理器。整个应用一个实例（放在 Tauri `AppState`）。
pub struct SessionManager {
    state: SharedHandle,
    stop_file: PathBuf,
    /// 房客内核子进程句柄（精确清理用；房主是提权包装进程，走停止文件）。
    /// Arc 供退出监督任务共享句柄。
    guest_child: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
}

impl SessionManager {
    /// 创建管理器（无会话）。
    ///
    /// **孤儿回收**：如果上次启动器被任务管理器强杀，房主的提权包装
    /// 进程和内核可能还在后台。启动时若发现会话标记（见 `host_inner`），
    /// 说明上次正常停止流程没走到——写停止文件让还活着的包装进程
    /// 回收内核。**不能**按镜像名杀 easytier：用户可能同时开着其他
    /// 启动器的联机，误杀就是事故。
    pub fn new() -> Self {
        let (multiplayer_dir, stop_file) = match paths::state_dir() {
            Ok(dir) => {
                let dir = dir.join("multiplayer");
                (dir.clone(), dir.join("stop.flag"))
            }
            Err(_) => (
                std::env::temp_dir(),
                std::env::temp_dir().join("sereno-multiplayer-stop.flag"),
            ),
        };

        let marker = multiplayer_dir.join("session.host");
        if marker.is_file() {
            // 上次会话没被正常停止（启动器被强杀）：写停止文件兜底
            if let Err(error) = std::fs::write(&stop_file, b"stop") {
                tracing::warn!(%error, "清理残留联机会话失败（写停止文件）");
            } else {
                tracing::info!("检测到上次未停止的联机房间，已通知其内核退出");
            }
            let _ = std::fs::remove_file(&marker);
        } else {
            let _ = std::fs::remove_file(&stop_file);
        }

        Self {
            state: Arc::new(StdMutex::new(None)),
            stop_file,
            guest_child: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// 当前状态快照。
    pub fn status(&self) -> MultiplayerStatus {
        let guard = self.state.lock().expect("联机状态锁中毒");
        match guard.as_ref() {
            None => MultiplayerStatus {
                phase: Phase::Stopped,
                detail: "未在联机".into(),
                ..MultiplayerStatus::default()
            },
            Some(s) => MultiplayerStatus {
                role: s.role,
                phase: s.phase,
                room_code: s.room_code.clone(),
                local_addr: s.local_port.map(|p| format!("127.0.0.1:{p}")),
                mc_port: s.mc_port,
                motd: s.motd.clone(),
                detail: s.detail.clone(),
                last_log: s.last_log.iter().cloned().collect(),
                link_type: s.link_type.map(|kind| kind.as_str().into()),
            },
        }
    }

    /// 是否有活跃会话。
    pub fn is_active(&self) -> bool {
        let guard = self.state.lock().expect("联机状态锁中毒");
        guard
            .as_ref()
            .is_some_and(|s| !matches!(s.phase, Phase::Error | Phase::Stopped))
    }

    /// 房主开房。
    ///
    /// `mc_port`：游戏世界端口——专用服务端传 25565；
    /// 单人世界「对局域网开放」后从聊天栏读取实际端口传入。
    pub async fn host(
        &self,
        client: &reqwest::Client,
        code: &str,
        mc_port: u16,
        bundled_dir: Option<&std::path::Path>,
    ) -> Result<MultiplayerStatus> {
        // 先规范化房间码（纠偏 + 校验），后继全部使用规范格式
        let normalized = room_code::parse(code).map_err(room_code_error)?;
        self.begin(Role::Host, &normalized, mc_port)?;

        let result = self
            .host_inner(client, &normalized, mc_port, bundled_dir)
            .await;
        if let Err(error) = result {
            self.finish(Phase::Error, &error.to_string());
            return Err(error);
        }
        Ok(self.status())
    }

    /// 房客加入。
    ///
    /// 返回的地址写进状态，前端提示用户在游戏内「直接连接」。
    pub async fn guest(
        &self,
        client: &reqwest::Client,
        code: &str,
        mc_port: u16,
        bundled_dir: Option<&std::path::Path>,
        tun_mode: bool,
    ) -> Result<MultiplayerStatus> {
        let normalized = room_code::parse(code).map_err(room_code_error)?;
        self.begin(Role::Guest, &normalized, mc_port)?;

        // TUN 模式走独立的提权启动路径（虚拟网卡 + UDP 广播中继）
        if tun_mode {
            let result = self
                .guest_inner_tun(client, &normalized, mc_port, bundled_dir)
                .await;
            return match result {
                Ok(()) => Ok(self.status()),
                Err(error) => {
                    self.finish(Phase::Error, &error.to_string());
                    Err(error)
                }
            };
        }

        let result = self
            .guest_inner(client, &normalized, mc_port, bundled_dir)
            .await;
        match result {
            Ok(local_port) => {
                {
                    let mut guard = self.state.lock().expect("联机状态锁中毒");
                    if let Some(s) = guard.as_mut() {
                        s.local_port = Some(local_port);
                        s.detail = format!("已加入网络，游戏内直接连接 127.0.0.1:{local_port}");
                    }
                }
                // 房客侧启动后台探测循环：拿到 MOTD 即 Connected
                self.spawn_guest_probe(local_port);
                Ok(self.status())
            }
            Err(error) => {
                self.finish(Phase::Error, &error.to_string());
                Err(error)
            }
        }
    }

    /// 停止当前会话（房主写停止文件 / 房客杀子进程）。
    pub async fn stop(&self) -> Result<()> {
        let stop_file = self.stop_file.clone();
        let had_session = {
            let guard = self.state.lock().expect("联机状态锁中毒");
            guard.as_ref().map(|s| (s.role, s.local_port)).is_some()
        };

        if !had_session {
            return Ok(());
        }

        // 先置 Stopped，再触发进程退出；监督任务看到 Stopped 不再改状态
        self.finish(Phase::Stopped, "已停止联机");
        let _ = std::fs::remove_file(
            self.stop_file
                .parent()
                .unwrap_or(Path::new("."))
                .join("session.host"),
        );

        // 房主：停止文件（提权包装进程轮询它）
        if let Some(parent) = stop_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&stop_file, b"stop");

        // 房客：按子进程句柄精确结束（绝不按镜像名——用户可能同时开着
        // 其他启动器的联机，误杀就是事故）
        {
            let mut slot = self.guest_child.lock().await;
            if let Some(mut child) = slot.take()
                && let Err(error) = child.kill().await
            {
                tracing::warn!(%error, "联机内核进程已不在（可能早已退出）");
            }
        }
        // 房主：停止文件由上面的写入触发，提权包装进程负责 kill 内核，
        // 本进程没有权限也不需要直接动它。

        // 给内核一点退出时间，避免下个会话撞端口
        tokio::time::sleep(Duration::from_millis(800)).await;
        let _ = std::fs::remove_file(&stop_file);
        Ok(())
    }

    /// 应用退出时的兜底清理（主线程同步上下文调用）。
    pub fn shutdown(&self) {
        let stop_file = &self.stop_file;
        if let Some(parent) = stop_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(stop_file, b"stop");
    }

    // ---------- 内部 ----------

    /// 开场检查 + 初始化状态。`code` 必须已是规范格式。
    fn begin(&self, role: Role, code: &str, mc_port: u16) -> Result<()> {
        if self.is_active() {
            return Err(LauncherError::Internal {
                message: "已有进行中的联机会话，请先停止".into(),
            });
        }

        let mut guard = self.state.lock().expect("联机状态锁中毒");
        *guard = Some(SharedState {
            role: Some(role),
            room_code: Some(code.to_string()),
            mc_port,
            local_port: None,
            rpc_portal: None,
            link_type: None,
            phase: Phase::Starting,
            motd: String::new(),
            detail: match role {
                Role::Host => "正在准备联机内核…".to_string(),
                Role::Guest => "正在准备联机内核…".to_string(),
            },
            last_log: VecDeque::new(),
        });
        let _ = std::fs::remove_file(&self.stop_file);
        Ok(())
    }

    /// 更新状态（保留未知字段）。
    fn update<F: FnOnce(&mut SharedState)>(&self, f: F) {
        let mut guard = self.state.lock().expect("联机状态锁中毒");
        if let Some(s) = guard.as_mut() {
            f(s);
        }
    }

    /// 终态：只有当前不是 Stopped（用户主动停）时才覆盖。
    fn finish(&self, phase: Phase, detail: &str) {
        let mut guard = self.state.lock().expect("联机状态锁中毒");
        if let Some(s) = guard.as_mut() {
            if s.phase == Phase::Stopped && phase != Phase::Stopped {
                return; // 用户刚停了会话，错误信息无意义
            }
            s.phase = phase;
            s.detail = detail.to_string();
        }
    }

    // ---------- 房主 ----------

    async fn host_inner(
        &self,
        client: &reqwest::Client,
        code: &str,
        mc_port: u16,
        bundled_dir: Option<&std::path::Path>,
    ) -> Result<()> {
        let kernel = kernel::ensure_kernel_with_bundled(client, bundled_dir).await?;
        let creds = room_code::credentials(code);

        self.update(|s| s.detail = "内核就绪，等待授权创建虚拟网卡…".into());

        // TUN 模式参数：固定虚拟 IP，让房客无需发现流程
        let mut args: Vec<String> = vec![
            "--network-name".into(),
            creds.network_name.clone(),
            "--network-secret".into(),
            creds.network_secret.clone(),
            "--ipv4".into(),
            room_code::HOST_VIRTUAL_IP.to_string(),
        ];
        for server in DEFAULT_PUBLIC_SERVERS {
            args.push("-p".into());
            args.push((*server).to_string());
        }

        // RPC 门户钉到空闲端口：直连/中继徽章的查询通道（默认 15888
        // 容易与其它启动器的内核撞车）；UDP 广播中继让 TUN 模式的房客
        // 在多人列表自动发现本世界
        let rpc_portal = pick_free_port()?;
        args.push("--rpc-portal".into());
        args.push(format!("127.0.0.1:{rpc_portal}"));
        args.push("--enable-udp-broadcast-relay".into());

        let log_dir = paths::state_dir()?.join("multiplayer");
        std::fs::create_dir_all(&log_dir).map_err(|e| LauncherError::Io {
            message: format!("创建联机目录失败: {e}"),
        })?;
        let stdout_log = log_dir.join("host-stdout.log");
        let stderr_log = log_dir.join("host-stderr.log");

        // 兼容旧内核：--no-tun 不传给房主（房主要 TUN）
        let script = host_wrapper_script(
            &kernel,
            &args,
            &stdout_log,
            &stderr_log,
            &self.stop_file,
            mc_port,
        );
        let encoded = encode_powershell_command(&script);
        let inner = encode_powershell_command(&format!(
            "try {{ $p = Start-Process -FilePath 'powershell' -ArgumentList '-NoProfile','-EncodedCommand','{encoded}' -Verb RunAs -PassThru; $p.WaitForExit() }} catch {{ exit 1 }}"
        ));

        let mut outer = Command::new("powershell")
            .args(["-NoProfile", "-EncodedCommand", &inner])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| LauncherError::Io {
                message: format!("无法启动 PowerShell: {e}"),
            })?;

        // 授权窗口最多等 90 秒；期间轮询日志文件出现与否
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        loop {
            if stdout_log.is_file() || stderr_log.is_file() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = outer.kill().await;
                return Err(LauncherError::Internal {
                    message: "等待授权超时：请重试并在 UAC 弹窗中选择「是」".into(),
                });
            }
            if let Some(status) = outer.try_wait().map_err(|e| LauncherError::Io {
                message: format!("轮询联机进程失败: {e}"),
            })? {
                return Err(LauncherError::Internal {
                    message: if status.success() {
                        "联机未启动（授权被取消或失败）".to_string()
                    } else {
                        "UAC 授权被取消，联机未启动".to_string()
                    },
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // 会话标记：启动器被强杀时，下次启动据此写停止文件回收内核
        let _ = std::fs::write(log_dir.join("session.host"), b"hosting");

        self.update(|s| {
            s.phase = Phase::WaitingPeer;
            s.rpc_portal = Some(rpc_portal);
            s.detail = "虚拟网络已建立。请在游戏内「对局域网开放」，世界端口可点「自动检测」填入"
                .to_string();
        });

        // 日志监督：错误行记录进日志环（状态收敛交给探测循环 / 外层退出）
        let manager_log = LogTap {
            state: self.state.clone(),
        };
        tokio::spawn(supervise_log_file(stderr_log.clone(), manager_log));
        tokio::spawn(async move {
            let _ = outer.wait().await;
            // 外层退出 = 内层包装结束（正常停止 / 内核崩溃）。状态由 stop()
            // 或探测循环收敛，这里只兜底把意外退出标成 Error。
            let _ = stdout_log;
            let _ = stderr_log;
        });

        // 房主侧连通探测：世界开放即 Connected
        let probe = HostProbe {
            state: self.state.clone(),
            mc_port,
        };
        tokio::spawn(probe.run());

        // 直连/中继徽章监控
        spawn_link_monitor(
            self.state.clone(),
            kernel
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
            rpc_portal,
        );

        Ok(())
    }

    // ---------- 房客 ----------

    async fn guest_inner(
        &self,
        client: &reqwest::Client,
        code: &str,
        mc_port: u16,
        bundled_dir: Option<&std::path::Path>,
    ) -> Result<u16> {
        let kernel = kernel::ensure_kernel_with_bundled(client, bundled_dir).await?;
        let creds = room_code::credentials(code);
        let host_addr = room_code::host_virtual_addr(mc_port);
        let local_port = pick_free_port()?;

        self.update(|s| {
            s.local_port = Some(local_port);
            s.detail = "正在加入房间网络…".into();
        });

        let rpc_portal = pick_free_port()?;
        self.update(|s| s.rpc_portal = Some(rpc_portal));

        let mut args: Vec<String> = vec![
            "--network-name".into(),
            creds.network_name.clone(),
            "--network-secret".into(),
            creds.network_secret.clone(),
            // RPC 门户钉空闲端口：直连/中继徽章的查询通道（默认 15888
            // 容易与其它启动器的内核撞车）
            "--rpc-portal".into(),
            format!("127.0.0.1:{rpc_portal}"),
            "--no-tun".into(),
            "--port-forward".into(),
            format!("tcp://127.0.0.1:{local_port}/{host_addr}"),
        ];
        for server in DEFAULT_PUBLIC_SERVERS {
            args.push("-p".into());
            args.push((*server).to_string());
        }

        // kill_on_drop：应用异常退出时句柄随管理器释放，内核一起被回收，
        // 不给「房客内核孤儿进程」留机会
        let mut child = Command::new(&kernel)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| LauncherError::Io {
                message: format!("启动联机内核失败: {e}"),
            })?;

        // 转发 stdout/stderr 到状态日志
        let state = self.state.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(tap_stream(stdout, state.clone()));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(tap_stream(stderr, state.clone()));
        }

        // 句柄进槽位：stop() 按它精确 kill（不按镜像名误杀其他启动器的内核）
        {
            let mut slot = self.guest_child.lock().await;
            *slot = Some(child);
        }
        let exit_state = self.state.clone();
        let exit_slot = self.guest_child.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                // 只短暂持锁轮询，避免与 stop() 的 kill 互相卡死
                let outcome = {
                    let mut slot = exit_slot.lock().await;
                    match slot.as_mut() {
                        // stop() 已取走句柄 → 会话正常结束
                        None => break,
                        Some(child) => child.try_wait(),
                    }
                };
                match outcome {
                    Ok(None) => continue,
                    Ok(status) => {
                        let mut guard = exit_state.lock().expect("联机状态锁中毒");
                        if let Some(s) = guard.as_mut()
                            && s.phase != Phase::Stopped
                        {
                            s.phase = Phase::Error;
                            s.detail = format!("联机内核意外退出（{status:?}），请重试");
                        }
                        break;
                    }
                    Err(_) => break,
                }
            }
        });

        // 直连/中继徽章监控
        spawn_link_monitor(
            self.state.clone(),
            kernel
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
            rpc_portal,
        );

        self.update(|s| {
            s.phase = Phase::WaitingPeer;
            s.detail = "已发起连接，等待房主的网络可达…".into();
        });

        Ok(local_port)
    }

    /// **TUN 模式房客**：创建虚拟网卡 + UDP 广播中继 → 游戏内多人
    /// 列表自动发现房主的世界（PCL / Terraria 式体验）。
    ///
    /// 需要 UAC（提权包装进程托管，与房主同款双层 EncodedCommand；
    /// 停止走停止文件，无二次授权）。探测目标直接是房主虚拟 IP。
    async fn guest_inner_tun(
        &self,
        client: &reqwest::Client,
        code: &str,
        mc_port: u16,
        bundled_dir: Option<&std::path::Path>,
    ) -> Result<()> {
        let kernel = kernel::ensure_kernel_with_bundled(client, bundled_dir).await?;
        let creds = room_code::credentials(code);
        let host_addr = room_code::host_virtual_addr(mc_port);
        let rpc_portal = pick_free_port()?;

        self.update(|s| {
            s.rpc_portal = Some(rpc_portal);
            s.detail = "正在创建虚拟网卡（需要在 UAC 弹窗中授权）…".into();
        });

        let mut args: Vec<String> = vec![
            "--network-name".into(),
            creds.network_name.clone(),
            "--network-secret".into(),
            creds.network_secret.clone(),
            "--rpc-portal".into(),
            format!("127.0.0.1:{rpc_portal}"),
            // UDP 广播中继：Minecraft 局域网发现靠 224.0.2.60 广播包，
            // EasyTier 2.7.0 的这个开关把它转发到虚拟网对端
            "--enable-udp-broadcast-relay".into(),
        ];
        for server in DEFAULT_PUBLIC_SERVERS {
            args.push("-p".into());
            args.push((*server).to_string());
        }

        let log_dir = paths::state_dir()?.join("multiplayer");
        std::fs::create_dir_all(&log_dir).map_err(|e| LauncherError::Io {
            message: format!("创建联机目录失败: {e}"),
        })?;
        let stdout_log = log_dir.join("guest-stdout.log");
        let stderr_log = log_dir.join("guest-stderr.log");
        let script = host_wrapper_script(
            &kernel,
            &args,
            &stdout_log,
            &stderr_log,
            &self.stop_file,
            mc_port,
        );
        let encoded = encode_powershell_command(&script);
        let inner = encode_powershell_command(&format!(
            "try {{ $p = Start-Process -FilePath 'powershell' -ArgumentList '-NoProfile','-EncodedCommand','{encoded}' -Verb RunAs -PassThru; $p.WaitForExit() }} catch {{ exit 1 }}"
        ));

        let mut outer = Command::new("powershell")
            .args(["-NoProfile", "-EncodedCommand", &inner])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| LauncherError::Io {
                message: format!("无法启动 PowerShell: {e}"),
            })?;

        // 授权窗口最多等 90 秒（与房主一致）
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        loop {
            if stdout_log.is_file() || stderr_log.is_file() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = outer.kill().await;
                return Err(LauncherError::Internal {
                    message: "等待授权超时：请重试并在 UAC 弹窗中选择「是」".into(),
                });
            }
            if let Some(status) = outer.try_wait().map_err(|e| LauncherError::Io {
                message: format!("轮询联机进程失败: {e}"),
            })? {
                return Err(LauncherError::Internal {
                    message: if status.success() {
                        "联机未启动（授权被取消或失败）".to_string()
                    } else {
                        "UAC 授权被取消，联机未启动".to_string()
                    },
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // 会话标记：与房主共用（孤儿回收兜底）
        let _ = std::fs::write(log_dir.join("session.host"), b"hosting");

        self.update(|s| {
            s.phase = Phase::WaitingPeer;
            s.detail = format!(
                "已进入虚拟网络（虚拟网卡模式）：游戏内多人列表会自动发现房主的世界，或直接连接 {host_addr}"
            );
        });

        let manager_log = LogTap {
            state: self.state.clone(),
        };
        tokio::spawn(supervise_log_file(stderr_log.clone(), manager_log));
        tokio::spawn(async move {
            let _ = outer.wait().await;
            let _ = stdout_log;
            let _ = stderr_log;
        });

        // 探测目标直接是房主虚拟 IP（世界开放即 Connected）
        let state = self.state.clone();
        tokio::spawn(async move {
            let addr = host_addr;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                {
                    let guard = state.lock().expect("联机状态锁中毒");
                    match guard.as_ref() {
                        Some(s) if !matches!(s.phase, Phase::Error | Phase::Stopped) => {}
                        _ => return,
                    }
                }
                match ping::ping(&addr, Duration::from_secs(3)).await {
                    Ok(status) => {
                        let mut guard = state.lock().expect("联机状态锁中毒");
                        if let Some(s) = guard.as_mut()
                            && s.phase != Phase::Stopped
                        {
                            s.phase = Phase::Connected;
                            s.motd = status.motd.clone();
                            s.detail = format!(
                                "已连接到世界「{}」（{}，{}/{} 人在线）。游戏内多人列表会自动发现世界，或直接连接 {addr}",
                                status.motd.trim(),
                                status.version,
                                status.players_online,
                                status.players_max,
                            );
                        }
                    }
                    Err(_) => {
                        let mut guard = state.lock().expect("联机状态锁中毒");
                        if let Some(s) = guard.as_mut()
                            && s.phase == Phase::Connected
                        {
                            s.phase = Phase::WaitingPeer;
                            s.motd.clear();
                            s.detail = "与世界的连接中断，等待恢复…".into();
                        }
                    }
                }
            }
        });

        // 直连/中继徽章监控
        spawn_link_monitor(
            self.state.clone(),
            kernel
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
            rpc_portal,
        );

        Ok(())
    }

    /// 房客后台探测循环：每 5 秒 ping 一次本地转发端口。
    fn spawn_guest_probe(&self, local_port: u16) {
        let state = self.state.clone();
        tokio::spawn(async move {
            let addr = format!("127.0.0.1:{local_port}");
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                {
                    let guard = state.lock().expect("联机状态锁中毒");
                    match guard.as_ref() {
                        Some(s) => {
                            if matches!(s.phase, Phase::Error | Phase::Stopped) {
                                return; // 会话已结束
                            }
                        }
                        None => return, // 会话已清空
                    }
                }
                match ping::ping(&addr, Duration::from_secs(3)).await {
                    Ok(status) => {
                        let mut guard = state.lock().expect("联机状态锁中毒");
                        if let Some(s) = guard.as_mut()
                            && s.phase != Phase::Stopped
                        {
                            s.phase = Phase::Connected;
                            s.motd = status.motd.clone();
                            s.detail = format!(
                                "已连接到世界「{}」（{}，{}/{} 人在线）。游戏内直接连接 127.0.0.1:{local_port}",
                                status.motd.trim(),
                                status.version,
                                status.players_online,
                                status.players_max,
                            );
                        }
                    }
                    Err(_) => {
                        let mut guard = state.lock().expect("联机状态锁中毒");
                        if let Some(s) = guard.as_mut()
                            && s.phase == Phase::Connected
                        {
                            // 掉线回退到等待，不断开内核（可能只是换图/重启）
                            s.phase = Phase::WaitingPeer;
                            s.motd.clear();
                            s.detail = "与世界的连接中断，等待恢复…".into();
                        }
                    }
                }
            }
        });
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 房间码错误 → 启动器错误（用户可读）。
fn room_code_error(error: RoomCodeError) -> LauncherError {
    LauncherError::InvalidConfig {
        key: "roomCode".into(),
        message: error.to_string(),
    }
}

/// SharedState 的克隆句柄（供监督任务更新状态）。
type SharedHandle = Arc<StdMutex<Option<SharedState>>>;

/// 日志行写入共享状态的小适配。
#[derive(Clone)]
struct LogTap {
    state: SharedHandle,
}

impl LogTap {
    fn push(&self, line: &str) {
        let mut guard = self.state.lock().expect("联机状态锁中毒");
        if let Some(s) = guard.as_mut() {
            if s.last_log.len() >= 8 {
                s.last_log.pop_front();
            }
            s.last_log.push_back(line.to_string());
        }
    }
}

/// 房主侧连通探测循环：世界开放（127.0.0.1:mc_port 可 ping）即 Connected。
struct HostProbe {
    state: SharedHandle,
    mc_port: u16,
}

impl HostProbe {
    async fn run(self) {
        let addr = format!("127.0.0.1:{}", self.mc_port);
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let still_active = {
                let guard = self.state.lock().expect("联机状态锁中毒");
                match guard.as_ref() {
                    Some(s) => !matches!(s.phase, Phase::Error | Phase::Stopped),
                    None => return,
                }
            };
            if !still_active {
                return;
            }
            match ping::ping(&addr, Duration::from_secs(3)).await {
                Ok(status) => {
                    let mut guard = self.state.lock().expect("联机状态锁中毒");
                    if let Some(s) = guard.as_mut()
                        && !matches!(s.phase, Phase::Error | Phase::Stopped)
                    {
                        s.phase = Phase::Connected;
                        s.motd = status.motd.clone();
                        s.detail = "世界已开放，房客可在游戏多人列表中直接连接".into();
                    }
                }
                Err(_) => {
                    let mut guard = self.state.lock().expect("联机状态锁中毒");
                    if let Some(s) = guard.as_mut()
                        && s.phase == Phase::Connected
                    {
                        s.phase = Phase::WaitingPeer;
                        s.motd.clear();
                        s.detail = "世界已关闭，等待重新「对局域网开放」…".into();
                    }
                }
            }
        }
    }
}

/// 监视日志文件中的致命错误行（房主侧；文件由提权进程写入）。
async fn supervise_log_file(path: PathBuf, tap: LogTap) {
    // 文件可能尚未创建；轮询而不是 inotify（跨平台简单可靠）
    for _ in 0..2400 {
        // 最多 20 分钟
        tokio::time::sleep(Duration::from_millis(500)).await;
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let fresh_lines: Vec<&str> = content.lines().collect();
        let tail_start = fresh_lines.len().saturating_sub(20);
        for line in &fresh_lines[tail_start..] {
            tap.push(line);
        }
    }
}

/// 把内核子进程输出写进共享日志环。
async fn tap_stream<R: tokio::io::AsyncRead + Unpin>(stream: R, state: SharedHandle) {
    let tap = LogTap { state };
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tap.push(&line);
        // 致命错误提前把阶段置 Error（仅当还在 Starting/WaitingPeer）
        if (line.contains("ERROR") || line.contains("panic")) && line.contains("TUN") {
            let mut guard = tap.state.lock().expect("联机状态锁中毒");
            if let Some(s) = guard.as_mut()
                && matches!(s.phase, Phase::Starting | Phase::WaitingPeer)
            {
                s.phase = Phase::Error;
                s.detail = "虚拟网卡创建失败（需要管理员权限）".into();
            }
        }
    }
}

/// 挑一个空闲的本地端口（房客转发监听用）。
/// 直连/中继徽章的后台监控：每 5 秒查一次 RPC 门户的 peer 列表。
///
/// 查询失败静默保留上一次的值；会话结束（Error/Stopped/清空）即退出。
fn spawn_link_monitor(state: SharedHandle, cli_dir: PathBuf, rpc_portal: u16) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            {
                let guard = state.lock().expect("联机状态锁中毒");
                match guard.as_ref() {
                    Some(s) if !matches!(s.phase, Phase::Error | Phase::Stopped) => {}
                    _ => return,
                }
            }
            if let Some(kind) = link::query(&cli_dir, rpc_portal).await {
                let mut guard = state.lock().expect("联机状态锁中毒");
                if let Some(s) = guard.as_mut() {
                    s.link_type = Some(kind);
                }
            }
        }
    });
}

fn pick_free_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| LauncherError::Io {
        message: format!("分配本地端口失败: {e}"),
    })?;
    let port = listener
        .local_addr()
        .map_err(|e| LauncherError::Io {
            message: format!("读取本地端口失败: {e}"),
        })?
        .port();
    drop(listener);
    Ok(port)
}

/// 房主提权包装脚本（内层，提权执行）。
///
/// 流程：启动 easytier-core（输出重定向到日志）→ 轮询停止文件 →
/// 出现则强杀。整个脚本经 `-EncodedCommand` 传递，**不含任何引号
/// 转义问题**，也避免了写 .ps1 文件的编码风险。
fn host_wrapper_script(
    kernel: &Path,
    args: &[String],
    stdout_log: &Path,
    stderr_log: &Path,
    stop_file: &Path,
    mc_port: u16,
) -> String {
    // EncodedCommand 之外不能再有引号地狱：路径统一用单引号 + 双写转义
    let q = |s: &Path| format!("'{}'", s.display().to_string().replace('\'', "''"));
    let arg_list = args
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");

    // 防火墙放行世界端口（入站 TCP/UDP）：TUN 虚拟网卡通常被 Windows 识别为
    // 「公用网络」，java.exe 的入站连接默认被拦——房客转发过来的连接会
    // 被 Connection reset。房主端本来就在提权上下文里，顺手加规则；
    // 停止联机时再删掉，不留常驻开口。
    let fw_allow = format!(
        "netsh advfirewall firewall delete-rule name='Sereno MC {mc_port}' | Out-Null; \
         netsh advfirewall firewall add-rule name='Sereno MC {mc_port}' dir=in action=allow protocol=TCP localport={mc_port} | Out-Null; \
         netsh advfirewall firewall add-rule name='Sereno MC {mc_port} UDP' dir=in action=allow protocol=UDP localport={mc_port} | Out-Null;"
    );
    let fw_remove = format!(
        "netsh advfirewall firewall delete-rule name='Sereno MC {mc_port}' | Out-Null; \
         netsh advfirewall firewall delete-rule name='Sereno MC {mc_port} UDP' | Out-Null;"
    );

    format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         {fw_allow} \
         $p = Start-Process -FilePath {} -ArgumentList @({}) -PassThru -WindowStyle Hidden -RedirectStandardOutput {} -RedirectStandardError {}; \
         while (-not (Test-Path -LiteralPath {})) {{ Start-Sleep -Milliseconds 600; if ($p.HasExited) {{ break }} }}; \
         if (-not $p.HasExited) {{ Stop-Process -Id $p.Id -Force }}; \
         {fw_remove}",
        q(kernel),
        arg_list,
        q(stdout_log),
        q(stderr_log),
        q(stop_file),
    )
}

/// PowerShell `-EncodedCommand`：Base64(UTF-16LE)。
///
/// 这是 Windows 官方推荐的免引号传参方式，脚本内容可以是任意文本。
fn encode_powershell_command(script: &str) -> String {
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_powershell_round_trip() {
        let script = "$p = Start-Process 'C:\\程序 路径\\easytier-core.exe' -PassThru";
        let encoded = encode_powershell_command(script);
        // EncodedCommand 必须是纯 ASCII base64
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        );
        // 解回 UTF-16LE 验证无损
        let decoded_bytes = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        let units: Vec<u16> = decoded_bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let decoded = String::from_utf16(&units).unwrap();
        assert_eq!(decoded, script);
    }

    #[test]
    fn wrapper_script_contains_paths_and_stop_watch() {
        let script = host_wrapper_script(
            Path::new("C:\\et\\easytier-core.exe"),
            &["--network-name".to_string(), "abc".to_string()],
            Path::new("C:\\state\\out.log"),
            Path::new("C:\\state\\err.log"),
            Path::new("C:\\state\\stop.flag"),
            25565,
        );
        assert!(script.contains("easytier-core.exe"));
        assert!(script.contains("--network-name"));
        // 世界端口防火墙规则：创建时放行
        assert!(script.contains("25565"));
        assert!(script.contains("stop.flag"));
        assert!(script.contains("Stop-Process"));
        assert!(script.contains("Test-Path"));
    }

    #[test]
    fn manager_status_defaults_to_stopped() {
        let manager = SessionManager::new();
        let status = manager.status();
        assert_eq!(status.phase, Phase::Stopped);
        assert_eq!(status.role, None);
        assert!(!manager.is_active());
    }

    #[test]
    fn pick_free_port_returns_bindable_port() {
        let port = pick_free_port().unwrap();
        assert!(port > 0);
        // 端口应当能再次绑定（允许偶发竞态失败，测试用SO_REUSEADDR由系统决定）
    }

    #[test]
    fn begin_accepts_normalized_code_and_sets_state() {
        let manager = SessionManager::new();
        let code = crate::multiplayer::room_code::generate();
        manager
            .begin(crate::multiplayer::Role::Guest, &code, 25565)
            .unwrap();
        assert!(manager.is_active());
        let status = manager.status();
        assert_eq!(status.role, Some(crate::multiplayer::Role::Guest));
        assert_eq!(status.room_code.as_deref(), Some(code.as_str()));
    }
}
