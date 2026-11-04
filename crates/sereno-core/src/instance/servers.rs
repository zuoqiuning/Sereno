//! 服务器列表管理：读取 / 添加 / 删除 `minecraft/servers.dat`。
//!
//! `servers.dat` 是 NBT 格式（根 Compound → `servers` List of Compound，
//! 每项含 `name` / `ip` / 可选 `icon` 字节）。本模块基于
//! `utils::nbt` 的**全量泛型**编解码——只改服务器条目，
//! 文件里的其它字段（及其原始顺序）逐比特保留。

use std::path::PathBuf;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::models::settings::AppSettings;
use crate::utils::paths::{instance_dir, validate_instance_id};
use crate::{LauncherError, Result};

/// 单个服务器的摘要。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    /// 在 servers.dat 中的下标（删除时按它定位；顺序即文件内顺序）。
    pub index: u32,
    /// 服务器显示名。
    pub name: String,
    /// 服务器地址（`host:port`）。
    pub address: String,
    /// `icon` 字段的 base64 data URL（游戏内截图）；无图标为 None。
    pub icon_data_url: Option<String>,
}

fn servers_dat(settings: &AppSettings, instance_id: &str) -> Result<PathBuf> {
    validate_instance_id(instance_id)?;
    Ok(instance_dir(settings, instance_id)?
        .join("minecraft")
        .join("servers.dat"))
}

/// 校验用户输入的地址：非空、长度上限、不含控制字符。
///
/// 注意这里**不**做 host:port 格式强校验——Minecraft 的地址写法很宽
/// （SRV 记录、域名、IPv6），宽松交给游戏自己解析，只拦明显恶意的输入。
fn validate_address(address: &str) -> Result<()> {
    if address.trim().is_empty() {
        return Err(LauncherError::InvalidConfig {
            key: "address".into(),
            message: "服务器地址不能为空".into(),
        });
    }
    if address.chars().count() > 255 {
        return Err(LauncherError::InvalidConfig {
            key: "address".into(),
            message: "服务器地址过长".into(),
        });
    }
    if address.chars().any(|c| c.is_control()) {
        return Err(LauncherError::InvalidConfig {
            key: "address".into(),
            message: "服务器地址不能包含控制字符".into(),
        });
    }
    Ok(())
}

/// 读取服务器列表（保持文件内顺序，即游戏内显示顺序）。
///
/// 没有 `servers.dat`（从未添加过服务器）返回空列表，不是错误。
pub fn list(settings: &AppSettings, instance_id: &str) -> Result<Vec<ServerSummary>> {
    let path = servers_dat(settings, instance_id)?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(&path).map_err(|e| LauncherError::Io {
        message: format!("读取 servers.dat 失败: {e}"),
    })?;
    let (_, root) = crate::utils::nbt::read_root(&bytes)?;

    let Some((_, items)) = root
        .as_compound()
        .and_then(|fields| fields.iter().find(|(name, _)| name == "servers"))
        .and_then(|(_, value)| value.as_list())
    else {
        return Ok(Vec::new());
    };

    let mut servers = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(fields) = item.as_compound() else {
            continue;
        };
        let field = |key: &str| {
            fields
                .iter()
                .find(|(name, _)| name == key)
                .and_then(|(_, value)| value.as_str())
                .unwrap_or_default()
        };
        let icon_data_url = fields
            .iter()
            .find(|(name, _)| name == "icon")
            .and_then(|(_, value)| value.as_byte_array())
            .filter(|bytes| !bytes.is_empty() && bytes.len() <= 64 * 1024)
            .map(|bytes| {
                format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )
            });
        servers.push(ServerSummary {
            index: index as u32,
            name: field("name").to_string(),
            address: field("ip").to_string(),
            icon_data_url,
        });
    }
    Ok(servers)
}

/// 添加服务器（追加到列表末尾，即游戏内显示在最后）。
///
/// 没有 `servers.dat` 时会创建一份合法的空文件结构。
pub fn add(settings: &AppSettings, instance_id: &str, name: &str, address: &str) -> Result<()> {
    validate_address(address)?;
    let trimmed_name = name.trim();
    if trimmed_name.chars().count() > 64 {
        return Err(LauncherError::InvalidConfig {
            key: "name".into(),
            message: "服务器名最长 64 个字符".into(),
        });
    }

    let path = servers_dat(settings, instance_id)?;
    let mut root = if path.is_file() {
        let bytes = std::fs::read(&path).map_err(|e| LauncherError::Io {
            message: format!("读取 servers.dat 失败: {e}"),
        })?;
        crate::utils::nbt::read_root(&bytes)?.1
    } else {
        crate::utils::nbt::compound(vec![(
            "servers",
            crate::utils::nbt::NbtValue::List(crate::utils::nbt::TagId::Compound as u8, Vec::new()),
        )])
    };

    let servers = servers_list_mut(&mut root)?;
    servers.push(crate::utils::nbt::compound(vec![
        (
            "name",
            crate::utils::nbt::NbtValue::String(trimmed_name.into()),
        ),
        (
            "ip",
            crate::utils::nbt::NbtValue::String(address.trim().into()),
        ),
    ]));

    write_atomic(&path, &root)
}

