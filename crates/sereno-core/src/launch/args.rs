use std::collections::HashMap;

use crate::Result;
use crate::models::version::{Rule, VersionDetail, VersionType};
use crate::version::rules::{OsContext, evaluate_rules};

pub struct LaunchArgs {
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

/// 启动时由用户设置决定的参数。
pub struct LaunchOptions {
    /// 游戏内用户名。
    pub username: String,
    /// 带连字符的 UUID。离线账户留空，由启动器按用户名派生。
    pub uuid: String,
    /// 正版访问令牌。离线账户用 `"0"`。
    pub access_token: String,
    /// `legacy`（离线）/ `msa`（微软正版）。
    pub user_type: String,
    /// Xbox 用户 ID。离线账户为空；正版账户必须提供真实 XUID。
    pub xuid: String,
    /// 最大堆内存（MB）。
    pub memory_mb: u32,
    /// 是否自动追加 `--lang zh_cn`。
    pub auto_chinese: bool,
    /// 实例级附加 JVM 参数，追加在启动器基础参数之后。
    pub extra_jvm_args: Vec<String>,
}

pub fn build_args(
    version: &VersionDetail,
    classpath: &str,
    natives_dir: &str,
    game_dir: &str,
    assets_dir: &str,
    libraries_dir: &str,
    options: &LaunchOptions,
) -> Result<LaunchArgs> {
    // Windows 下的路径必须替换为正斜杠，否则 Java 会把 \ 当作转义符吃掉
    let classpath = classpath.replace('\\', "/");
    let natives_dir = natives_dir.replace('\\', "/");
    let game_dir = game_dir.replace('\\', "/");
    let assets_dir = assets_dir.replace('\\', "/");
    let libraries_dir = libraries_dir.replace('\\', "/");

    let classpath = classpath.as_str();
    let natives_dir = natives_dir.as_str();
    let game_dir = game_dir.as_str();
    let assets_dir = assets_dir.as_str();
    let libraries_dir = libraries_dir.as_str();

    let os_ctx = OsContext::current();
    let mut jvm_args = Vec::new();
    let mut game_args = Vec::new();

    let assets_index_name = version
        .asset_index
        .as_ref()
        .map(|a| a.id.as_str())
        .unwrap_or("legacy");

    // 正版账户用微软返回的 UUID；离线账户按用户名派生，保证同一用户名稳定
    let username = options.username.trim();
    let username = if username.is_empty() {
        "Player"
    } else {
        username
    };
    let uuid = if options.uuid.trim().is_empty() {
        offline_uuid(username)
    } else {
        options.uuid.clone()
    };
    let access_token = if options.access_token.trim().is_empty() {
        "0".to_string()
    } else {
        options.access_token.clone()
    };
    let user_type = if options.user_type.trim().is_empty() {
        "legacy".to_string()
    } else {
        options.user_type.clone()
    };
    let memory_mb = options.memory_mb.clamp(512, 32768);

    let mut ctx: HashMap<&str, &str> = HashMap::new();
    ctx.insert("auth_player_name", username);
    ctx.insert("version_name", &version.id);
    ctx.insert("game_directory", game_dir);
    ctx.insert("assets_root", assets_dir);
    ctx.insert("game_assets", assets_dir);
    ctx.insert("assets_index_name", assets_index_name);
    ctx.insert("auth_uuid", &uuid);
    ctx.insert("auth_access_token", &access_token);
    ctx.insert("auth_session", &access_token);
    // 正版账户传真实 XUID；离线账户没有 XUID，传 0。
    let xuid = if options.xuid.trim().is_empty() {
        "0"
    } else {
        options.xuid.trim()
    };
    ctx.insert("auth_xuid", xuid);
    ctx.insert("clientid", if user_type == "msa" { "" } else { "0" });
    ctx.insert("user_type", &user_type);
    ctx.insert("user_properties", "{}");
    ctx.insert("version_type", version_type_name(version.version_type));
    ctx.insert("classpath", classpath);
    ctx.insert("launcher_name", "Sereno");
    ctx.insert("launcher_version", crate::APP_VERSION);
    ctx.insert("natives_directory", natives_dir);
    // Forge / NeoForge 的模块路径参数依赖这两个占位符
    ctx.insert("library_directory", libraries_dir);
    ctx.insert("classpath_separator", if cfg!(windows) { ";" } else { ":" });

    // 基础 JVM 参数。内存来自设置，而不是写死 2G。
    jvm_args.push(format!("-Xmx{memory_mb}M"));
    jvm_args.push(format!("-Xms{}M", memory_mb.min(512)));
    jvm_args.push(format!("-Djava.library.path={natives_dir}"));
    jvm_args.push(format!("-Dorg.lwjgl.librarypath={natives_dir}"));
    jvm_args.push("-Dminecraft.launcher.brand=Sereno".to_string());
    jvm_args.push(format!(
        "-Dminecraft.launcher.version={}",
        crate::APP_VERSION
    ));
    jvm_args.push("-Dlog4j2.formatMsgNoLookups=true".to_string());

    if let Some(logging) = &version.logging
        && let Some(client) = &logging.client
        && let Some(file) = &client.file
    {
        let path = std::path::Path::new(game_dir)
            .join("assets")
            .join("log_configs")
            .join(&file.id);
        if path.exists() {
            // log4j 的 configurationFile 需要 URI，Windows 路径必须加 file: 前缀，
            // 否则会被当成协议名 `d`（MalformedURLException: unknown protocol: d）
            jvm_args.push(format!(
                "-Dlog4j.configurationFile=file:/{}",
                path.to_string_lossy().replace('\\', "/")
            ));
        }
    }

    // 新版 arguments（1.13+）
    if let Some(args_val) = &version.arguments {
        if let Some(jvm_arr) = args_val.get("jvm").and_then(|v| v.as_array()) {
            for arg in jvm_arr {
                if arg_matches(arg, &os_ctx) {
                    extract_values(arg, &mut jvm_args, &ctx);
                }
            }
        }
        if let Some(game_arr) = args_val.get("game").and_then(|v| v.as_array()) {
            for arg in game_arr {
                if arg_matches(arg, &os_ctx) {
                    extract_values(arg, &mut game_args, &ctx);
                }
            }
        }
    } else if let Some(mc_args) = &version.minecraft_arguments {
        // 旧版（1.12 及以前）只有空格分隔的字符串
        let parts: Vec<String> = mc_args
            .split_whitespace()
            .map(|s| replace_placeholders(s, &ctx))
            .collect();
        game_args.extend(parts);
    }

    // 版本 JSON 自带 `-cp ${classpath}`，与我们自己的 classpath 重复。
    // Java 里后出现的 -cp 会覆盖前面的，保留我们的那一份即可。
    strip_classpath_args(&mut jvm_args);

    // Forge / NeoForge 会把原版 client jar 当成第二个 `minecraft` 模块，
    // 必须通过 ignoreList 排除；版本 JSON 里的 `${version_name}` 是加载器 id，
    // 而实际的 jar 是父版本（如 1.21.1.jar），这里用真实 jar id 覆盖。
    let forgelike = version.main_class.as_deref().is_some_and(|main_class| {
        main_class.contains("bootstraplauncher") || main_class.contains("modlauncher")
    });
    if forgelike {
        let jar_id = version.jar.as_deref().unwrap_or(&version.id);
        jvm_args.push(format!("-DignoreList=client-extra,{jar_id}.jar"));
    }

    jvm_args.push("-cp".to_string());
    jvm_args.push(classpath.to_string());

    // 实例级自定义参数放在主类之前，晚于版本 JSON 的原生参数，
    // 用户写的 `-XX` / `-D` 可以覆盖前面的默认值。
    jvm_args.extend(options.extra_jvm_args.iter().cloned());

    if let Some(main_class) = &version.main_class {
        jvm_args.push(main_class.clone());
    }

    if options.auto_chinese && !game_args.iter().any(|arg| arg == "zh_cn") {
        game_args.push("--lang".to_string());
        game_args.push("zh_cn".to_string());
    }

    Ok(LaunchArgs {
        jvm_args,
        game_args,
    })
}

fn version_type_name(version_type: VersionType) -> &'static str {
    match version_type {
        VersionType::Release => "release",
        VersionType::Snapshot => "snapshot",
        VersionType::OldBeta => "old_beta",
        VersionType::OldAlpha => "old_alpha",
        VersionType::Unknown => "release",
    }
}

