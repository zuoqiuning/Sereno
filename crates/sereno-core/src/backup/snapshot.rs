//! 内容快照：在**改变实例内容之前**留一条可回滚的退路。
//!
//! ## 为什么必须有
//!
//! 玩家最常做、也最容易后悔的两件事是「装一个 Mod」和「换一次加载器」。
//! 删掉一个 Mod 之后发现存档打不开、换加载器之后发现整合包不兼容——
//! 这时候如果没有快照，启动器只能告诉用户「你自己想办法」。
//!
//! ## 边界
//!
//! - 只快照**内容目录**：`mods` / `config` / `resourcepacks` / `shaderpacks`；
//! - **不含** `saves`（世界存档体量大、变更模式不同，由 `backup::world` 独立负责）；
//! - **不含** `logs` / `crash-reports`（回滚日志没有意义）；
//! - 快照存放在实例自己的 `.launcher/snapshots/<id>/`，随实例一起删除；
//! - 默认保留最近 [`MAX_SNAPSHOTS`] 份，手动 `pinned` 的不参与淘汰。
//!
//! ## 回滚语义
//!
//! 回滚 = 把现有内容目录**移到一边** → 解压快照 → 成功后删除旧目录；
//! 解压失败则把旧目录移回来。任何一步失败都不会让玩家失去现有的 Mod。
//! 回滚**不再触发新快照**，否则会盖掉用户真正想回到的那个状态。

use std::fs::File;
use std::io::copy;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::models::backup::ContentSnapshot;
use crate::models::settings::AppSettings;
use crate::utils::archive::extract_zip_safely;
use crate::utils::paths::{ensure_dir, instance_dir};
use crate::{LauncherError, Result};

/// 参与快照的内容目录（相对 `minecraft/`）。
const CONTENT_DIRS: &[&str] = &["mods", "config", "resourcepacks", "shaderpacks"];

/// 自动保留的份数，不含 `pinned`。
const MAX_SNAPSHOTS: usize = 10;

const INDEX_SCHEMA_VERSION: u32 = 1;

/// `index.json` 的结构。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotIndex {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    items: Vec<ContentSnapshot>,
}

fn default_schema_version() -> u32 {
    INDEX_SCHEMA_VERSION
}

/// 快照根目录：`<实例>/.launcher/snapshots`。
fn snapshots_dir(settings: &AppSettings, instance_id: &str) -> Result<PathBuf> {
    Ok(instance_dir(settings, instance_id)?
        .join(".launcher")
        .join("snapshots"))
}

fn index_path(root: &Path) -> PathBuf {
    root.join("index.json")
}

fn read_index(root: &Path) -> SnapshotIndex {
    let Ok(raw) = std::fs::read_to_string(index_path(root)) else {
        return SnapshotIndex::default();
    };
    // 索引损坏时**不静默覆盖**：留一份 `.bak`，本次按空索引继续。
    match serde_json::from_str(&raw) {
        Ok(index) => index,
        Err(error) => {
            tracing::warn!(%error, "快照索引解析失败，本次按空索引处理");
            let _ = std::fs::rename(
                index_path(root),
                index_path(root).with_extension(format!("json.bak-{}", now_secs())),
            );
            SnapshotIndex::default()
        }
    }
}

fn write_index(root: &Path, index: &SnapshotIndex) -> Result<()> {
    let mut json = serde_json::to_string_pretty(index).map_err(|e| LauncherError::Parse {
        message: format!("序列化快照索引失败: {e}"),
    })?;
    json.push('\n');
    crate::utils::fs::atomic_write_str(&index_path(root), &json)
}

