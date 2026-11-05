//! Minecraft 服务器状态探测（Server List Ping）。
//!
//! 联机模块用它的回答作为「真正连上了」的判据——比解析 EasyTier 日志
//! 可靠得多：能 ping 通 = 端口转发 + 虚拟网 + 游戏世界三者全部就绪。
//!
//! 实现的是**无状态化的 Server List Ping**：
//! 握手包（state=1）→ 请求包 → 读 varint 前缀 JSON。
//! 不需要登录，不触发反作弊，对服务端零负担（与玩家在服务器列表
//! 里加一条地址等价）。
//!
//! 协议参考：<https://wiki.vg/Protocol>（Status / Handshaking）。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::{LauncherError, Result};

/// 探测结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    /// MOTD（描述文本，可能带 § 颜色码，前端按原样展示即可）。
    pub motd: String,
    /// 服务端版本字符串（如 `1.21.1`）。
    pub version: String,
    /// 当前在线玩家数。
    pub players_online: u32,
    /// 最大玩家数。
    pub players_max: u32,
}

/// 对 `addr` 做一次状态探测，`timeout` 内无响应视为失败。
///
/// 任何失败（拒绝连接、超时、协议不符）都返回 `Err`，
/// 调用方把它当「世界还没就绪」处理即可，不需要区分原因。
pub async fn ping(addr: &str, timeout: Duration) -> Result<ServerStatus> {
    match tokio::time::timeout(timeout, ping_inner(addr)).await {
        Ok(inner) => inner,
        Err(_) => Err(LauncherError::Internal {
            message: format!("探测超时: {addr}"),
        }),
    }
}

async fn ping_inner(addr: &str) -> Result<ServerStatus> {
    let mut stream = TcpStream::connect(addr)
        .await
        .map_err(|e| LauncherError::Internal {
            message: format!("连接失败: {e}"),
        })?;

    let (host, port) = split_addr(addr);

    // ---- 握手包：protocol_version=-1（状态查询），next_state=1 ----
    let mut handshake = Vec::with_capacity(32);
    write_varint(&mut handshake, -1i32 as u32); // protocol version: unknown
    write_varint(&mut handshake, host.len() as u32);
    handshake.extend_from_slice(host.as_bytes());
    handshake.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake, 1); // next state: status

    write_packet(&mut stream, &handshake).await?;

    // ---- 状态请求：空包体 ----
    write_packet(&mut stream, &[]).await?;

    // ---- 读响应 ----
    let response = read_packet(&mut stream).await?;
    let mut cursor = 0usize;
    let json_len = read_varint(&response, &mut cursor)? as usize;
    if json_len == 0 || cursor + json_len > response.len() {
        return Err(LauncherError::Internal {
            message: "状态响应长度异常".into(),
        });
    }
    let json: serde_json::Value = serde_json::from_slice(&response[cursor..cursor + json_len])
        .map_err(|e| LauncherError::Internal {
            message: format!("状态响应解析失败: {e}"),
        })?;

    Ok(ServerStatus {
        motd: json
            .get("description")
            .and_then(description_text)
            .unwrap_or_default(),
        version: json
            .pointer("/version/name")
            .and_then(|v| v.as_str())
            .unwrap_or("未知版本")
            .to_string(),
        players_online: json
            .pointer("/players/online")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(u32::MAX as u64) as u32)
            .unwrap_or(0),
        players_max: json
            .pointer("/players/max")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(u32::MAX as u64) as u32)
            .unwrap_or(0),
    })
}

/// `description` 可能是字符串，也可能是 `{"text": ...}` 组件树；取纯文本。
fn description_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(obj) => {
            obj.get("text").and_then(|t| t.as_str()).map(String::from)
        }
        _ => None,
    }
}

fn split_addr(addr: &str) -> (String, u16) {
    match addr.rsplit_once(':') {
        Some((h, p)) => (
            h.trim_matches(['[', ']']).to_string(),
            p.parse().unwrap_or(25565),
        ),
        None => (addr.to_string(), 25565),
    }
}

/// 写一个完整包：varint 长度前缀 + 包体。
async fn write_packet(stream: &mut TcpStream, body: &[u8]) -> Result<()> {
    let mut framed = Vec::with_capacity(body.len() + 5);
    write_varint(&mut framed, body.len() as u32);
    framed.extend_from_slice(body);
    stream
        .write_all(&framed)
        .await
        .map_err(|e| LauncherError::Internal {
            message: format!("发送失败: {e}"),
        })?;
    stream.flush().await.map_err(|e| LauncherError::Internal {
        message: format!("刷新失败: {e}"),
    })
}

/// 读一个完整包（长度上限 2 MiB，防异常服务器撑爆内存）。
async fn read_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    const MAX_PACKET: usize = 2 * 1024 * 1024;
    let mut length_buf = [0u8; 1];
    let mut length = 0u32;
    let mut shift = 0u32;
    loop {
        stream
            .read_exact(&mut length_buf)
            .await
            .map_err(|e| LauncherError::Internal {
                message: format!("读取包长失败: {e}"),
            })?;
        length |= u32::from(length_buf[0] & 0x7F) << shift;
        if length_buf[0] & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 21 {
            return Err(LauncherError::Internal {
                message: "包长 varint 异常".into(),
            });
        }
    }
    if length as usize > MAX_PACKET {
        return Err(LauncherError::Internal {
            message: "响应包过大".into(),
        });
    }
    let mut body = vec![0u8; length as usize];
    stream
        .read_exact(&mut body)
        .await
        .map_err(|e| LauncherError::Internal {
            message: format!("读取包体失败: {e}"),
        })?;
    Ok(body)
}

fn write_varint(buf: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn read_varint(buf: &[u8], cursor: &mut usize) -> Result<u32> {
    let mut value = 0u32;
    let mut shift = 0u32;
    loop {
        let Some(&byte) = buf.get(*cursor) else {
            return Err(LauncherError::Internal {
                message: "varint 越界".into(),
            });
        };
        *cursor += 1;
        value |= u32::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift > 28 {
            return Err(LauncherError::Internal {
                message: "varint 过长".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_round_trip() {
        for value in [0u32, 1, 127, 128, 255, 2097151, 2097152, u32::MAX / 2] {
            let mut buf = Vec::new();
            write_varint(&mut buf, value);
            let mut cursor = 0usize;
            assert_eq!(read_varint(&buf, &mut cursor).unwrap(), value);
            assert_eq!(cursor, buf.len());
        }
    }

    #[test]
    fn splits_addr_with_ipv6_and_default_port() {
        assert_eq!(split_addr("127.0.0.1:25565"), ("127.0.0.1".into(), 25565));
        assert_eq!(split_addr("[::1]:25566"), ("::1".into(), 25566));
        assert_eq!(split_addr("example.com"), ("example.com".into(), 25565));
    }

    #[test]
    fn description_extracts_text_component() {
        assert_eq!(
            description_text(&serde_json::json!("hello")),
            Some("hello".into())
        );
        assert_eq!(
            description_text(&serde_json::json!({"text": "world"})),
            Some("world".into())
        );
        assert_eq!(description_text(&serde_json::json!(42)), None);
    }
}
