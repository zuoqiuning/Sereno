//! Mod 依赖图：读 jar 内的元数据，检测重复、缺失与版本不匹配。
//!
//! 只读元数据、不解压整个 jar，成本很低；玩家装了两百个 Mod 也能秒出结果。
//!
//! 覆盖四种主流元数据格式：
//! - Fabric：`fabric.mod.json`
//! - Quilt：`quilt.mod.json`
//! - Forge / NeoForge：`META-INF/mods.toml`（NeoForge 为 `neoforge.mods.toml`）
//! - 旧版 Forge：`mcmod.info`

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::Path;

use crate::models::health::{ModDependencyIssue, ModDuplicate, ModHealth};

/// 平台自身提供的依赖，不需要用户装 Mod。
const PLATFORM_MODS: &[&str] = &[
    "minecraft",
    "fabricloader",
    "quilt_loader",
    "forge",
    "neoforge",
    "java",
    "javafml",
];

struct RawMod {
    id: String,
    name: String,
    version: Option<String>,
    dependencies: Vec<RawDep>,
}

struct RawDep {
    mod_id: String,
    mandatory: bool,
    range: Option<String>,
}

impl RawMod {
    fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            self.id.clone()
        } else {
            self.name.clone()
        }
    }
}

/// 扫描 `mods` 目录并生成依赖图检查结果。
///
/// 被禁用（`.disabled`）的文件会计数但不参与依赖检查：
/// 它们不会进入游戏，报错只会制造噪音。
pub fn analyze(mods_dir: &Path) -> ModHealth {
    let mut health = ModHealth::default();
    let mut mods = Vec::new();

    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return health;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".disabled") {
            health.disabled += 1;
            continue;
        }
        if !lower.ends_with(".jar") {
            continue;
        }

        health.enabled += 1;
        if let Some(parsed) = read_jar(&path) {
            mods.push(parsed);
        }
    }

    health.duplicates = detect_duplicates(&mods);
    let (missing, mismatched) = detect_dependency_issues(&mods);
    health.missing = missing;
    health.mismatched = mismatched;
    health
}

fn detect_duplicates(mods: &[RawMod]) -> Vec<ModDuplicate> {
    let mut by_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in mods {
        by_id
            .entry(item.id.to_lowercase())
            .or_default()
            .push(item.display_name());
    }

    by_id
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(mod_id, files)| ModDuplicate {
            mod_id,
            name: files.first().cloned().unwrap_or_default(),
            files,
        })
        .collect()
}

fn detect_dependency_issues(mods: &[RawMod]) -> (Vec<ModDependencyIssue>, Vec<ModDependencyIssue>) {
    let installed: HashMap<String, String> = mods
        .iter()
        .filter_map(|item| {
            let id = item.id.to_lowercase();
            let version = item.version.clone()?;
            Some((id, version))
        })
        .collect();

    struct Issue {
        required_by: Vec<String>,
        requirement: Option<String>,
        installed_version: Option<String>,
    }

    let mut missing: BTreeMap<String, Issue> = BTreeMap::new();
    let mut mismatched: BTreeMap<String, Issue> = BTreeMap::new();

    for owner in mods {
        for dep in &owner.dependencies {
            let dep_id = dep.mod_id.to_lowercase();
            if !dep.mandatory || dep_id.is_empty() || is_platform(&dep_id) {
                continue;
            }

            match installed.get(&dep_id) {
                Some(version) => {
                    let required = dep.range.as_deref().unwrap_or("").trim();
                    if required.is_empty() || version_satisfies(version, required) {
                        continue;
                    }
                    let issue = mismatched.entry(dep_id).or_insert(Issue {
                        required_by: Vec::new(),
                        requirement: None,
                        installed_version: Some(version.clone()),
                    });
                    push_unique(&mut issue.required_by, owner.display_name());
                    issue.requirement = Some(required.to_string());
                }
                None => {
                    let issue = missing.entry(dep_id).or_insert(Issue {
                        required_by: Vec::new(),
                        requirement: None,
                        installed_version: None,
                    });
                    push_unique(&mut issue.required_by, owner.display_name());
                    if let Some(range) = dep
                        .range
                        .as_deref()
                        .filter(|range| !range.trim().is_empty())
                    {
                        issue.requirement = Some(range.to_string());
                    }
                }
            }
        }
    }

    let to_issues = |map: BTreeMap<String, Issue>| {
        map.into_iter()
            .map(|(mod_id, issue)| ModDependencyIssue {
                mod_id,
                required_by: issue.required_by,
                requirement: issue.requirement.unwrap_or_default(),
                installed_version: issue.installed_version,
            })
            .collect::<Vec<_>>()
    };

    (to_issues(missing), to_issues(mismatched))
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

fn is_platform(id: &str) -> bool {
    PLATFORM_MODS.contains(&id)
}

fn read_jar(path: &Path) -> Option<RawMod> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    for name in [
        "fabric.mod.json",
        "quilt.mod.json",
        "META-INF/neoforge.mods.toml",
        "META-INF/mods.toml",
        "mcmod.info",
    ] {
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        let mut text = String::new();
        entry.read_to_string(&mut text).ok()?;
        if let Some(parsed) = parse_metadata(name, &text) {
            return Some(parsed);
        }
    }

    None
}