/// 创建一份快照。
///
/// `reason` 只用于展示与排障，不做枚举校验——新增触发点时不必改契约。
pub fn create(
    settings: &AppSettings,
    instance_id: &str,
    reason: &str,
    pinned: bool,
) -> Result<ContentSnapshot> {
    let inst_dir = instance_dir(settings, instance_id)?;
    let game_dir = inst_dir.join("minecraft");
    if !game_dir.is_dir() {
        return Err(LauncherError::InvalidConfig {
            key: "instanceId".into(),
            message: "实例目录不完整，无法创建快照".into(),
        });
    }

    let root = snapshots_dir(settings, instance_id)?;
    ensure_dir(&root)?;

    let id = format!("snap-{}", now_millis());
    let dir = root.join(&id);
    ensure_dir(&dir)?;

    let archive = dir.join("content.zip");
    let file_count = match write_archive(&game_dir, &archive) {
        Ok(count) => count,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(error);
        }
    };

    let size_bytes = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
    let snapshot = ContentSnapshot {
        id,
        reason: reason.to_string(),
        created_at: now_secs(),
        size_bytes,
        file_count,
        pinned,
    };

    let mut index = read_index(&root);
    index.schema_version = INDEX_SCHEMA_VERSION;
    index.items.push(snapshot.clone());
    prune(&root, &mut index)?;
    write_index(&root, &index)?;

    Ok(snapshot)
}

/// 列出快照，新的在前。
pub fn list(settings: &AppSettings, instance_id: &str) -> Result<Vec<ContentSnapshot>> {
    let root = snapshots_dir(settings, instance_id)?;
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = read_index(&root).items;
    // 索引里有、磁盘上没有的条目直接丢弃，避免界面上出现点不开的快照
    items.retain(|item| root.join(&item.id).join("content.zip").is_file());
    items.sort_by_key(|item| std::cmp::Reverse(item.created_at));
    Ok(items)
}

/// 回滚到指定快照。
pub fn restore(settings: &AppSettings, instance_id: &str, snapshot_id: &str) -> Result<()> {
    validate_snapshot_id(snapshot_id)?;

    let inst_dir = instance_dir(settings, instance_id)?;
    let game_dir = inst_dir.join("minecraft");
    let root = snapshots_dir(settings, instance_id)?;
    let archive_path = root.join(snapshot_id).join("content.zip");
    if !archive_path.is_file() {
        return Err(LauncherError::Internal {
            message: format!("快照不存在或已损坏: {snapshot_id}"),
        });
    }

    // 1. 把现有内容目录移到一边（不是直接删）
    let stamp = now_secs();
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for name in CONTENT_DIRS {
        let current = game_dir.join(name);
        if !current.exists() {
            continue;
        }
        let parked = game_dir.join(format!(".{name}.rollback-{stamp}"));
        if let Err(error) = std::fs::rename(&current, &parked) {
            // 移不动（多半是被游戏占用）→ 立刻恢复已移动的，避免半回滚
            restore_moved(&moved);
            return Err(LauncherError::Io {
                message: format!("无法暂存 {name} 目录（可能被占用）: {error}"),
            });
        }
        moved.push((current, parked));
    }

    // 2. 解压快照
    let file = File::open(&archive_path).map_err(|e| LauncherError::Io {
        message: format!("打开快照失败: {e}"),
    })?;
    if let Err(error) = extract_zip_safely(file, &game_dir) {
        restore_moved(&moved);
        return Err(error);
    }

    // 3. 成功后删除暂存目录
    for (_, parked) in moved {
        let _ = std::fs::remove_dir_all(parked);
    }

    tracing::info!(
        instance = instance_id,
        snapshot = snapshot_id,
        "内容已回滚到快照"
    );
    Ok(())
}

/// 删除一份快照（`pinned` 的也允许删，这是用户的明确意图）。
pub fn delete(settings: &AppSettings, instance_id: &str, snapshot_id: &str) -> Result<()> {
    validate_snapshot_id(snapshot_id)?;

    let root = snapshots_dir(settings, instance_id)?;
    let dir = root.join(snapshot_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| LauncherError::Io {
            message: format!("删除快照失败: {e}"),
        })?;
    }

    let mut index = read_index(&root);
    index.items.retain(|item| item.id != snapshot_id);
    write_index(&root, &index)
}

