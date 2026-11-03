//! 规则求值：Mojang 版本 JSON 里的 `rules` 决定依赖库在当前系统上是否生效。
//!
//! ## 语义（与官方启动器一致）
//!
//! - **规则列表为空** → **允许**（该依赖库对所有系统生效）；
//! - 规则**按顺序**遍历，**最后一条命中的规则决定结果**；
//! - 有规则但**一条都没命中** → **不允许**（该依赖库被排除）。
//!
//! 举例（Mojang 真实数据里的典型形态）：
//!
//! ```json
//! "rules": [
//!   { "action": "allow", "os": { "name": "windows", "version": "^10\\." } },
//!   { "action": "allow", "os": { "name": "osx" } },
//!   { "action": "allow" }
//! ]
//! ```
//!
//! 在 Windows 10 上：第一条命中（版本 `10.0` 匹配 `^10\.`）→ 允许；
//! 在 Windows 7 上：第一条不命中、第二条不命中、第三条命中（无 os 条件）→ 允许。
//!
//! ## 系统版本怎么来
//!
//! Windows 用 `RtlGetVersion`（**不是** `GetVersionExW`——后者对未声明
//! 兼容清单的应用会谎报成 6.2）。Linux 读 `/proc/sys/kernel/osrelease`。
//! macOS 暂时拿不到版本号（需要 `sysctl`），此时带版本条件的规则不会命中，
//! 其余规则照常工作。

use regex::Regex;

use crate::models::version::{Library, OsSpec, Rule, RuleAction};

/// 当前系统环境：规则的求值上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsContext {
    /// 操作系统名：`windows` / `osx` / `linux`。
    pub name: String,
    /// 系统版本，如 `10.0`。探测不到时为空串。
    pub version: String,
    /// 架构：`x86_64` / `arm64` / `x86`。
    pub arch: String,
}

impl OsContext {
    /// 探测当前系统。
    pub fn current() -> Self {
        Self {
            name: os_name().to_string(),
            version: os_version(),
            arch: os_arch().to_string(),
        }
    }

    /// 构造一个自定义上下文（测试与「模拟其它系统」用）。
    pub fn new(name: &str, version: &str, arch: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            arch: arch.to_string(),
        }
    }

    /// natives 构件的分类前缀：`natives-windows` / `natives-linux` / `natives-macos`。
    pub fn natives_classifier(&self) -> &'static str {
        match self.name.as_str() {
            "windows" => "natives-windows",
            "osx" => "natives-macos",
            _ => "natives-linux",
        }
    }
}

/// 操作系统名，映射到 Mojang 的叫法。
fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        "linux" => "linux",
        // 其它 unix（freebsd 等）按 linux 处理，Mojang 只区分这三类
        other => other,
    }
}

/// 架构，映射到 Mojang 的叫法。
fn os_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "x86" => "x86",
        // Mojang 用 arm64，Rust 用 aarch64
        "aarch64" => "arm64",
        other => other,
    }
}

/// 系统版本字符串。
#[cfg(target_os = "windows")]
fn os_version() -> String {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    // SAFETY: `info` 是本函数栈上的有效结构体，
    // `dwOSVersionInfoSize` 已按结构体大小填写，
    // `RtlGetVersion` 只写入该结构体，不接受任何指针参数。
    unsafe {
        let mut info: OSVERSIONINFOW = std::mem::zeroed();
        info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;

        // NTSTATUS：0 = STATUS_SUCCESS
        let status = RtlGetVersion(&mut info);
        if status != 0 {
            tracing::warn!(status, "RtlGetVersion 调用失败，系统版本未知");
            return String::new();
        }

        format!("{}.{}", info.dwMajorVersion, info.dwMinorVersion)
    }
}

/// 系统版本字符串（非 Windows）。
#[cfg(not(target_os = "windows"))]
fn os_version() -> String {
    #[cfg(target_os = "linux")]
    {
        // 内核版本号，形如 `6.8.0-45-generic`
        if let Ok(raw) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
            let trimmed = raw.trim();
            // 只取前两段数字（主.次），与 Windows 的 "10.0" 形式对齐
            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() >= 2 {
                return format!("{}.{}", parts[0], parts[1]);
            }
            return trimmed.to_string();
        }
    }

    String::new()
}

