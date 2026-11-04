//! 远程 ZIP 的**按需读取**。
//!
//! ## 为什么需要
//!
//! CurseForge 的整合包在 CF 上是一个几十 MB 的 zip（RLCraft 有 51 MB），
//! 但真正需要的信息只有两样：
//!
//! - 根目录的 `manifest.json`（几 KB）：决定游戏版本、加载器与文件清单；
//! - `overrides/` 目录：配置与脚本。
//!
//! 若为了读一个 `manifest.json` 就把整个包下下来，
//! 用户点「安装整合包」前要先白等一次几十 MB 的下载——
//! 而**下载之前根本还不知道装不装得起来**（版本不匹配、文件已下架）。
//!
//! ## 做法
//!
//! 用 HTTP Range 请求只取需要的字节，**在内存里重建一个只含目标条目的
//! 单条目 ZIP**，再交给 `zip` crate 解压：
//!
//! 1. `Range: bytes=0-0` 探测总长度（顺带验证服务端支持 Range）；
//! 2. 读文件尾部找到 EOCD（中央目录结束记录），拿到中央目录的位置与大小；
//! 3. 读中央目录，定位目标条目，拿到它的本地头偏移与压缩后大小；
//! 4. 读本地头 + 压缩数据；
//! 5. 把「本地头 + 数据 + 改写过偏移的中央目录项 + EOCD」拼成一个内存 ZIP，
//!    交给 `zip::ZipArchive` 解压。
//!
//! 全程只取几十 KB，**不阻塞运行时的异步 IO**。

use crate::{LauncherError, Result};

/// 中央目录结束记录（EOCD）的签名。
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
/// 中央目录文件头（Central Directory Header）的签名。
const CD_SIGNATURE: u32 = 0x0201_4b50;
/// 本地文件头（Local File Header）的签名。
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;

/// EOCD 固定长度。
const EOCD_LEN: usize = 22;
/// 中央目录项固定长度（不含文件名与扩展字段）。
const CD_FIXED_LEN: usize = 46;
/// 本地文件头固定长度（不含文件名与扩展字段）。
const LOCAL_FIXED_LEN: usize = 30;

/// 读文件尾部时的最大长度。
///
/// ZIP 的 EOCD 位于文件末尾，但后面可能跟着最多 64 KB 的注释，
/// 因此尾部读取量取 64 KB + EOCD 长度足够覆盖。
const TAIL_WINDOW: u64 = 64 * 1024 + EOCD_LEN as u64;

/// 单个条目解压后的上限（512 MB）。
///
/// `overrides/` 里偶尔有上百 MB 的世界存档，但也不该让一个远程条目
/// 把内存吃光——超了直接拒绝，让用户改用完整下载。
const MAX_ENTRY_SIZE: u64 = 512 * 1024 * 1024;

/// 从远程 ZIP 中读取指定条目（按名字，路径用 `/`）。
///
/// 条目名要与 ZIP 内部记录一致：根目录文件写 `"manifest.json"`，
/// 子目录写 `"overrides/config/a.toml"`。
pub async fn read_entry(client: &reqwest::Client, url: &str, entry: &str) -> Result<Vec<u8>> {
    read_entries(client, url, &[entry.to_string()])
        .await?
        .into_iter()
        .next()
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| LauncherError::Parse {
            message: format!("远程 ZIP 中没有条目 {entry}"),
        })
}

