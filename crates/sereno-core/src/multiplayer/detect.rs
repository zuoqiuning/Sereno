//! 游戏世界端口自动探测。
//!
//! 痛点：单人世界「对局域网开放」的端口是**随机的**（如 54321），
//! 要求房主去聊天栏找端口再切回启动器手填，体验断裂。
//!
//! 方案（不依赖组播中继的短期解法）：
//! 1. 枚举本机**Java 进程**的监听 TCP 端口（`tasklist` + `netstat`）；
//! 2. 对每个候选端口做一次 MC Server List Ping（复用 `ping.rs`）；
//! 3. 能回答状态协议的那个端口就是游戏世界。
//!
//! 两层过滤保证**零误报**且不骚扰无关服务：
//! 进程名过滤（只看 java/javaw）+ 协议判定（不是 MC 就静默失败）。
//! 只连 127.0.0.1，不产生任何外联流量。

use std::collections::HashSet;

use crate::Result;

use super::ping;

/// 探测结果：端口 + 世界信息（给 UI 确认用）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DetectedWorld {
    /// 世界监听端口。
    pub port: u16,
    /// MOTD（通常是世界名）。
    pub motd: String,
    /// 游戏版本。
    pub version: String,
}

/// 探测当前机器上开放的游戏世界。
///
/// 返回 `None` 表示没有找到（游戏没开 / 还没「对局域网开放」），
/// 调用方保持手动输入兜底。整个探测约 1–2 秒。
pub async fn detect_world() -> Result<Option<DetectedWorld>> {
    let candidates = candidate_ports()?;
    if candidates.is_empty() {
        return Ok(None);
    }

    for port in candidates {
        let addr = format!("127.0.0.1:{port}");
        // 单端口探测超时收得很短：候选通常只有一两个
        if let Ok(status) = ping::ping(&addr, std::time::Duration::from_millis(900)).await {
            return Ok(Some(DetectedWorld {
                port,
                motd: status.motd,
                version: status.version,
            }));
        }
    }
    Ok(None)
}

/// 枚举 Java 进程监听的 TCP 端口。
fn candidate_ports() -> Result<Vec<u16>> {
    #[cfg(target_os = "windows")]
    {
        windows_candidates()
    }
    #[cfg(target_os = "linux")]
    {
        linux_candidates()
    }
    #[cfg(target_os = "macos")]
    {
        macos_candidates()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Ok(Vec::new())
    }
}

/// 判断进程名是否是 Java 运行时（游戏 / 服务端的常见形态）。
fn is_java_image(image: &str) -> bool {
    let lower = image.to_ascii_lowercase();
    lower == "java.exe"
        || lower == "javaw.exe"
        || lower == "java"
        || lower.starts_with("java-") // 如 java-17-openjdk
        || lower.contains("javaw")
}

/// 执行外部命令并返回 stdout（失败返回空，调用方按「无候选」处理）。
fn command_output(program: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(program).args(args).output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        _ => String::new(),
    }
}

#[cfg(target_os = "windows")]
fn windows_candidates() -> Result<Vec<u16>> {
    // 1. PID → 进程名，筛出 Java
    let tasklist = command_output("tasklist", &["/FO", "CSV", "/NH"]);
    let mut java_pids: HashSet<String> = HashSet::new();
    for line in tasklist.lines() {
        // CSV: "名称","PID","会话名",...
        let mut fields = line.split("\",\"");
        let Some(name) = fields.next() else { continue };
        let Some(pid) = fields.next() else { continue };
        let name = name.trim_start_matches('"');
        if is_java_image(name) {
            java_pids.insert(pid.trim().trim_end_matches('"').to_string());
        }
    }
    if java_pids.is_empty() {
        return Ok(Vec::new());
    }

    // 2. netstat LISTENING 行 → 属于 Java 的端口
    let netstat = command_output("netstat", &["-ano", "-p", "TCP"]);
    let mut ports: Vec<u16> = Vec::new();
    for line in netstat.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // 形如：TCP  0.0.0.0:54321  0.0.0.0:0  LISTENING  12345
        if fields.len() < 5 || fields[3] != "LISTENING" {
            continue;
        }
        if !java_pids.contains(fields[4]) {
            continue;
        }
        if let Some(port) = fields[1].rsplit(':').next().and_then(|p| p.parse().ok()) {
            ports.push(port);
        }
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

#[cfg(target_os = "linux")]
fn linux_candidates() -> Result<Vec<u16>> {
    // ss -tlnp：State Recv-Q Send-Q Local:Port Peer:Port Process
    let ss = command_output("ss", &["-tlnp"]);
    let mut ports: Vec<u16> = Vec::new();
    for line in ss.lines().skip(1) {
        if !line.contains("java") {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        if let Some(port) = fields[3].rsplit(':').next().and_then(|p| p.parse().ok()) {
            ports.push(port);
        }
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

#[cfg(target_os = "macos")]
fn macos_candidates() -> Result<Vec<u16>> {
    // lsof -nP -iTCP -sTCP:LISTEN：java 1234 user 44u IPv4 ... TCP *:54321 (LISTEN)
    let lsof = command_output("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN"]);
    let mut ports: Vec<u16> = Vec::new();
    for line in lsof.lines().skip(1) {
        if !line.contains("java") {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(addr_field) = fields.iter().rev().find(|f| f.contains(':')) else {
            continue;
        };
        if let Some(port) = addr_field.rsplit(':').next().and_then(|p| p.parse().ok()) {
            ports.push(port);
        }
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_image_detection() {
        assert!(is_java_image("java.exe"));
        assert!(is_java_image("javaw.exe"));
        assert!(is_java_image("java"));
        assert!(is_java_image("java-17-openjdk"));
        assert!(!is_java_image("javasc*ipt"));
        assert!(!is_java_image("explorer.exe"));
        assert!(!is_java_image("notepad.exe"));
    }

    #[test]
    fn command_output_handles_missing_binary() {
        // 不存在的命令 → 空串而不是 panic
        let out = command_output("definitely-not-a-real-binary-xyz", &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn detect_returns_ok_even_without_game() {
        // 无游戏运行时应该优雅返回 None（而不是错误）
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(detect_world());
        assert!(result.is_ok());
    }
}