/// Mojang 离线 UUID：`md5("OfflinePlayer:<name>")`，改写成 v3 UUID 的位。
pub fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};

    let digest = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn arg_matches(arg: &serde_json::Value, os_ctx: &OsContext) -> bool {
    let Some(rules) = arg.get("rules").and_then(|v| v.as_array()) else {
        return true;
    };
    let parsed: Vec<Rule> = rules
        .iter()
        .filter_map(|r| serde_json::from_value(r.clone()).ok())
        .collect();
    evaluate_rules(&parsed, os_ctx)
}

fn extract_values(arg: &serde_json::Value, target: &mut Vec<String>, ctx: &HashMap<&str, &str>) {
    if let Some(s) = arg.as_str() {
        target.push(replace_placeholders(s, ctx));
        return;
    }

    if let Some(val) = arg.get("value") {
        if let Some(s) = val.as_str() {
            target.push(replace_placeholders(s, ctx));
        } else if let Some(arr) = val.as_array() {
            for v in arr {
                if let Some(s) = v.as_str() {
                    target.push(replace_placeholders(s, ctx));
                }
            }
        }
    }
}

fn replace_placeholders(s: &str, ctx: &HashMap<&str, &str>) -> String {
    let mut result = s.to_string();
    for (k, v) in ctx {
        result = result.replace(&format!("${{{}}}", k), v);
    }
    result
}