/// 批量读取远程 ZIP 中的若干条目。
///
/// 返回 `(条目名, 内容)`，顺序与请求一致；某个条目不存在时该位置不出现。
///
/// 批量版本比逐个调用更省事：中央目录只读一次，后续条目各自取自己的字节。
pub async fn read_entries(
    client: &reqwest::Client,
    url: &str,
    entries: &[String],
) -> Result<Vec<(String, Vec<u8>)>> {
    let total = probe_length(client, url).await?;
    let eocd = read_eocd(client, url, total).await?;
    let cd = read_central_directory(client, url, eocd).await?;

    let mut result = Vec::new();
    for entry in entries {
        let Some(target) = find_entry(&cd, entry) else {
            continue;
        };

        if target.uncompressed_size > MAX_ENTRY_SIZE {
            return Err(LauncherError::Parse {
                message: format!(
                    "远程 ZIP 条目过大（{} 字节），已拒绝",
                    target.uncompressed_size
                ),
            });
        }

        // 本地头：固定 30 字节 + 文件名 + 扩展字段。
        // 先取固定部分读出名字与扩展字段的长度，再取完整头部。
        let local_fixed =
            fetch_range(client, url, target.local_offset, LOCAL_FIXED_LEN as u64).await?;
        if local_fixed.len() < LOCAL_FIXED_LEN {
            return Err(LauncherError::Parse {
                message: "远程 ZIP 本地文件头不完整".into(),
            });
        }
        if read_u32(&local_fixed, 0) != LOCAL_SIGNATURE {
            return Err(LauncherError::Parse {
                message: "远程 ZIP 本地文件头签名不正确".into(),
            });
        }

        let name_len = read_u16(&local_fixed, 26) as u64;
        let extra_len = read_u16(&local_fixed, 28) as u64;
        let header_len = LOCAL_FIXED_LEN as u64 + name_len + extra_len;

        let local = fetch_range(client, url, target.local_offset, header_len).await?;
        let data_offset = target.local_offset + header_len;

        // 压缩数据
        let data = fetch_range(client, url, data_offset, target.compressed_size).await?;

        // 重建单条目 ZIP：本地头 + 数据 + 中央目录项 + EOCD
        let rebuilt = rebuild_single_entry(&local, &data, &cd, &target)?;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(rebuilt)).map_err(|e| {
            LauncherError::Parse {
                message: format!("打开远程 ZIP 条目失败 {entry}: {e}"),
            }
        })?;

        let mut file = archive.by_name(entry).map_err(|e| LauncherError::Parse {
            message: format!("读取远程 ZIP 条目 {entry} 失败: {e}"),
        })?;
        let mut buffer = Vec::with_capacity(target.uncompressed_size as usize);
        std::io::Read::read_to_end(&mut file, &mut buffer).map_err(|e| LauncherError::Parse {
            message: format!("解压远程 ZIP 条目 {entry} 失败: {e}"),
        })?;

        result.push((entry.clone(), buffer));
    }

    Ok(result)
}

/// 列出远程 ZIP 中的全部条目名（不含目录）。
///
/// 用于「按需取出某个前缀下的所有文件」——例如整合包的 `overrides/`。
pub async fn list_entries(client: &reqwest::Client, url: &str) -> Result<Vec<String>> {
    let total = probe_length(client, url).await?;
    let eocd = read_eocd(client, url, total).await?;
    let cd = read_central_directory(client, url, eocd).await?;

    let mut names = Vec::new();
    let mut offset = 0usize;
    while offset + CD_FIXED_LEN <= cd.len() {
        if read_u32(&cd, offset) != CD_SIGNATURE {
            break;
        }
        let name_len = read_u16(&cd, offset + 28) as usize;
        let extra_len = read_u16(&cd, offset + 30) as usize;
        let comment_len = read_u16(&cd, offset + 32) as usize;
        let name =
            String::from_utf8_lossy(&cd[offset + CD_FIXED_LEN..offset + CD_FIXED_LEN + name_len])
                .to_string();
        if !name.ends_with('/') {
            names.push(name);
        }
        offset += CD_FIXED_LEN + name_len + extra_len + comment_len;
    }

    Ok(names)
}

/// 探测远程文件总长度，同时确认服务端支持 Range。
async fn probe_length(client: &reqwest::Client, url: &str) -> Result<u64> {
    let response = client
        .get(url)
        .header("Range", "bytes=0-0")
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("请求远程 ZIP 失败: {e}"),
            url: url.to_string(),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(LauncherError::DownloadFailed {
            message: format!("远程 ZIP 返回 HTTP {status}"),
            url: url.to_string(),
        });
    }

    // 服务端可能直接返回 200（忽略了 Range），此时只有 Content-Length，
    // 无法按需读取——但下面的尾部与目录请求仍会退化成整文件下载，
    // 因此这里明确报错而不是悄悄变成「下载整个包」。
    if let Some(range) = response.headers().get("Content-Range")
        && let Some(total) = parse_range_total(range.to_str().unwrap_or(""))
    {
        return Ok(total);
    }

    Err(LauncherError::DownloadFailed {
        message: "远程 ZIP 不支持 Range 请求，无法按需读取".into(),
        url: url.to_string(),
    })
}

/// 解析 `Content-Range: bytes 0-0/12345` 里的总长度。
fn parse_range_total(value: &str) -> Option<u64> {
    let total = value.rsplit('/').next()?.trim();
    if total == "*" {
        return None;
    }
    total.parse().ok()
}