/// 切换「保留」标记。
pub fn set_pinned(
    settings: &AppSettings,
    instance_id: &str,
    snapshot_id: &str,
    pinned: bool,
) -> Result<()> {
    validate_snapshot_id(snapshot_id)?;
    let root = snapshots_dir(settings, instance_id)?;
    let mut index = read_index(&root);
    let Some(item) = index.items.iter_mut().find(|item| item.id == snapshot_id) else {
        return Err(LauncherError::Internal {
            message: format!("快照记录不存在: {snapshot_id}"),
        });
    };
    item.pinned = pinned;
    write_index(&root, &index)
}

/// 按保留策略淘汰最旧的非 `pinned` 快照。
fn prune(root: &Path, index: &mut SnapshotIndex) -> Result<()> {
    index
        .items
        .sort_by_key(|item| std::cmp::Reverse(item.created_at));

    while index.items.len() > MAX_SNAPSHOTS {
        let Some(position) = index.items.iter().rposition(|item| !item.pinned) else {
            break; // 全是 pinned：一份都不淘汰
        };
        let removed = index.items.remove(position);
        let _ = std::fs::remove_dir_all(root.join(&removed.id));
    }
    Ok(())
}

fn restore_moved(moved: &[(PathBuf, PathBuf)]) {
    for (original, parked) in moved {
        if parked.exists() {
            let _ = std::fs::remove_dir_all(original);
            let _ = std::fs::rename(parked, original);
        }
    }
}

/// 把内容目录写进 ZIP，返回文件数。
///
/// 不跟随符号链接：跟随会把实例外的文件打进快照（体积失控 + 隐私泄露）。
fn write_archive(game_dir: &Path, archive: &Path) -> Result<u32> {
    let file = File::create(archive).map_err(|e| LauncherError::Io {
        message: format!("创建快照文件失败: {e}"),
    })?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut count = 0u32;
    for name in CONTENT_DIRS {
        let dir = game_dir.join(name);
        if dir.is_dir() {
            count += add_dir(&mut writer, game_dir, &dir, options)?;
        }
    }

    writer.finish().map_err(|e| LauncherError::Io {
        message: format!("写入快照失败: {e}"),
    })?;
    Ok(count)
}

fn add_dir(
    writer: &mut ZipWriter<File>,
    root: &Path,
    current: &Path,
    options: SimpleFileOptions,
) -> Result<u32> {
    let entries = std::fs::read_dir(current).map_err(|e| LauncherError::Io {
        message: format!("读取内容目录失败 {}: {e}", current.display()),
    })?;

    let mut count = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map_err(|_| LauncherError::Internal {
                message: format!("无法计算快照相对路径: {}", path.display()),
            })?;
        let entry_name = relative.to_string_lossy().replace('\\', "/");

        if file_type.is_dir() {
            count += add_dir(writer, root, &path, options)?;
        } else if file_type.is_file() {
            writer
                .start_file(entry_name.clone(), options)
                .map_err(|e| LauncherError::Io {
                    message: format!("写入快照条目失败 {entry_name}: {e}"),
                })?;
            let mut source = File::open(&path).map_err(|e| LauncherError::Io {
                message: format!("读取文件失败 {}: {e}", path.display()),
            })?;
            copy(&mut source, writer).map_err(|e| LauncherError::Io {
                message: format!("写入快照失败 {entry_name}: {e}"),
            })?;
            count += 1;
        }
    }
    Ok(count)
}