/// 单条规则是否命中当前系统。
///
/// 没有 `os` 条件的规则对**所有系统**命中。
pub fn rule_matches(rule: &Rule, ctx: &OsContext) -> bool {
    // `features` 是官方启动器的开关（demo / quickPlay / 自定义分辨率）。
    // 我们只做普通启动，所有特性均为关闭；带特性的规则一律不命中，
    // 否则 `--demo`、`--quickPlaySingleplayer` 会被无条件塞进启动参数。
    if let Some(features) = &rule.features
        && !features.is_empty()
    {
        return false;
    }

    match &rule.os {
        None => true,
        Some(os) => os_matches(os, ctx),
    }
}

/// 平台条件是否匹配。
fn os_matches(os: &OsSpec, ctx: &OsContext) -> bool {
    if let Some(name) = &os.name
        && !name.eq_ignore_ascii_case(&ctx.name)
    {
        return false;
    }

    if let Some(arch) = &os.arch
        && !arch.eq_ignore_ascii_case(&ctx.arch)
    {
        return false;
    }

    if let Some(pattern) = &os.version {
        match Regex::new(pattern) {
            Ok(re) => {
                if !re.is_match(&ctx.version) {
                    return false;
                }
            }
            Err(e) => {
                // 正则写错时**宁可不过滤**：一条坏规则不该把整份依赖列表清空。
                tracing::warn!(
                    pattern = %pattern,
                    error = %e,
                    "规则里的版本正则无法编译，已忽略该条件"
                );
            }
        }
    }

    true
}

/// 按 Mojang 语义求值一组规则。
///
/// **最后一条命中的规则决定结果**。
///
/// 两种「没有允许」的情况要分清：
///
/// - **规则列表为空** → **允许**。没有规则的依赖库对所有系统生效
///   （Mojang 的官方启动器里就是 `rules == null || rules.isEmpty() → true`）；
/// - **有规则但一条都没命中** → 不允许。
pub fn evaluate_rules(rules: &[Rule], ctx: &OsContext) -> bool {
    // 空规则 = 对所有系统生效
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;

    for rule in rules {
        if rule_matches(rule, ctx) {
            allowed = rule.action == RuleAction::Allow;
        }
    }

    allowed
}

/// 过滤出在当前系统上生效的依赖库。
pub fn filter_libraries<'a>(libraries: &'a [Library], ctx: &OsContext) -> Vec<&'a Library> {
    libraries
        .iter()
        .filter(|lib| evaluate_rules(&lib.rules, ctx))
        .collect()
}