/// 删掉 `-cp <value>` 对，避免与启动器自己的 classpath 重复。
fn strip_classpath_args(args: &mut Vec<String>) {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-cp" || args[i] == "-classpath" {
            i += 2;
            continue;
        }
        out.push(args[i].clone());
        i += 1;
    }
    *args = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_is_stable_and_valid() {
        let a = offline_uuid("Player");
        assert_eq!(a, offline_uuid("Player"));
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "3", "必须是 v3 UUID");
    }

    #[test]
    fn strip_classpath_removes_json_pair() {
        let mut args = vec!["-Xmx2G".to_string(), "-cp".to_string(), "a:b".to_string()];
        strip_classpath_args(&mut args);
        assert_eq!(args, vec!["-Xmx2G"]);
    }

    #[test]
    fn forgelike_ignore_list_uses_parent_jar() {
        let json = r#"{
            "id": "neoforge-21.1.265",
            "type": "release",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "jar": "1.21.1",
            "libraries": [],
            "arguments": {"jvm": ["-DignoreList=client-extra,neoforge-21.1.265.jar"]}
        }"#;
        let version: VersionDetail = serde_json::from_str(json).unwrap();
        let args = build_args(
            &version,
            "cp.jar",
            "natives",
            "game",
            "assets",
            "libraries",
            &LaunchOptions {
                username: "Player".into(),
                uuid: String::new(),
                access_token: "0".into(),
                user_type: "legacy".into(),
                xuid: String::new(),
                memory_mb: 2048,
                auto_chinese: false,
                extra_jvm_args: Vec::new(),
            },
        )
        .unwrap();

        assert!(
            args.jvm_args
                .iter()
                .any(|arg| arg == "-DignoreList=client-extra,1.21.1.jar"),
            "必须用真实 client jar 覆盖 ignoreList：{:?}",
            args.jvm_args
        );
    }

    #[test]
    fn extra_jvm_args_are_appended_before_main_class() {
        let json = r#"{
            "id": "1.21.1",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": []
        }"#;
        let version: VersionDetail = serde_json::from_str(json).unwrap();
        let args = build_args(
            &version,
            "cp.jar",
            "natives",
            "game",
            "assets",
            "libraries",
            &LaunchOptions {
                username: "Player".into(),
                uuid: String::new(),
                access_token: "0".into(),
                user_type: "legacy".into(),
                xuid: String::new(),
                memory_mb: 2048,
                auto_chinese: false,
                extra_jvm_args: vec!["-XX:+UseG1GC".into()],
            },
        )
        .unwrap();

        let extra = args
            .jvm_args
            .iter()
            .position(|arg| arg == "-XX:+UseG1GC")
            .expect("附加参数必须被加入 JVM 参数");
        let main = args
            .jvm_args
            .iter()
            .position(|arg| arg == "net.minecraft.client.main.Main")
            .expect("主类必须存在");
        assert!(extra < main, "附加参数必须位于主类之前");
    }

    #[test]
    fn demo_and_quickplay_arguments_are_not_added() {
        let json = r#"{
            "id": "1.21.1",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": [],
            "arguments": {
                "game": [
                    {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
                    {"rules": [{"action": "allow", "features": {"is_quick_play_singleplayer": true}}], "value": "--quickPlaySingleplayer"},
                    "--username", "${auth_player_name}"
                ]
            }
        }"#;
        let version: VersionDetail = serde_json::from_str(json).unwrap();
        let args = build_args(
            &version,
            "cp.jar",
            "natives",
            "game",
            "assets",
            "libraries",
            &LaunchOptions {
                username: "Player".into(),
                uuid: String::new(),
                access_token: "0".into(),
                user_type: "legacy".into(),
                xuid: String::new(),
                memory_mb: 2048,
                auto_chinese: false,
                extra_jvm_args: Vec::new(),
            },
        )
        .unwrap();

        assert!(!args.game_args.iter().any(|arg| arg == "--demo"));
        assert!(
            !args
                .game_args
                .iter()
                .any(|arg| arg == "--quickPlaySingleplayer")
        );
        assert!(args.game_args.iter().any(|arg| arg == "--username"));
    }

    #[test]
    fn microsoft_account_args_use_real_token_and_uuid() {
        let json = r#"{
            "id": "1.21.1",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": [],
            "arguments": {
                "game": ["--uuid", "${auth_uuid}", "--accessToken", "${auth_access_token}", "--xuid", "${auth_xuid}"]
            }
        }"#;
        let version: VersionDetail = serde_json::from_str(json).unwrap();
        let args = build_args(
            &version,
            "cp.jar",
            "natives",
            "game",
            "assets",
            "libraries",
            &LaunchOptions {
                username: "Steve".into(),
                uuid: "8667ba71-b85a-4004-af54-457a9734eed7".into(),
                access_token: "ms-token".into(),
                user_type: "msa".into(),
                xuid: "2535411234567890".into(),
                memory_mb: 2048,
                auto_chinese: false,
                extra_jvm_args: Vec::new(),
            },
        )
        .unwrap();

        assert!(
            args.game_args
                .iter()
                .any(|arg| arg == "8667ba71-b85a-4004-af54-457a9734eed7"),
            "正版 UUID 必须原样传入：{:?}",
            args.game_args
        );
        assert!(
            args.game_args.iter().any(|arg| arg == "ms-token"),
            "正版访问令牌必须原样传入：{:?}",
            args.game_args
        );
        assert!(
            args.game_args.iter().any(|arg| arg == "2535411234567890"),
            "正版 XUID 必须原样传入：{:?}",
            args.game_args
        );
    }

    #[test]
    fn offline_account_still_derives_uuid() {
        let json = r#"{
            "id": "1.21.1",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": [],
            "arguments": {"game": ["--uuid", "${auth_uuid}"]}
        }"#;
        let version: VersionDetail = serde_json::from_str(json).unwrap();
        let args = build_args(
            &version,
            "cp.jar",
            "natives",
            "game",
            "assets",
            "libraries",
            &LaunchOptions {
                username: "Player".into(),
                uuid: String::new(),
                access_token: "0".into(),
                user_type: "legacy".into(),
                xuid: String::new(),
                memory_mb: 2048,
                auto_chinese: false,
                extra_jvm_args: Vec::new(),
            },
        )
        .unwrap();

        assert!(
            args.game_args
                .iter()
                .any(|arg| arg == &offline_uuid("Player")),
            "离线账户必须派生 UUID：{:?}",
            args.game_args
        );
    }
}