/// 取 `root.servers` 的可变列表引用（add / delete 共用）。
fn servers_list_mut(
    root: &mut crate::utils::nbt::NbtValue,
) -> Result<&mut Vec<crate::utils::nbt::NbtValue>> {
    let crate::utils::nbt::NbtValue::Compound(fields) = root else {
        return Err(LauncherError::Parse {
            message: "servers.dat 根节点不是 Compound".into(),
        });
    };
    let Some((_, value)) = fields.iter_mut().find(|(name, _)| name == "servers") else {
        return Err(LauncherError::Parse {
            message: "servers.dat 缺少 servers 字段".into(),
        });
    };
    let crate::utils::nbt::NbtValue::List(_, items) = value else {
        return Err(LauncherError::Parse {
            message: "servers 字段不是 List".into(),
        });
    };
    Ok(items)
}

/// 按下标删除服务器。
pub fn delete(settings: &AppSettings, instance_id: &str, index: u32) -> Result<()> {
    let path = servers_dat(settings, instance_id)?;
    if !path.is_file() {
        return Err(LauncherError::InvalidConfig {
            key: "server".into(),
            message: "该实例还没有服务器列表".into(),
        });
    }
    let bytes = std::fs::read(&path).map_err(|e| LauncherError::Io {
        message: format!("读取 servers.dat 失败: {e}"),
    })?;
    let mut root = crate::utils::nbt::read_root(&bytes)?.1;

    let servers = servers_list_mut(&mut root)?;
    if index as usize >= servers.len() {
        return Err(LauncherError::InvalidConfig {
            key: "server".into(),
            message: format!("服务器下标越界: {index}"),
        });
    }
    servers.remove(index as usize);

    write_atomic(&path, &root)
}

/// 原子写出（先写临时文件再换名，中途失败不损坏 servers.dat）。
fn write_atomic(path: &std::path::Path, root: &crate::utils::nbt::NbtValue) -> Result<()> {
    let bytes = crate::utils::nbt::write_root("", root);
    crate::utils::fs::atomic_write(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::nbt::{self, NbtValue, TagId};

    fn temp_settings(dir: &std::path::Path) -> AppSettings {
        AppSettings {
            instances_root: Some(dir.to_string_lossy().to_string()),
            ..AppSettings::default()
        }
    }

    #[test]
    fn add_list_delete_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = temp_settings(tmp.path());

        // 没有 servers.dat：空列表
        assert!(list(&settings, "inst1").unwrap().is_empty());

        add(&settings, "inst1", "生存服", "mc.example.com:25565").unwrap();
        add(&settings, "inst1", "创造服", "10.0.0.2").unwrap();

        let servers = list(&settings, "inst1").unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "生存服");
        assert_eq!(servers[0].index, 0);
        assert_eq!(servers[1].address, "10.0.0.2");
        assert!(servers[0].icon_data_url.is_none());

        delete(&settings, "inst1", 0).unwrap();
        let servers = list(&settings, "inst1").unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "创造服");
        assert_eq!(servers[0].index, 0);
    }

    #[test]
    fn unknown_fields_survive_edits() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = temp_settings(tmp.path());
        let dir = tmp.path().join("inst2").join("minecraft");
        std::fs::create_dir_all(&dir).unwrap();

        // 手工构造带额外字段的 servers.dat（模拟老版本 / 服务端写入的数据）
        let root = nbt::compound(vec![
            (
                "servers",
                NbtValue::List(
                    TagId::Compound as u8,
                    vec![nbt::compound(vec![
                        ("name", NbtValue::String("旧服".into())),
                        ("ip", NbtValue::String("old.example.com".into())),
                        // 原版/第三方可能写入的额外字段：必须原样保留
                        ("acceptedVersions", NbtValue::String("1.21.1".into())),
                        ("hideAddress", NbtValue::Byte(1)),
                    ])],
                ),
            ),
            ("extraRootField", NbtValue::Int(7)),
        ]);
        std::fs::write(dir.join("servers.dat"), nbt::write_root("", &root)).unwrap();

        add(&settings, "inst2", "新服", "new.example.com").unwrap();
        let servers = list(&settings, "inst2").unwrap();
        assert_eq!(servers.len(), 2);

        // 重读原始结构：额外字段必须还在
        let bytes = std::fs::read(dir.join("servers.dat")).unwrap();
        let (_, root) = nbt::read_root(&bytes).unwrap();
        let fields = root.as_compound().unwrap();
        let entry = &fields
            .iter()
            .find(|(name, _)| name == "servers")
            .unwrap()
            .1
            .as_list()
            .unwrap()
            .1[0]
            .as_compound()
            .unwrap();
        assert!(
            entry.iter().any(|(name, _)| name == "acceptedVersions"),
            "旧条目的自定义字段必须保留"
        );
        assert!(fields.iter().any(|(name, _)| name == "extraRootField"));
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = temp_settings(tmp.path());
        assert!(add(&settings, "inst3", "", "  ").is_err());
        assert!(add(&settings, "inst3", "x", "bad\naddr").is_err());
        assert!(delete(&settings, "inst3", 0).is_err());
    }
}
