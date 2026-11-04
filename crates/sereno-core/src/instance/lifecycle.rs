//! 实例生命周期管理：创建、读取列表。

use std::path::{Path, PathBuf};
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

    // 指纹索引：每个实例目录只 stat 一次 instance.json（大小 + mtime），
    // 200+ 实例时 stat 一遍是毫秒级，而读+解析 200 个 JSON 是秒级。
    // 指纹命中 → 直接返回索引；未命中 → 全量重扫并更新索引。
    // 任何写路径（创建/删除/收藏/分组）都会改 instance.json 的 mtime，
    // 指纹自动失配，无需手动失效。
    let mut fingerprint_inputs: Vec<(String, u64, u64)> = Vec::new();
    let mut dir_paths: Vec<(String, PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|e| LauncherError::Io {
        message: format!("读取实例根目录失败 {}: {e}", root.display()),
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        let meta_path = path.join(".launcher").join("instance.json");
        let (size, mtime) = std::fs::metadata(&meta_path)
            .map(|m| (m.len(), mtime_millis(&m)))
            .unwrap_or((0, 0));
        fingerprint_inputs.push((name.clone(), size, mtime));
        dir_paths.push((name, path));
    }
    fingerprint_inputs.sort();

    let fingerprint = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for (name, size, mtime) in &fingerprint_inputs {
            name.hash(&mut hasher);
            size.hash(&mut hasher);
            mtime.hash(&mut hasher);
        }
        hasher.finish()
    };

    if let Some(cached) = load_instances_index()
        && cached.fingerprint == fingerprint
    {
        let mut instances = cached.entries;
        instances.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        return Ok(instances);
    }

    let mut instances = Vec::new();
    for (_, path) in &dir_paths {
        // let-chain（Rust 2024 稳定）：等价于嵌套 `if`，
        // 但 clippy 不再报 collapsible_if，也少一层缩进。
        if let Ok(meta) = read_metadata(path) {
            instances.push(meta);
        }
    }

    // 按创建时间倒序（字符串比较即可，因为都是同等位数的秒级时间戳）
    instances.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    save_instances_index(InstancesIndex {
        schema_version: INDEX_SCHEMA_VERSION,
        fingerprint,
        entries: instances.clone(),
    });

    Ok(instances)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct InstancesIndex {
    schema_version: u32,
    fingerprint: u64,
    entries: Vec<crate::models::instance::InstanceMetadata>,
}

const INDEX_SCHEMA_VERSION: u32 = 1;

fn instances_index_file() -> Result<PathBuf> {
    Ok(crate::utils::paths::state_dir()?.join("instances-index.json"))
}

fn load_instances_index() -> Option<InstancesIndex> {
    let bytes = std::fs::read(instances_index_file().ok()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save_instances_index(index: InstancesIndex) {
    let Ok(path) = instances_index_file() else {
        return;
    };
    match serde_json::to_vec(&index) {
        Ok(bytes) => {
            if let Err(error) = crate::utils::fs::atomic_write(&path, &bytes) {
                tracing::warn!(%error, "实例索引写入失败（不影响正确性）");
            }
        }
        Err(error) => tracing::warn!(%error, "实例索引序列化失败"),
    }
}

fn mtime_millis(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
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
        group: None,
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

    // 跨进程保护：启动器重启后内存里的 `running` 集合是空的，
    // 但游戏很可能还在跑。只信内存集合会让用户把运行中的实例删进回收站，
    // 存档被中途写入半截。这里以磁盘上的运行锁为准再判一次。
    if crate::instance::lock::alive_pid(&inst_dir).is_some() {
        return Err(LauncherError::InstanceBusy {
            id: instance_id.to_string(),
        });
    }

    // 删除标记：启动时若发现它，会继续清理，避免“删了一半”的实例留在列表里
    let marker = inst_dir.join(".launcher").join("deleting");
    ensure_dir(marker.parent().unwrap())?;
    crate::utils::fs::atomic_write_str(&marker, "")?;

    // 回收站失败的三种真实场景（实测）：
    //   1. 安装被取消后，下载协程可能仍短暂持有文件句柄；
    //   2. 实例目录文件数巨大 / 路径过深，Windows IFileOperation 部分失败
    //     （报「Some operations were aborted」）；
    //   3. 回收站对跨盘移动大目录偶发失败。
    // 处理：等待句柄释放后重试一次；仍失败则**回退物理删除**——
    // 用户已明确确认删除意图，卡死在「删不掉」比不可恢复更糟。
    match trash::delete(&inst_dir) {
        Ok(_) => Ok(()),
        Err(first) => {
            std::thread::sleep(std::time::Duration::from_millis(600));
            match trash::delete(&inst_dir) {
                Ok(_) => Ok(()),
                Err(second) => {
                    let _ = std::fs::remove_file(&marker);
                    tracing::warn!(
                        instance = %inst_dir.display(),
                        first = %first,
                        second = %second,
                        "移入回收站两次失败，回退物理删除"
                    );
                    crate::utils::fs::remove_dir_all(&inst_dir).map_err(|e| LauncherError::Io {
                        message: format!(
                            "移入回收站失败（{} / {}），物理删除也失败: {e}",
                            first, second
                        ),
                    })
                }
            }
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
                // 只清理**确实已死**的运行锁：`alive_pid` 会按 PID + 进程创建时间
                // 校验，失效才删。绝不能无条件 `clear`——启动器重启时游戏可能
                // 还在跑，把锁抹掉就会让运行中的实例被当成空闲：能被删除、
                // 能再次启动，两个进程同时写同一份存档等于坏档。
                let _ = crate::instance::lock::alive_pid(&entry.path());
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
                    // 同 delete_instance：回退物理删除，彻底清掉「删了一半」的目录
                    tracing::warn!(
                        instance = %path.display(),
                        %error,
                        "恢复删除移入回收站失败，回退物理删除"
                    );
                    if let Err(remove_error) = crate::utils::fs::remove_dir_all(&path) {
                        tracing::error!(%remove_error, "物理删除也失败，保留目录并清除标记");
                        let _ = std::fs::remove_file(&marker);
                    }
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
