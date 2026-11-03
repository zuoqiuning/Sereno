use std::path::PathBuf;

fn main() {
    // 版本号的唯一事实源是仓库根的 version.json。
    // UI / 安装包 / Cargo / package.json 全部由它派生，不允许各自维护一份。
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let version_json = manifest_dir
        .join("..")
        .join("..")
        .join("version.json")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("..").join("..").join("version.json"));

    println!("cargo:rerun-if-changed={}", version_json.display());

    let raw = std::fs::read_to_string(&version_json)
        .unwrap_or_else(|e| panic!("读取版本号失败 {}: {e}", version_json.display()));

    let value: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("解析 version.json 失败: {e}"));

    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("version.json 缺少 version 字段"));

    println!("cargo:rustc-env=SERENO_APP_VERSION={version}");
}