/// 该依赖库是否要放进客户端 classpath。
///
/// `client: false` 表示「服务端要、客户端不要」。
pub fn applies_to_client(library: &Library) -> bool {
    library.client != Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::version::RuleAction;

    fn rule(action: RuleAction, os: Option<OsSpec>) -> Rule {
        Rule {
            action,
            os,
            features: None,
        }
    }

    fn os_spec(name: Option<&str>, version: Option<&str>, arch: Option<&str>) -> OsSpec {
        OsSpec {
            name: name.map(str::to_string),
            version: version.map(str::to_string),
            arch: arch.map(str::to_string),
        }
    }

    #[test]
    fn no_rules_means_allowed_everywhere() {
        // Mojang 官方启动器：rules 为空 → 对所有系统生效
        let ctx = OsContext::new("windows", "10.0", "x86_64");
        assert!(evaluate_rules(&[], &ctx));
    }

    #[test]
    fn rule_without_os_matches_everything() {
        let ctx = OsContext::new("windows", "10.0", "x86_64");
        let rules = vec![rule(RuleAction::Allow, None)];
        assert!(evaluate_rules(&rules, &ctx));
    }

    #[test]
    fn os_name_is_matched_case_insensitively() {
        let ctx = OsContext::new("windows", "10.0", "x86_64");
        let rules = vec![rule(
            RuleAction::Allow,
            Some(os_spec(Some("WINDOWS"), None, None)),
        )];
        assert!(evaluate_rules(&rules, &ctx));
    }

    #[test]
    fn last_matching_rule_wins() {
        let ctx = OsContext::new("windows", "10.0", "x86_64");

        // 先 allow 再 disallow → 最终 disallow
        let rules = vec![
            rule(RuleAction::Allow, None),
            rule(
                RuleAction::Disallow,
                Some(os_spec(Some("windows"), None, None)),
            ),
        ];
        assert!(!evaluate_rules(&rules, &ctx));

        // 顺序反过来 → 最终 allow
        let rules = vec![
            rule(
                RuleAction::Disallow,
                Some(os_spec(Some("windows"), None, None)),
            ),
            rule(RuleAction::Allow, None),
        ];
        assert!(evaluate_rules(&rules, &ctx));
    }

    #[test]
    fn non_matching_rule_is_skipped() {
        let ctx = OsContext::new("linux", "6.8.0", "x86_64");

        // 第一条只对 windows 生效（不命中），第二条无条件 allow → 允许
        let rules = vec![
            rule(
                RuleAction::Disallow,
                Some(os_spec(Some("windows"), None, None)),
            ),
            rule(RuleAction::Allow, None),
        ];
        assert!(evaluate_rules(&rules, &ctx));

        // 只有一条不命中的 disallow → 不允许
        let rules = vec![rule(
            RuleAction::Disallow,
            Some(os_spec(Some("windows"), None, None)),
        )];
        assert!(!evaluate_rules(&rules, &ctx));
    }

    #[test]
    fn version_condition_uses_regex() {
        let win10 = OsContext::new("windows", "10.0", "x86_64");
        let win7 = OsContext::new("windows", "6.1", "x86_64");

        let rules = vec![rule(
            RuleAction::Allow,
            Some(os_spec(Some("windows"), Some("^10\\."), None)),
        )];

        assert!(evaluate_rules(&rules, &win10), "Windows 10 应命中 ^10\\.");
        assert!(!evaluate_rules(&rules, &win7), "Windows 7 不应命中 ^10\\.");
    }

    #[test]
    fn arch_condition_is_matched() {
        let ctx = OsContext::new("windows", "10.0", "x86_64");
        let rules = vec![rule(
            RuleAction::Allow,
            Some(os_spec(None, None, Some("x86_64"))),
        )];
        assert!(evaluate_rules(&rules, &ctx));

        let ctx = OsContext::new("windows", "10.0", "arm64");
        assert!(!evaluate_rules(&rules, &ctx));
    }

    #[test]
    fn broken_regex_is_ignored_not_fatal() {
        let ctx = OsContext::new("windows", "10.0", "x86_64");
        // `[` 不是一个合法的正则
        let rules = vec![rule(
            RuleAction::Allow,
            Some(os_spec(None, Some("["), None)),
        )];
        // 正则坏掉 → 该条件被忽略 → 规则仍然命中 → 允许
        assert!(evaluate_rules(&rules, &ctx));
    }

    #[test]
    fn current_context_is_sane() {
        let ctx = OsContext::current();
        assert!(!ctx.name.is_empty());
        assert!(!ctx.arch.is_empty());
        assert!(ctx.natives_classifier().starts_with("natives-"));
    }

    #[test]
    fn feature_rules_do_not_match_normal_launch() {
        use std::collections::BTreeMap;

        let rule = Rule {
            action: RuleAction::Allow,
            os: None,
            features: Some(BTreeMap::from([("is_demo_user".to_string(), true)])),
        };
        let ctx = OsContext::new("windows", "10.0", "x86_64");

        assert!(
            !rule_matches(&rule, &ctx),
            "demo / quickPlay 规则在普通启动下必须不命中"
        );
    }

    #[test]
    fn client_false_excludes_library() {
        let lib = Library {
            name: "a:b:1".into(),
            url: None,
            downloads: None,
            rules: vec![],
            natives: None,
            extract: None,
            client: Some(false),
            server: None,
        };
        assert!(!applies_to_client(&lib));
    }
}
