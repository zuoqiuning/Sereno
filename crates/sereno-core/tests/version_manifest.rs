//! 版本域的端到端集成测试。
//!
//! 跑完整链路：**解析清单 → 查版本 → 解析详情 → 按系统过滤依赖 → 合并继承链**。
//! 全部读取 `tests/fixtures/versions/` 里的固件，**不发起任何网络请求**。

use std::path::PathBuf;

use sereno_core::models::version::{VersionDetail, VersionManifest};
use sereno_core::version::rules::{OsContext, applies_to_client, filter_libraries};
use sereno_core::version::{merge_inherits, parse_detail, parse_manifest};

/// 固件目录。
fn fixture(name: &str) -> String {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("versions")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取固件 {} 失败: {e}", path.display()))
}

fn load_manifest() -> VersionManifest {
    parse_manifest(&fixture("manifest.json")).expect("清单固件应能解析")
}

fn load_detail(name: &str) -> VersionDetail {
    parse_detail(&fixture(name)).expect("详情固件应能解析")
}

#[test]
fn manifest_exposes_latest_and_history() {
    let manifest = load_manifest();

    assert_eq!(manifest.len(), 5);
    assert_eq!(manifest.latest_release().unwrap().id, "1.21.1");
    assert_eq!(manifest.latest_snapshot().unwrap().id, "24w33a");
    assert_eq!(manifest.releases().len(), 3);
    assert_eq!(manifest.snapshots().len(), 1);

    // 老版本没有 complianceLevel，应该退化成 0 而不是解析失败
    let beta = manifest.find("b1.7.3").unwrap();
    assert_eq!(beta.compliance_level, 0);
}

#[test]
fn detail_carries_java_requirement_and_downloads() {
    let detail = load_detail("detail-1.21.1.json");

    assert_eq!(detail.id, "1.21.1");
    assert_eq!(
        detail.main_class.as_deref(),
        Some("net.minecraft.client.main.Main")
    );
    assert_eq!(detail.required_java_major(), Some(21));
    assert_eq!(detail.client_download().unwrap().size, 20516930);
    assert_eq!(detail.asset_index.as_ref().unwrap().id, "12");
    assert_eq!(detail.libraries.len(), 6);
}

#[test]
fn rules_select_the_right_natives_for_windows() {
    let detail = load_detail("detail-1.21.1.json");
    let windows = OsContext::new("windows", "10.0", "x86_64");

    let names: Vec<&str> = filter_libraries(&detail.libraries, &windows)
        .iter()
        .map(|l| l.name.as_str())
        .collect();

    assert!(
        names.contains(&"org.lwjgl:lwjgl-platform:3.3.3:natives-windows"),
        "Windows 应保留 windows natives: {names:?}"
    );
    assert!(
        !names.contains(&"org.lwjgl:lwjgl-platform:3.3.3:natives-macos"),
        "Windows 不应保留 macos natives: {names:?}"
    );
    assert!(
        !names.contains(&"org.lwjgl:lwjgl-platform:3.3.3:natives-linux"),
        "Windows 不应保留 linux natives: {names:?}"
    );
    assert!(
        names.contains(&"com.mojang:brigadier:1.3.10"),
        "无规则的库在所有系统都保留: {names:?}"
    );
}

#[test]
fn version_regex_gates_the_unix_common_library() {
    let detail = load_detail("detail-1.21.1.json");
    let target = "net.java.dev.jna:netty-transport-native-unix-common:4.1.86.Final";

    let win10 = filter_libraries(
        &detail.libraries,
        &OsContext::new("windows", "10.0", "x86_64"),
    );
    assert!(
        win10.iter().any(|l| l.name == target),
        "Windows 10 应命中 ^10\\. 规则"
    );

    let win7 = filter_libraries(
        &detail.libraries,
        &OsContext::new("windows", "6.1", "x86_64"),
    );
    assert!(
        !win7.iter().any(|l| l.name == target),
        "Windows 7 不应命中 ^10\\. 规则"
    );

    let linux = filter_libraries(
        &detail.libraries,
        &OsContext::new("linux", "6.8.0", "x86_64"),
    );
    assert!(
        linux.iter().any(|l| l.name == target),
        "Linux 走第二条 allow 规则，也应保留"
    );
}

#[test]
fn current_os_matches_the_running_machine() {
    let detail = load_detail("detail-1.21.1.json");
    let ctx = OsContext::current();

    // 当前系统上必须至少留下一部分依赖，不能被规则全滤光
    let kept = filter_libraries(&detail.libraries, &ctx);
    assert!(
        !kept.is_empty(),
        "当前系统（{}）上不应滤光所有依赖",
        ctx.name
    );

    // 无规则的库在任何系统都保留
    assert!(
        kept.iter().any(|l| l.name == "com.mojang:brigadier:1.3.10"),
        "无规则的库必须保留"
    );
}

#[test]
fn inheritance_merges_parent_and_child() {
    let parent = load_detail("detail-1.21.1.json");
    let child = load_detail("detail-loader.json");

    assert_eq!(child.inherits_from.as_deref(), Some("1.21.1"));

    let merged = merge_inherits(&parent, &child);

    assert_eq!(merged.id, "1.21.1-loader", "身份取子版本");
    assert_eq!(
        merged.main_class.as_deref(),
        Some("net.minecraft.client.main.Main"),
        "主类应从父版本继承"
    );
    assert_eq!(merged.required_java_major(), Some(21));
    assert!(merged.client_download().is_some(), "下载条目应从父版本继承");
    assert!(merged.inherits_from.is_none(), "合并后不再有继承关系");

    // 父 6 个 + 子 2 个，其中 lwjgl 同坐标被覆盖 → 7 个
    assert_eq!(merged.libraries.len(), 7);

    let lwjgl = merged
        .libraries
        .iter()
        .find(|l| l.name == "org.lwjgl:lwjgl:3.3.3")
        .expect("lwjgl 应存在");
    let url = &lwjgl
        .downloads
        .as_ref()
        .unwrap()
        .artifact
        .as_ref()
        .unwrap()
        .url;
    assert!(url.contains("patched"), "同坐标依赖应以子版本为准: {url}");
}

#[test]
fn resolved_dependencies_are_ready_for_classpath() {
    let parent = load_detail("detail-1.21.1.json");
    let child = load_detail("detail-loader.json");
    let merged = merge_inherits(&parent, &child);

    let windows = OsContext::new("windows", "10.0", "x86_64");
    let kept = filter_libraries(&merged.libraries, &windows)
        .into_iter()
        .filter(|l| applies_to_client(l))
        .collect::<Vec<_>>();

    // 合并后的依赖里应同时包含原版库与加载器库
    let names: Vec<&str> = kept.iter().map(|l| l.name.as_str()).collect();
    assert!(names.contains(&"com.mojang:brigadier:1.3.10"));
    assert!(names.contains(&"org.spongepowered:forge:1.21.1-52.0.16"));
    assert!(names.contains(&"org.lwjgl:lwjgl-platform:3.3.3:natives-windows"));
}
