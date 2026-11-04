//! 崩溃日志诊断：用一组可维护的规则把「堆栈」翻译成「人话」。
//!
//! ## 为什么是规则引擎而不是模型
//!
//! 崩溃日志的形态高度稳定（Java 异常类名、Mixin 报错、OOM、显卡初始化失败），
//! 规则匹配准确、可解释、零依赖，而且**每条结论都能附上「下一步怎么做」**——
//! 这比丢给用户一个笼统的「游戏崩溃了」有用得多。
//!
//! 规则表集中放在 [`RULES`]，新增一条规则只需要加一行数据，不需要改逻辑。

use std::sync::LazyLock;

use crate::models::crash::{CrashAnalysis, CrashFinding};

/// 一条诊断规则。
#[derive(Debug, Clone, serde::Deserialize)]
struct Rule {
    /// 在日志里匹配的关键字（小写比较）。
    needle: String,
    title: String,
    detail: String,
    suggestion: String,
    severity: String,
}

/// 规则表：**外置**为 `crash/rules.json`（`include_str!` 编译期内嵌，
/// 仍是单一可执行文件），新增规则只需要改数据文件，不需要碰逻辑。
/// 顺序即优先级：越靠前越具体。
static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("rules.json")).expect("内置崩溃规则表必须是合法 JSON")
});

/// 某些结论可以直接转成修复入口：能一步做完的，就不要让用户自己找菜单。
fn action_for(rule: &Rule) -> Option<(&'static str, &'static str)> {
    match rule.needle.as_str() {
        "outofmemoryerror" => Some(("open-settings", "调整内存")),
        "unsupportedclassversionerror" => Some(("open-settings", "切换 Java")),
        "duplicatemodsfoundexception"
        | "missing or unsupported mandatory dependencies"
        | "incompatible mod set" => Some(("open-mods", "打开 Mod 目录")),
        _ => None,
    }
}

/// 结论的确定程度。
///
/// 只有「异常类名 + 已知根因」一一对应的规则才敢标 high；
/// 「类找不到」这类可能由文件损坏、加载器不匹配、多个原因造成的，标 medium，
/// 界面不给一键修复按钮，避免把用户带到错误的修复路径。
fn confidence_for(rule: &Rule) -> &'static str {
    match rule.needle.as_str() {
        "nosuchmethoderror" | "nosuchfielderror" | "classnotfoundexception" => "medium",
        _ => "high",
    }
}

/// 对一段日志做诊断。
pub fn analyze(text: &str) -> CrashAnalysis {
    let lower = text.to_lowercase();
    let mut findings = Vec::new();
    let mut seen_titles = std::collections::HashSet::new();

    for rule in RULES.iter() {
        if !lower.contains(rule.needle.as_str()) {
            continue;
        }
        // 同一标题只保留第一条（例如 NoSuchMethod 与 NoSuchField 都是「版本不匹配」）
        if !seen_titles.insert(rule.title.as_str()) {
            continue;
        }
        let action = action_for(rule);
        findings.push(CrashFinding {
            title: rule.title.to_string(),
            detail: rule.detail.to_string(),
            suggestion: rule.suggestion.to_string(),
            severity: rule.severity.to_string(),
            confidence: confidence_for(rule).to_string(),
            action_kind: action.map(|(kind, _)| kind.to_string()),
            action_label: action.map(|(_, label)| label.to_string()),
        });
    }

    CrashAnalysis {
        findings,
        excerpt: excerpt(text),
    }
}

/// 摘出关键报错行：只认行首的异常形态，避免把栈帧 `at ...` 当成结论。
fn excerpt(text: &str) -> Vec<String> {
    const MAX_LINES: usize = 5;
    const MAX_CHARS: usize = 300;

    let mut lines = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let is_key = trimmed.starts_with("Caused by:")
            || trimmed.starts_with("Exception in thread")
            || trimmed.starts_with("FATAL")
            || trimmed.starts_with("Mixin")
            || (trimmed.starts_with("java.")
                && (trimmed.contains("Exception") || trimmed.contains("Error")))
            || (trimmed.starts_with("net.") && trimmed.contains("Error"));
        if !is_key {
            continue;
        }

        let short: String = trimmed.chars().take(MAX_CHARS).collect();
        if seen.insert(short.clone()) {
            lines.push(short);
        }
        if lines.len() >= MAX_LINES {
            break;
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_rules_table_is_complete_and_unique() {
        assert!(
            RULES.len() >= 48,
            "规则表应至少 48 条，当前 {}",
            RULES.len()
        );
        let mut needles: Vec<&str> = RULES.iter().map(|r| r.needle.as_str()).collect();
        needles.sort_unstable();
        let before = needles.len();
        needles.dedup();
        assert_eq!(before, needles.len(), "needle 不允许重复");
        for rule in RULES.iter() {
            assert!(
                rule.severity == "error" || rule.severity == "warning",
                "非法 severity: {}",
                rule.severity
            );
            assert!(!rule.title.is_empty() && !rule.suggestion.is_empty());
        }
    }

    #[test]
    fn detects_out_of_memory() {
        let text = "java.lang.OutOfMemoryError: Java heap space\n\tat net.minecraft...";
        let analysis = analyze(text);
        assert_eq!(analysis.findings.len(), 1);
        assert_eq!(analysis.findings[0].title, "内存不足");
        assert!(analysis.excerpt[0].contains("OutOfMemoryError"));
    }

    #[test]
    fn deduplicates_similar_findings() {
        let text = "java.lang.NoSuchMethodError: foo\njava.lang.NoSuchFieldError: bar";
        let analysis = analyze(text);
        assert_eq!(analysis.findings.len(), 1, "同类结论只应出现一次");
        assert_eq!(analysis.findings[0].title, "模组与游戏版本不匹配");
    }

    #[test]
    fn empty_log_has_no_findings() {
        let analysis = analyze("");
        assert!(analysis.findings.is_empty());
        assert!(analysis.excerpt.is_empty());
    }

    #[test]
    fn excerpt_is_limited() {
        let text = (0..100)
            .map(|i| format!("java.lang.RuntimeException: line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let analysis = analyze(&text);
        assert!(analysis.excerpt.len() <= 5);
    }

    #[test]
    fn excerpt_ignores_stack_frames() {
        let text = "java.lang.NullPointerException: boom\n\tat net.example.Foo.bar(Foo.java:12)\n\tat net.example.Baz.qux(Baz.java:34)";
        let analysis = analyze(text);
        assert_eq!(analysis.excerpt.len(), 1, "栈帧不应被当成关键报错行");
        assert!(analysis.excerpt[0].starts_with("java.lang.NullPointerException"));
    }
}
