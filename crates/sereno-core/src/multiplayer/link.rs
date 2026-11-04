//! 链路类型探测（直连 / 中继徽章，路线图 #7）。
//!
//! 通过随包分发的 `easytier-cli` 查询本机内核的 RPC 门户，拿 peer 列表：
//! 每个对端的 `cost` 字段标明链路类型（`Direct` / `Relay`）。解析用
//! `-o json` 输出——表格格式会随版本变列宽，JSON 是稳定契约
//! （内核版本被我们钉死在 2.7.0，双保险）。

use std::path::Path;

use serde::Deserialize;

/// 虚拟网络内到对端的链路类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// P2P 直连：延迟最低。
    Direct,
    /// 经公共中继转发：NAT 打洞失败时的稳定兜底。
    Relay,
}

impl LinkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Relay => "relay",
        }
    }
}

#[derive(Debug, Deserialize)]
struct PeerJson {
    #[serde(default)]
    cost: String,
}

fn cli_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "easytier-cli.exe"
    } else {
        "easytier-cli"
    }
}

fn build_command(cli: &Path, rpc_portal: u16) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(cli);
    command
        .args([
            "-p",
            &format!("127.0.0.1:{rpc_portal}"),
            "-o",
            "json",
            "peer",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x0800_0000);
    command
}

/// 查询到其它对端的链路类型。
///
/// 返回 `None` 表示本次查询失败（CLI 不在、RPC 未就绪、超时）——
/// 徽章保持上一次的值，不打断会话。
pub async fn query(cli_dir: &Path, rpc_portal: u16) -> Option<LinkKind> {
    let cli = cli_dir.join(cli_binary_name());
    if !cli.is_file() {
        return None;
    }

    let child = build_command(&cli, rpc_portal).spawn().ok()?;

    let output =
        match tokio::time::timeout(std::time::Duration::from_secs(4), child.wait_with_output())
            .await
        {
            // 超时（Elapsed）或子进程 IO 错误 → 本次查询失败
            Ok(Ok(output)) => output,
            _ => return None,
        };

    let peers: Vec<PeerJson> = serde_json::from_slice(&output.stdout).ok()?;
    let mut kind = None;
    for peer in &peers {
        let cost = peer.cost.to_lowercase();
        if cost.contains("direct") {
            // 直连优先：多个对端时取最优
            return Some(LinkKind::Direct);
        }
        if cost.contains("relay") && kind.is_none() {
            kind = Some(LinkKind::Relay);
        }
    }
    kind
}
