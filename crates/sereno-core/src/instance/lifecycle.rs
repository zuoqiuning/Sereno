//! 实例生命周期管理：创建、读取列表。

use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::instance::{InstanceMetadata, LoaderInfo};
use crate::models::settings::AppSettings;
use crate::storage::instance::{read_metadata, write_metadata};
use crate::utils::paths::{ensure_dir, instances_root};
use crate::{LauncherError, Result};

/// 获取当前时间的 Unix 时间戳字符串。
fn now_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

/// 获取所有实例的列表。
pub fn list_instances(settings: &AppSettings) -> Result<Vec<InstanceMetadata>> {
    let root = instances_root(settings)?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut instances = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|e| LauncherError::Io {
        message: format!("读取实例根目录失败 {}: {e}", root.display()),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        // let-chain（Rust 2024 稳定）：等价于嵌套 `if`，
        // 但 clippy 不再报 collapsible_if，也少一层缩进。
        if path.is_dir()
            && let Ok(meta) = read_metadata(&path)
        {
            instances.push(meta);
        }
    }

    // 按创建时间倒序（字符串比较即可，因为都是同等位数的秒级时间戳）
    instances.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(instances)
}

/// 创建一个新的实例。
pub fn create_instance(
    settings: &AppSettings,
    game_version: &str,
    display_name: &str,
) -> Result<InstanceMetadata> {
    let root = instances_root(settings)?;
    ensure_dir(&root)?;

    // 生成一个稳定且唯一的 id
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let short_id = format!("inst-{}", timestamp);

    let inst_dir = root.join(&short_id);
    if inst_dir.exists() {
        return Err(LauncherError::InstanceAlreadyExists { id: short_id });
    }

    // 1. 创建目录骨架
    ensure_dir(&inst_dir.join(".launcher"))?;
    ensure_dir(&inst_dir.join("minecraft"))?;
    ensure_dir(&inst_dir.join("libraries"))?;
    ensure_dir(&inst_dir.join("versions"))?;

    // 2. 构造元数据
    let metadata = InstanceMetadata {
        schema_version: 1,
        id: short_id,
        display_name: display_name.to_string(),
        game_version: game_version.to_string(),
        loader: LoaderInfo {
            r#type: "vanilla".to_string(),
            version: None,
        },
        status: "created".to_string(),
        created_at: now_timestamp(),
        last_played_at: None,
        playtime_secs: 0,
        java_path: None,
        memory_mb: None,
        jvm_args: Vec::new(),
        favorite: false,
    };

    // 3. 写入磁盘
    write_metadata(&inst_dir, &metadata)?;

    Ok(metadata)
}
/// 删除实例：写入删除标记后移入回收站，而不是直接物理删除。
pub fn delete_instance(settings: &AppSettings, instance_id: &str) -> Result<()> {
    crate::utils::paths::validate_instance_id(instance_id)?;
    let root = instances_root(settings)?;
    let inst_dir = root.join(instance_id);
    if !inst_dir.exists() {
        return Err(LauncherError::InstanceNotFound {
            id: instance_id.to_string(),
        });
    }

    // 删除标记：启动时若发现它，会继续清理，避免“删了一半”的实例留在列表里
    let marker = inst_dir.join(".launcher").join("deleting");
    ensure_dir(marker.parent().unwrap())?;
    crate::utils::fs::atomic_write_str(&marker, "")?;

    match trash::delete(&inst_dir) {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&marker);
            Err(LauncherError::Io {
                message: format!("移入回收站失败 {}: {e}", inst_dir.display()),
            })
        }
    }
}

/// 启动时修复上次异常退出留下的状态。
///
/// - `status = installing`：视为安装被中断，回退为 `created`，允许用户重新安装；
/// - `.launcher/deleting` 标记：说明删除没走完，继续移入回收站。
pub fn recover_interrupted(settings: &AppSettings) -> Result<()> {
    let root = instances_root(settings)?;
    if !root.exists() {
        return Ok(());
    }

    // 清理上个进程遗留的**运行锁** `running.json`。
    //
    // 绝不能碰 `.launcher/lock.json`：它是「期望内容清单」（mods / 资源包的
    // sha1 与来源），属于用户数据；而运行锁是 `instance/lock.rs` 维护的
    // `running.json`。两者名字相近、语义完全不同——删错一个，
    // 缺失检测与内容回滚会永久失效。
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                crate::instance::lock::clear(&entry.path());
            }
        }
    }

    let entries = std::fs::read_dir(&root).map_err(|e| LauncherError::Io {
        message: format!("读取实例根目录失败 {}: {e}", root.display()),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let marker = path.join(".launcher").join("deleting");
        if marker.exists() {
            match trash::delete(&path) {
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        instance = %path.display(),
                        %error,
                        "恢复删除失败，已保留目录并清除标记"
                    );
                    let _ = std::fs::remove_file(&marker);
                }
            }
            continue;
        }

        let Ok(mut metadata) = read_metadata(&path) else {
            continue;
        };
        if metadata.status == "installing" {
            metadata.status = "created".to_string();
            if let Err(error) = write_metadata(&path, &metadata) {
                tracing::warn!(
                    instance = %path.display(),
                    %error,
                    "中断安装状态回退失败"
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        std::env::temp_dir().join(format!("sereno-lifecycle-{stamp}"))
    }

    #[test]
    fn recover_turns_installing_back_to_created() {
        let root = temp_root();
        let settings = AppSettings {
            instances_root: Some(root.display().to_string()),
            ..Default::default()
        };

        let mut metadata = create_instance(&settings, "1.20.1", "测试实例").unwrap();
        metadata.status = "installing".to_string();
        write_metadata(&root.join(&metadata.id), &metadata).unwrap();

        recover_interrupted(&settings).unwrap();

        let recovered = read_metadata(&root.join(&metadata.id)).unwrap();
        assert_eq!(recovered.status, "created");

        let _ = std::fs::remove_dir_all(&root);
    }
}