/// 快照 ID 会拼进路径，必须只允许安全字符。
fn validate_snapshot_id(id: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');

    if valid {
        Ok(())
    } else {
        Err(LauncherError::InvalidConfig {
            key: "snapshotId".into(),
            message: format!("快照 ID 含非法字符: {id}"),
        })
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, AppSettings) {
        let root = tempfile::tempdir().unwrap();
        let settings = AppSettings {
            instances_root: Some(root.path().to_string_lossy().to_string()),
            ..AppSettings::default()
        };
        let inst = root.path().join("inst-1");
        std::fs::create_dir_all(inst.join("minecraft").join("mods")).unwrap();
        std::fs::create_dir_all(inst.join("minecraft").join("config")).unwrap();
        std::fs::create_dir_all(inst.join(".launcher")).unwrap();
        std::fs::write(inst.join("minecraft").join("mods").join("a.jar"), b"mod-a").unwrap();
        std::fs::write(
            inst.join("minecraft").join("config").join("x.toml"),
            b"k = 1\n",
        )
        .unwrap();
        (root, settings)
    }

    #[test]
    fn create_and_list_snapshot() {
        let (_root, settings) = setup();
        let snapshot = create(&settings, "inst-1", "manual", false).unwrap();
        assert_eq!(snapshot.file_count, 2);

        let items = list(&settings, "inst-1").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, snapshot.id);
        assert!(items[0].size_bytes > 0);
    }

    #[test]
    fn restore_brings_back_deleted_mod_and_config() {
        let (_root, settings) = setup();
        let snapshot = create(&settings, "inst-1", "manual", false).unwrap();

        // 用户删了 mod、改了配置、又装了一个新的
        let inst = settings.instances_root.clone().unwrap();
        let game = PathBuf::from(&inst).join("inst-1").join("minecraft");
        let mods = game.join("mods");
        std::fs::remove_file(mods.join("a.jar")).unwrap();
        std::fs::write(mods.join("b.jar"), b"mod-b").unwrap();
        std::fs::write(game.join("config").join("x.toml"), b"k = 2\n").unwrap();

        restore(&settings, "inst-1", &snapshot.id).unwrap();

        assert!(mods.join("a.jar").exists(), "回滚后旧 mod 必须回来");
        assert!(!mods.join("b.jar").exists(), "回滚后新装的 mod 必须消失");
        let config = std::fs::read_to_string(game.join("config").join("x.toml")).unwrap();
        assert!(config.contains("k = 1"), "配置必须回到快照时刻");
    }

    #[test]
    fn prune_keeps_newest_and_respects_pinned() {
        let (_root, settings) = setup();
        let root = snapshots_dir(&settings, "inst-1").unwrap();
        ensure_dir(&root).unwrap();
        let mut index = SnapshotIndex::default();
        for i in 0..12u64 {
            let id = format!("snap-{i}");
            let dir = root.join(&id);
            ensure_dir(&dir).unwrap();
            std::fs::write(dir.join("content.zip"), b"x").unwrap();
            index.items.push(ContentSnapshot {
                id,
                reason: "manual".into(),
                created_at: i,
                size_bytes: 1,
                file_count: 1,
                pinned: i == 0,
            });
        }
        prune(&root, &mut index).unwrap();

        assert_eq!(index.items.len(), MAX_SNAPSHOTS);
        assert!(
            index.items.iter().any(|item| item.id == "snap-0"),
            "pinned 的快照不能被淘汰"
        );
        assert!(
            !index.items.iter().any(|item| item.id == "snap-1"),
            "最旧的非 pinned 快照应先被淘汰"
        );
    }

    #[test]
    fn rejects_path_like_snapshot_id() {
        let (_root, settings) = setup();
        assert!(restore(&settings, "inst-1", "../../etc").is_err());
        assert!(delete(&settings, "inst-1", "a/b").is_err());
    }

    #[test]
    fn delete_removes_entry_and_directory() {
        let (_root, settings) = setup();
        let snapshot = create(&settings, "inst-1", "manual", false).unwrap();
        delete(&settings, "inst-1", &snapshot.id).unwrap();
        assert!(list(&settings, "inst-1").unwrap().is_empty());
        assert!(
            !snapshots_dir(&settings, "inst-1")
                .unwrap()
                .join(&snapshot.id)
                .exists()
        );
    }
}