fn parse_metadata(file_name: &str, text: &str) -> Option<RawMod> {
    match file_name {
        "fabric.mod.json" => parse_fabric(text),
        "quilt.mod.json" => parse_quilt(text),
        _ if file_name.ends_with(".toml") => parse_mods_toml(text),
        "mcmod.info" => parse_mcmod(text),
        _ => None,
    }
}

fn parse_fabric(text: &str) -> Option<RawMod> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let id = value.get("id")?.as_str()?.trim().to_string();
    if id.is_empty() {
        return None;
    }

    let name = value
        .get("name")
        .and_then(|name| name.as_str())
        .unwrap_or(&id)
        .to_string();
    let version = value
        .get("version")
        .and_then(|version| version.as_str())
        .map(str::to_string);

    let mut dependencies = Vec::new();
    if let Some(depends) = value.get("depends").and_then(|depends| depends.as_object()) {
        for (mod_id, spec) in depends {
            dependencies.push(RawDep {
                mod_id: mod_id.clone(),
                mandatory: true,
                range: spec.as_str().map(str::to_string),
            });
        }
    }

    Some(RawMod {
        id,
        name,
        version,
        dependencies,
    })
}

fn parse_quilt(text: &str) -> Option<RawMod> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let loader = value.get("quilt_loader")?;
    let id = loader.get("id")?.as_str()?.trim().to_string();
    if id.is_empty() {
        return None;
    }

    let name = loader
        .get("metadata")
        .and_then(|meta| meta.get("name"))
        .and_then(|name| name.as_str())
        .unwrap_or(&id)
        .to_string();
    let version = loader
        .get("version")
        .and_then(|version| version.as_str())
        .map(str::to_string);

    let mut dependencies = Vec::new();
    if let Some(deps) = loader.get("depends").and_then(|deps| deps.as_array()) {
        for dep in deps {
            let optional = dep
                .get("optional")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if optional {
                continue;
            }
            let Some(dep_id) = dep.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            let range = dep
                .get("versions")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    dep.get("versions")
                        .filter(|value| value.is_array())
                        .and_then(|value| value.as_array())
                        .and_then(|list| list.first())
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                });
            dependencies.push(RawDep {
                mod_id: dep_id.to_string(),
                mandatory: true,
                range,
            });
        }
    }

    Some(RawMod {
        id,
        name,
        version,
        dependencies,
    })
}

/// Forge / NeoForge 的 `mods.toml`。
///
/// 只需要 `[[mods]]` 的 id 与 `[[dependencies.<自身 id>]]` 的区间，
/// 因此用一个很小的块解析器，而不是引入完整 TOML 依赖。
fn parse_mods_toml(text: &str) -> Option<RawMod> {
    #[derive(PartialEq)]
    enum Section {
        Other,
        Mods,
        Deps(String),
    }

    let mut id: Option<String> = None;
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut deps: Vec<(String, RawDep)> = Vec::new();
    let mut section = Section::Other;
    let mut pending: Option<(String, RawDep)> = None;

    fn flush(pending: &mut Option<(String, RawDep)>, out: &mut Vec<(String, RawDep)>) {
        if let Some((owner, dep)) = pending.take()
            && !dep.mod_id.is_empty()
        {
            out.push((owner, dep));
        }
    }

    for raw in text.lines() {
        let without_comment = strip_comment(raw);
        let line = without_comment.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("[[") {
            flush(&mut pending, &mut deps);
            let header = line.trim_start_matches("[[").trim_end_matches("]]").trim();
            section = if header == "mods" {
                Section::Mods
            } else if let Some(owner) = header.strip_prefix("dependencies.") {
                Section::Deps(owner.trim().to_string())
            } else {
                Section::Other
            };
            continue;
        }
        if line.starts_with('[') {
            flush(&mut pending, &mut deps);
            section = Section::Other;
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match &section {
            Section::Mods => match key {
                "modId" => id = Some(unquote(value)),
                "displayName" => name = Some(unquote(value)),
                "version" => {
                    let parsed = unquote(value);
                    // 新版 Forge 允许 `${file.jarVersion}` 占位符
                    version = (!parsed.starts_with("${")).then_some(parsed);
                }
                _ => {}
            },
            Section::Deps(owner) => {
                let needs_new = pending
                    .as_ref()
                    .is_none_or(|(current_owner, _)| current_owner != owner);
                if needs_new {
                    flush(&mut pending, &mut deps);
                    pending = Some((
                        owner.clone(),
                        RawDep {
                            mod_id: String::new(),
                            mandatory: true,
                            range: None,
                        },
                    ));
                }
                let dep = &mut pending.as_mut().expect("pending 刚被初始化").1;
                match key {
                    "modId" => dep.mod_id = unquote(value),
                    "mandatory" => dep.mandatory = value.eq_ignore_ascii_case("true"),
                    "versionRange" => dep.range = Some(unquote(value)),
                    _ => {}
                }
            }
            Section::Other => {}
        }
    }
    flush(&mut pending, &mut deps);

    let id = id?.trim().to_string();
    if id.is_empty() {
        return None;
    }
    let dependencies = deps
        .into_iter()
        .filter(|(owner, _)| owner.eq_ignore_ascii_case(&id))
        .map(|(_, dep)| dep)
        .collect();

    Some(RawMod {
        id,
        name: name.unwrap_or_default(),
        version,
        dependencies,
    })
}

fn parse_mcmod(text: &str) -> Option<RawMod> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let first = value.as_array()?.first()?;
    let id = first.get("modid")?.as_str()?.trim().to_string();

    let mut dependencies = Vec::new();
    for key in ["requiredMods", "dependencies"] {
        let Some(list) = first.get(key).and_then(|value| value.as_array()) else {
            continue;
        };
        for spec in list {
            let Some(text) = spec.as_str() else {
                continue;
            };
            let (mod_id, range) = match text.split_once('@') {
                Some((mod_id, range)) => (mod_id, Some(range.to_string())),
                None => (text, None),
            };
            dependencies.push(RawDep {
                mod_id: mod_id.to_string(),
                mandatory: true,
                range,
            });
        }
    }

    Some(RawMod {
        id,
        name: first
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or_default()
            .to_string(),
        version: first
            .get("version")
            .and_then(|version| version.as_str())
            .map(str::to_string),
        dependencies,
    })
}