/// 读取文件尾部，定位 EOCD。
async fn read_eocd(client: &reqwest::Client, url: &str, total: u64) -> Result<Eocd> {
    let tail_len = TAIL_WINDOW.min(total);
    let tail = fetch_range(client, url, total - tail_len, tail_len).await?;

    // 从后往前找 EOCD 签名。
    // 注释里也可能出现同样的字节序列，因此要校验后续字段是否自洽。
    let mut position = tail.len().checked_sub(EOCD_LEN);
    while let Some(index) = position {
        if read_u32(&tail, index) == EOCD_SIGNATURE {
            let comment_len = read_u16(&tail, index + 20) as u64;
            // EOCD 之后应当正好是注释结束
            if index as u64 + EOCD_LEN as u64 + comment_len == tail.len() as u64 {
                let cd_size = read_u32(&tail, index + 12) as u64;
                let cd_offset = read_u32(&tail, index + 16) as u64;

                // ZIP64：偏移被写成 0xFFFFFFFF 时需要额外的定位记录。
                // 整合包不会有 4 GB 以上的中央目录，遇到就直接拒绝。
                if cd_offset == u32::MAX as u64 || cd_size == u32::MAX as u64 {
                    return Err(LauncherError::Parse {
                        message: "远程 ZIP 使用了 ZIP64 扩展，无法按需读取".into(),
                    });
                }

                return Ok(Eocd { cd_offset, cd_size });
            }
        }
        position = index.checked_sub(1);
    }

    Err(LauncherError::Parse {
        message: "远程 ZIP 的中央目录结束记录损坏".into(),
    })
}

/// EOCD 里解析出的中央目录信息。
#[derive(Debug, Clone, Copy)]
struct Eocd {
    cd_offset: u64,
    cd_size: u64,
}

/// 读中央目录区段。
async fn read_central_directory(
    client: &reqwest::Client,
    url: &str,
    eocd: Eocd,
) -> Result<Vec<u8>> {
    fetch_range(client, url, eocd.cd_offset, eocd.cd_size).await
}

/// 定位到的中央目录条目。
#[derive(Debug, Clone)]
struct CdEntry {
    /// 本地文件头偏移。
    local_offset: u64,
    /// 压缩后大小。
    compressed_size: u64,
    /// 解压后大小。
    uncompressed_size: u64,
    /// 在中央目录中的起始位置（用于取出原始条目字节）。
    position: usize,
}

/// 在中央目录里找目标条目。
fn find_entry(cd: &[u8], name: &str) -> Option<CdEntry> {
    let mut offset = 0usize;

    while offset + CD_FIXED_LEN <= cd.len() {
        if read_u32(cd, offset) != CD_SIGNATURE {
            // 中央目录里出现非目录项，说明数据损坏，后面不用再看了。
            return None;
        }

        let name_len = read_u16(cd, offset + 28) as usize;
        let extra_len = read_u16(cd, offset + 30) as usize;
        let comment_len = read_u16(cd, offset + 32) as usize;
        let entry_start = offset;
        let entry_end = offset + CD_FIXED_LEN + name_len + extra_len + comment_len;

        if entry_end > cd.len() {
            return None;
        }

        let raw_name = &cd[offset + CD_FIXED_LEN..offset + CD_FIXED_LEN + name_len];
        let decoded = String::from_utf8_lossy(raw_name).to_string();

        if decoded == name {
            return Some(CdEntry {
                local_offset: read_u32(cd, offset + 42) as u64,
                compressed_size: read_u32(cd, offset + 20) as u64,
                uncompressed_size: read_u32(cd, offset + 24) as u64,
                position: entry_start,
            });
        }

        offset = entry_end;
    }

    None
}

/// 用「本地头 + 数据 + 中央目录项 + EOCD」重建一个只含目标条目的 ZIP。
///
/// 中央目录项里的「本地头偏移」被改写为 0——因为重建后本地头就在最前面。
fn rebuild_single_entry(
    local_header: &[u8],
    data: &[u8],
    cd: &[u8],
    target: &CdEntry,
) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(local_header.len() + data.len() + cd.len() + EOCD_LEN);
    out.extend_from_slice(local_header);
    out.extend_from_slice(data);

    // 原始中央目录项（只取这一条），并把本地头偏移改成 0
    let cd_entry = &cd[target.position..];
    let cd_entry_len = cd_entry_len(cd_entry)?;
    let mut entry = cd_entry[..cd_entry_len].to_vec();
    entry[42..46].copy_from_slice(&0u32.to_le_bytes());

    let cd_offset = out.len() as u32;
    out.extend_from_slice(&entry);

    // EOCD：只有一条记录
    let mut eocd = vec![0u8; EOCD_LEN];
    eocd[0..4].copy_from_slice(&EOCD_SIGNATURE.to_le_bytes());
    eocd[8..10].copy_from_slice(&1u16.to_le_bytes()); // 本盘条目数
    eocd[10..12].copy_from_slice(&1u16.to_le_bytes()); // 总条目数
    eocd[12..16].copy_from_slice(&(entry.len() as u32).to_le_bytes()); // 中央目录大小
    eocd[16..20].copy_from_slice(&cd_offset.to_le_bytes()); // 中央目录偏移
    out.extend_from_slice(&eocd);

    Ok(out)
}

