//! 离线档案：创建、切换、删除。
//!
//! 每个档案在创建时固化 UUID（Mojang 离线 UUID 算法），用户名只影响显示名。
//! Minecraft 用 UUID 区分玩家数据，**换档案等于换玩家**：
//! 同一个世界里，各档案的物品与进度互不影响；
//! 想保留原有物品，就要切回创建它的那个档案。

use crate::launch::args::offline_uuid;
use crate::models::account::{OfflineProfile, kind};
use crate::models::settings::AppSettings;
use crate::{LauncherError, Result};

pub const MIN_NAME_LEN: usize = 3;
pub const MAX_NAME_LEN: usize = 16;

/// 校验离线用户名：只允许 ASCII 字母、数字、下划线。
pub fn validate_name(raw: &str) -> Result<String> {
    let name = raw.trim().to_string();
    let length = name.chars().count();
    let ascii_ok = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');

    if !(MIN_NAME_LEN..=MAX_NAME_LEN).contains(&length) || !ascii_ok {
        return Err(LauncherError::InvalidConfig {
            key: "name".into(),
            message: format!("离线用户名需为 {MIN_NAME_LEN}–{MAX_NAME_LEN} 位字母、数字或下划线"),
        });
    }
    Ok(name)
}

fn id_for(uuid: &str) -> String {
    format!("offline-{uuid}")
}

fn default_profile(settings: &AppSettings) -> OfflineProfile {
    let name = settings.profile.name.clone();
    let uuid = offline_uuid(&name);
    OfflineProfile {
        id: id_for(&uuid),
        name,
        uuid,
    }
}

/// 可展示的离线档案列表。
///
/// 老配置没有档案列表时，用当前 `profile.name` 派生一个，
/// 保证升级后至少有一个可用档案。
pub fn profiles(settings: &AppSettings) -> Vec<OfflineProfile> {
    if settings.account.offline_accounts.is_empty() {
        vec![default_profile(settings)]
    } else {
        settings.account.offline_accounts.clone()
    }
}

/// 当前生效的离线档案；ID 失效时回落到第一个。
pub fn active(settings: &AppSettings) -> OfflineProfile {
    let list = profiles(settings);
    settings
        .account
        .active_offline_id
        .as_deref()
        .and_then(|id| list.iter().find(|profile| profile.id == id).cloned())
        .unwrap_or_else(|| list[0].clone())
}

/// 写入前补齐档案列表与选中项，避免把「派生档案」弄丢。
pub fn ensure(settings: &mut AppSettings) {
    if settings.account.offline_accounts.is_empty() {
        settings.account.offline_accounts = vec![default_profile(settings)];
    }
    let valid = settings
        .account
        .active_offline_id
        .as_deref()
        .is_some_and(|id| {
            settings
                .account
                .offline_accounts
                .iter()
                .any(|profile| profile.id == id)
        });
    if !valid {
        settings.account.active_offline_id = Some(settings.account.offline_accounts[0].id.clone());
    }
}

/// 新建离线档案并立即切换过去。
pub fn add(settings: &mut AppSettings, raw_name: &str) -> Result<OfflineProfile> {
    let name = validate_name(raw_name)?;
    ensure(settings);

    if settings
        .account
        .offline_accounts
        .iter()
        .any(|profile| profile.name.eq_ignore_ascii_case(&name))
    {
        return Err(LauncherError::InvalidConfig {
            key: "name".into(),
            message: format!("已存在同名离线档案: {name}"),
        });
    }

    let uuid = offline_uuid(&name);
    let profile = OfflineProfile {
        id: id_for(&uuid),
        name: name.clone(),
        uuid,
    };
    settings.account.offline_accounts.push(profile.clone());
    activate(settings, &profile.id).expect("刚写入的档案必然存在");
    Ok(profile)
}

/// 切换到指定离线档案。
pub fn switch(settings: &mut AppSettings, id: &str) -> Result<OfflineProfile> {
    ensure(settings);
    activate(settings, id)
}

fn activate(settings: &mut AppSettings, id: &str) -> Result<OfflineProfile> {
    let profile = settings
        .account
        .offline_accounts
        .iter()
        .find(|entry| entry.id == id)
        .cloned()
        .ok_or_else(|| LauncherError::InvalidConfig {
            key: "id".into(),
            message: format!("离线档案不存在: {id}"),
        })?;

    settings.profile.name = profile.name.clone();
    settings.account.kind = kind::OFFLINE.to_string();
    settings.account.active_offline_id = Some(profile.id.clone());
    Ok(profile)
}

/// 删除离线档案；至少保留一个。
pub fn remove(settings: &mut AppSettings, id: &str) -> Result<()> {
    ensure(settings);
    if settings.account.offline_accounts.len() <= 1 {
        return Err(LauncherError::InvalidConfig {
            key: "id".into(),
            message: "至少保留一个离线档案".into(),
        });
    }

    let before = settings.account.offline_accounts.len();
    settings
        .account
        .offline_accounts
        .retain(|profile| profile.id != id);
    if settings.account.offline_accounts.len() == before {
        return Err(LauncherError::InvalidConfig {
            key: "id".into(),
            message: format!("离线档案不存在: {id}"),
        });
    }

    if settings.account.active_offline_id.as_deref() == Some(id) {
        let next = settings.account.offline_accounts[0].id.clone();
        activate(settings, &next)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(name: &str) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.profile.name = name.to_string();
        settings
    }

    #[test]
    fn rejects_non_ascii_names() {
        assert!(validate_name("玩家").is_err());
        assert!(validate_name("a b").is_err());
        assert!(validate_name("ab").is_err());
        assert!(validate_name("Player_01").is_ok());
    }

    #[test]
    fn adding_profile_keeps_uuid_stable() {
        let mut settings = settings_with("Player");
        let profile = add(&mut settings, "Alice").unwrap();
        assert_eq!(profile.uuid, offline_uuid("Alice"));
        switch(&mut settings, &profile.id).unwrap();
        assert_eq!(settings.profile.name, "Alice");
        assert_eq!(
            settings.account.active_offline_id.as_deref(),
            Some(profile.id.as_str())
        );
    }

    #[test]
    fn existing_name_is_derived_without_losing_identity() {
        let settings = settings_with("Player");
        let profile = active(&settings);
        assert_eq!(profile.name, "Player");
        assert_eq!(profile.uuid, offline_uuid("Player"));
    }

    #[test]
    fn removing_last_profile_is_rejected() {
        let mut settings = settings_with("Player");
        ensure(&mut settings);
        let id = settings.account.offline_accounts[0].id.clone();
        assert!(remove(&mut settings, &id).is_err());
    }
}
