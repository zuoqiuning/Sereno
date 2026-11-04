//! 命令层：薄适配器，按业务域分文件。
//!
//! **本层只做三件事**：参数校验、调用 core、把 `Result` 转成可序列化的错误。
//! **不写业务逻辑**，每个命令函数控制在几十行内，它是「薄适配器」而非「实现层」。
//!
//! ## 命令注册：只有一个清单
//!
//! 本文件里的 `all_commands!` 宏是**全项目唯一一份注册清单**。
//! `tauri_specta::collect_commands!` 内部同时展开 Tauri 的 handler 与 specta 的
//! 类型收集，因此不需要第二份 `generate_handler!`——同一个事实写两遍，
//! 就是等一个忘记同步的下午。

macro_rules! all_commands {
    () => {
        tauri_specta::collect_commands![
            crate::commands::settings::get_app_info,
            crate::commands::settings::log_from_frontend,
            crate::commands::version::get_version_manifest,
            crate::commands::version::get_version_detail,
            crate::commands::java::get_java_candidates,
            crate::commands::java::install_java_runtime,
            crate::commands::instance::list_instances,
            crate::commands::instance::get_running_instances,
            crate::commands::instance::create_instance,
            crate::commands::instance::delete_instance,
            crate::commands::instance::open_instance_folder,
            crate::commands::instance::select_java_executable,
            crate::commands::instance::update_instance_options,
            crate::commands::instance::set_instance_favorite,
            crate::commands::instance::read_instance_logs,
            crate::commands::backup::create_world_backup,
            crate::commands::backup::list_world_backups,
            crate::commands::backup::restore_world_backup,
            crate::commands::backup::create_content_snapshot,
            crate::commands::backup::list_content_snapshots,
            crate::commands::backup::restore_content_snapshot,
            crate::commands::backup::delete_content_snapshot,
            crate::commands::backup::pin_content_snapshot,
            crate::commands::crash::export_diagnostic,
            crate::commands::settings::get_settings,
            crate::commands::settings::save_settings,
            crate::commands::settings::select_directory,
            crate::commands::settings::finish_startup,
            crate::commands::settings::save_window_state,
            crate::commands::links::open_supporter_page,
            crate::commands::links::open_project_repo,
            crate::commands::links::open_project_issues,
            crate::commands::install::install_game,
            crate::commands::install::repair_instance,
            crate::commands::loader::install_loader,
            crate::commands::loader::get_loader_versions,
            crate::commands::resource::search_resources,
            crate::commands::resource::get_mod_versions,
            crate::commands::resource::download_resource,
            crate::commands::resource::cancel_download,
            crate::commands::resource::install_resource_dependency,
            crate::commands::resource::get_cf_mod_versions,
            crate::commands::resource::get_cf_mod_versions_batch,
            crate::commands::resource::get_resource_dependencies,
            crate::commands::resource::open_resource_page,
            crate::commands::resource::update_local_resource,
            crate::commands::resource::list_local_resources,
            crate::commands::resource::set_local_resource_enabled,
            crate::commands::resource::open_local_resource_folder,
            crate::commands::resource::import_local_resources,
            crate::commands::resource::check_local_resource_updates,
            crate::commands::modpack::get_modpack_info,
            crate::commands::modpack::install_modpack,
            crate::commands::launch::launch_game,
            crate::commands::account::get_account_state,
            crate::commands::account::start_microsoft_login,
            crate::commands::account::complete_microsoft_login,
            crate::commands::account::logout_account,
            crate::commands::account::set_offline_account,
            crate::commands::account::add_offline_account,
            crate::commands::account::switch_offline_account,
            crate::commands::account::remove_offline_account,
            crate::commands::account::activate_microsoft_account,
            crate::commands::account::get_microsoft_config,
            crate::commands::account::warm_account_session,
            crate::commands::account::open_microsoft_login,
            crate::commands::account::cancel_microsoft_login,
            crate::commands::health::check_instance_health,
            crate::commands::storage::get_storage_usage,
            crate::commands::storage::clear_storage_cache,
        ]
    };
}
pub(crate) use all_commands;

pub mod account;
pub mod backup;
pub mod crash;
pub mod health;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod links;
pub mod loader;
pub mod modpack;
pub mod resource;
pub mod settings;
pub mod storage;
pub mod version;