/// 逐行去掉 `#` 之后的注释，但保留引号内的 `#`。
fn strip_comment(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        match quote {
            Some(current) => {
                result.push(ch);
                if ch == current {
                    quote = None;
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                    result.push(ch);
                } else if ch == '#' {
                    break;
                } else {
                    result.push(ch);
                }
            }
        }
    }
    result
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// 版本区间判断。
///
/// 支持 Maven 区间（`[1.0,)`、`(1.0,2.0]`）、比较符（`>=1.2`、`<2.0`）、
/// 通配（`1.20.x`）与精确值。解析不了的一律视为满足——
/// 宁可漏报，也不要给玩家一堆误报。
fn version_satisfies(version: &str, range: &str) -> bool {
    use std::cmp::Ordering;

    let range = range.trim();
    if range.is_empty()
        || range == "*"
        || range.eq_ignore_ascii_case("any")
        || range.eq_ignore_ascii_case("latest")
    {
        return true;
    }

    if let Some(inner) = range.strip_prefix('[').or_else(|| range.strip_prefix('(')) {
        let Some(inner) = inner.strip_suffix(']').or_else(|| inner.strip_suffix(')')) else {
            return true;
        };
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() != 2 {
            return true;
        }
        let lower_inclusive = range.starts_with('[');
        let upper_inclusive = range.ends_with(']');

        if !parts[0].is_empty() {
            let ordering = cmp_versions(version, parts[0]);
            if ordering == Ordering::Less || (!lower_inclusive && ordering == Ordering::Equal) {
                return false;
            }
        }
        if !parts[1].is_empty() {
            let ordering = cmp_versions(version, parts[1]);
            if ordering == Ordering::Greater || (!upper_inclusive && ordering == Ordering::Equal) {
                return false;
            }
        }
        return true;
    }

    use std::cmp::Ordering::*;

    if let Some(rest) = range.strip_prefix(">=") {
        return !matches!(cmp_versions(version, rest.trim()), Less);
    }
    if let Some(rest) = range.strip_prefix("<=") {
        return !matches!(cmp_versions(version, rest.trim()), Greater);
    }
    if let Some(rest) = range.strip_prefix('>') {
        return cmp_versions(version, rest.trim()) == Greater;
    }
    if let Some(rest) = range.strip_prefix('<') {
        return cmp_versions(version, rest.trim()) == Less;
    }
    if let Some(rest) = range.strip_prefix('=') {
        return cmp_versions(version, rest.trim()) == Equal;
    }
    // `~` / `^` 的语义更细，这里只保证「不低于」，避免误报
    if let Some(rest) = range.strip_prefix(['~', '^']) {
        return !matches!(cmp_versions(version, rest.trim()), Less);
    }

    if range.contains(['x', 'X', '*']) {
        let prefix = range
            .split(['x', 'X', '*'])
            .next()
            .unwrap_or("")
            .trim_end_matches('.');
        return version.starts_with(prefix);
    }

    cmp_versions(version, range) == Ordering::Equal
}