/// 中央目录项的总长度（含文件名、扩展字段与注释）。
fn cd_entry_len(entry: &[u8]) -> Result<usize> {
    if entry.len() < CD_FIXED_LEN {
        return Err(LauncherError::Parse {
            message: "中央目录项不完整".into(),
        });
    }
    let name_len = read_u16(entry, 28) as usize;
    let extra_len = read_u16(entry, 30) as usize;
    let comment_len = read_u16(entry, 32) as usize;
    Ok(CD_FIXED_LEN + name_len + extra_len + comment_len)
}

/// 读一段字节区间。
async fn fetch_range(
    client: &reqwest::Client,
    url: &str,
    start: u64,
    length: u64,
) -> Result<Vec<u8>> {
    if length == 0 {
        return Ok(Vec::new());
    }
    let end = start + length - 1;

    let response = client
        .get(url)
        .header("Range", format!("bytes={start}-{end}"))
        .send()
        .await
        .map_err(|e| LauncherError::DownloadFailed {
            message: format!("Range 请求失败: {e}"),
            url: url.to_string(),
        })?;

    let status = response.status();
    if status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(LauncherError::DownloadFailed {
            message: format!("Range 请求未返回 206（实际 {status}），无法按需读取"),
            url: url.to_string(),
        });
    }

    crate::utils::http::read_limited(response, length + 1024).await
}

/// 读小端 u16。字节不足时返回 0。
fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    if offset + 2 > bytes.len() {
        return 0;
    }
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// 读小端 u32。字节不足时返回 0。
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    if offset + 4 > bytes.len() {
        return 0;
    }
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range_total() {
        assert_eq!(parse_range_total("bytes 0-0/51324367"), Some(51324367));
        assert_eq!(parse_range_total("bytes 0-0/*"), None);
        assert_eq!(parse_range_total("garbage"), None);
    }

    #[test]
    fn reads_little_endian_scalars() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        assert_eq!(read_u16(&bytes, 0), 0x0201);
        assert_eq!(read_u32(&bytes, 0), 0x04030201);
        // 越界读取不 panic，返回 0
        assert_eq!(read_u16(&bytes, 5), 0);
        assert_eq!(read_u32(&bytes, 3), 0);
    }

    /// 用真实的 zip 结构验证「中央目录定位」。
    #[test]
    fn finds_entry_in_central_directory() {
        let dir = std::env::temp_dir().join("sereno-remote-zip-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pack = dir.join("pack.zip");

        {
            use std::io::Write;
            use zip::write::SimpleFileOptions;
            let file = std::fs::File::create(&pack).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = SimpleFileOptions::default();

            writer.start_file("manifest.json", options).unwrap();
            writer.write_all(br#"{"name":"t"}"#).unwrap();
            writer
                .start_file("overrides/config/a.toml", options)
                .unwrap();
            writer.write_all(b"key = 1").unwrap();
            writer.finish().unwrap();
        }

        let bytes = std::fs::read(&pack).unwrap();
        let cd_offset = find_central_directory(&bytes).unwrap();
        let cd = &bytes[cd_offset..];

        let found = find_entry(cd, "manifest.json").expect("必须找到 manifest.json");
        assert_eq!(found.uncompressed_size, 12);
        assert!(found.compressed_size > 0);

        assert!(find_entry(cd, "overrides/config/a.toml").is_some());
        assert!(find_entry(cd, "nope.json").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 从 zip 尾部反查中央目录起点（仅测试辅助）。
    fn find_central_directory(bytes: &[u8]) -> Option<usize> {
        let mut index = bytes.len().checked_sub(EOCD_LEN)?;
        loop {
            if read_u32(bytes, index) == EOCD_SIGNATURE {
                return Some(read_u32(bytes, index + 16) as usize);
            }
            index = index.checked_sub(1)?;
        }
    }
}