/// 按数字段比较版本；非数字部分忽略。
fn cmp_versions(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let numbers = |text: &str| -> Vec<u64> {
        text.split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u64>().ok())
            .collect()
    };

    let left = numbers(left);
    let right = numbers(right);
    if left.is_empty() || right.is_empty() {
        return Ordering::Equal;
    }

    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        match a.cmp(&b) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fabric_metadata() {
        let text = r#"{
            "id": "sodium",
            "name": "Sodium",
            "version": "0.6.13",
            "depends": { "minecraft": "1.21.x", "fabricloader": ">=0.16", "fabric-api": "*" }
        }"#;
        let parsed = parse_fabric(text).unwrap();
        assert_eq!(parsed.id, "sodium");
        assert_eq!(parsed.name, "Sodium");
        assert_eq!(parsed.version.as_deref(), Some("0.6.13"));
        assert_eq!(parsed.dependencies.len(), 3);
        assert!(
            parsed
                .dependencies
                .iter()
                .any(|dep| dep.mod_id == "fabric-api" && dep.mandatory)
        );
    }

    #[test]
    fn parses_forge_toml_and_keeps_only_own_dependencies() {
        let text = r#"
modLoader="javafml"
loaderVersion="[47,)"

[[mods]]
    modId="create"
    displayName="Create"
    version="0.5.6"

[[dependencies.create]]
    modId="forge"
    mandatory=true
    versionRange="[47,)"
    ordering="NONE"
    side="BOTH"

[[dependencies.create]]
    modId="flywheel"
    mandatory=true
    versionRange="[0.6.10,)"

[[dependencies.other]]
    modId="not-ours"
    mandatory=true
"#;
        let parsed = parse_mods_toml(text).unwrap();
        assert_eq!(parsed.id, "create");
        assert_eq!(parsed.name, "Create");
        let ids: Vec<&str> = parsed
            .dependencies
            .iter()
            .map(|dep| dep.mod_id.as_str())
            .collect();
        assert_eq!(ids, ["forge", "flywheel"], "只应保留自身 id 的依赖块");
        assert_eq!(parsed.dependencies[1].range.as_deref(), Some("[0.6.10,)"));
    }

    #[test]
    fn matches_common_version_ranges() {
        assert!(version_satisfies("1.2.3", ">=1.0"));
        assert!(!version_satisfies("0.9.0", ">=1.0"));
        assert!(version_satisfies("1.20.1", "[1.20,1.21)"));
        assert!(!version_satisfies("1.21.0", "[1.20,1.21)"));
        assert!(version_satisfies("0.6.13", "[0.6.10,)"));
        assert!(!version_satisfies("0.6.9", "[0.6.10,)"));
        assert!(version_satisfies("1.20.4", "1.20.x"));
        assert!(version_satisfies("1.2.3", "*"));
        assert!(version_satisfies("anything", "无法解析的区间"));
    }

    #[test]
    fn flags_missing_and_mismatched_dependencies() {
        let sodium = RawMod {
            id: "sodium".into(),
            name: "Sodium".into(),
            version: Some("0.6.9".into()),
            dependencies: vec![RawDep {
                mod_id: "fabric-api".into(),
                mandatory: true,
                range: Some(">=0.100.0".into()),
            }],
        };
        let creator = RawMod {
            id: "creator".into(),
            name: "Creator".into(),
            version: Some("1.0.0".into()),
            dependencies: vec![RawDep {
                mod_id: "cloth-config".into(),
                mandatory: true,
                range: Some("[10.0,)".into()),
            }],
        };
        let fabric_api = RawMod {
            id: "fabric-api".into(),
            name: "Fabric API".into(),
            version: Some("0.90.0".into()),
            dependencies: vec![],
        };

        let (missing, mismatched) = detect_dependency_issues(&[sodium, creator, fabric_api]);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].mod_id, "cloth-config");
        assert_eq!(missing[0].required_by, ["Creator"]);

        assert_eq!(mismatched.len(), 1);
        assert_eq!(mismatched[0].mod_id, "fabric-api");
        assert_eq!(mismatched[0].installed_version.as_deref(), Some("0.90.0"));
    }

    #[test]
    fn detects_duplicate_mod_ids() {
        let mods = [
            RawMod {
                id: "sodium".into(),
                name: "Sodium".into(),
                version: None,
                dependencies: vec![],
            },
            RawMod {
                id: "SODIUM".into(),
                name: "Sodium (old)".into(),
                version: None,
                dependencies: vec![],
            },
        ];
        let duplicates = detect_duplicates(&mods);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].mod_id, "sodium");
        assert_eq!(duplicates[0].files.len(), 2);
    }
}
