//! 崩溃日志诊断：用一组可维护的规则把「堆栈」翻译成「人话」。
//!
//! ## 为什么是规则引擎而不是模型
//!
//! 崩溃日志的形态高度稳定（Java 异常类名、Mixin 报错、OOM、显卡初始化失败），
//! 规则匹配准确、可解释、零依赖，而且**每条结论都能附上「下一步怎么做」**——
//! 这比丢给用户一个笼统的「游戏崩溃了」有用得多。
//!
//! 规则表集中放在 [`RULES`]，新增一条规则只需要加一行数据，不需要改逻辑。

use crate::models::crash::{CrashAnalysis, CrashFinding};

/// 一条诊断规则。
struct Rule {
    /// 在日志里匹配的关键字（小写比较）。
    needle: &'static str,
    title: &'static str,
    detail: &'static str,
    suggestion: &'static str,
    severity: &'static str,
}

/// 规则表。顺序即优先级：越靠前越具体。
const RULES: &[Rule] = &[
    Rule {
        needle: "outofmemoryerror",
        title: "内存不足",
        detail: "游戏在分配内存时耗尽堆空间，通常是分配的 -Xmx 太小或同时开了太多程序。",
        suggestion: "在实例设置里把内存调大（整合包建议 4096 MB 以上），并关闭其它占内存的程序。",
        severity: "error",
    },
    Rule {
        needle: "unsupportedclassversionerror",
        title: "Java 版本过低",
        detail: "当前 Java 的主版本低于游戏或模组编译时使用的版本。",
        suggestion: "在设置里换用更高版本的 Java（1.20.5+ 建议 Java 21）。",
        severity: "error",
    },
    Rule {
        needle: "duplicatemodsfoundexception",
        title: "重复安装了同一个模组",
        detail: "mods 目录里存在两个文件提供同一个 mod id。",
        suggestion: "在实例的资源列表里删掉旧版本，只保留一个。",
        severity: "error",
    },
    Rule {
        needle: "missing or unsupported mandatory dependencies",
        title: "缺少必需的前置模组",
        detail: "有模组声明了必需依赖，但依赖没有被安装或被装成了不兼容版本。",
        suggestion: "打开该模组的详情页，安装它列出的前置资源后重试。",
        severity: "error",
    },
    Rule {
        needle: "incompatible mod set",
        title: "模组之间冲突",
        detail: "加载器检测到互相排斥的模组组合（常见于同时装了两个优化/渲染类模组）。",
        suggestion: "按崩溃报告里列出的冲突项，二选一保留。",
        severity: "error",
    },
    Rule {
        needle: "mixintransformererror",
        title: "Mixin 注入失败",
        detail: "某个模组试图修改的代码与当前游戏/加载器版本不匹配。",
        suggestion: "检查报错里提到的模组是否支持当前 Minecraft 与加载器版本。",
        severity: "error",
    },
    Rule {
        needle: "nosuchmethoderror",
        title: "模组与游戏版本不匹配",
        detail: "模组调用了当前版本里不存在的方法，通常是跨版本使用了模组。",
        suggestion: "换成与该游戏版本对应的模组文件。",
        severity: "error",
    },
    Rule {
        needle: "nosuchfielderror",
        title: "模组与游戏版本不匹配",
        detail: "模组读取了当前版本里不存在的字段，通常是跨版本使用了模组。",
        suggestion: "换成与该游戏版本对应的模组文件。",
        severity: "error",
    },
    Rule {
        needle: "classnotfoundexception",
        title: "类加载失败",
        detail: "缺少一个应该在游戏或模组里的类，可能文件损坏或依赖被拆掉。",
        suggestion: "先用「修复实例」重下一遍游戏文件；仍失败就检查报错里提到的模组。",
        severity: "error",
    },
    Rule {
        needle: "exception_access_violation",
        title: "显卡驱动崩溃",
        detail: "JVM 在原生代码里崩溃，最常见的原因是显卡驱动不兼容或超频。",
        suggestion: "更新显卡驱动，关闭超频；笔记本用户尝试指定独显运行。",
        severity: "error",
    },
    Rule {
        needle: "pixel format not accelerated",
        title: "OpenGL 不可用",
        detail: "当前显卡驱动不支持游戏需要的 OpenGL 版本。",
        suggestion: "安装官方显卡驱动（不要用系统自带的基础驱动），或降低游戏图形设置。",
        severity: "error",
    },
    Rule {
        needle: "glfw error",
        title: "图形窗口初始化失败",
        detail: "创建 OpenGL 窗口时失败，常见于远程桌面或虚拟机环境。",
        suggestion: "在物理机本地运行，或为虚拟机启用 GPU 加速。",
        severity: "warning",
    },
    Rule {
        needle: "sslhandshakeexception",
        title: "网络连接失败",
        detail: "启动或登录阶段 TLS 握手失败，可能是网络中断或代理配置不一致。",
        suggestion: "检查网络与系统代理设置后重试。",
        severity: "warning",
    },
    Rule {
        needle: "connection refused",
        title: "网络连接被拒绝",
        detail: "目标服务器不可达，常见于镜像源临时故障。",
        suggestion: "切换镜像源后重试，或稍后再启动。",
        severity: "warning",
    },
    Rule {
        needle: "java.lang.unsatisfiedlinkerror",
        title: "本地库加载失败",
        detail: "natives（.dll）没能加载，可能位数不匹配或被安全软件拦截。",
        suggestion: "确认使用 64 位 Java；把启动器与游戏目录加入杀毒软件白名单。",
        severity: "error",
    },
    Rule {
        needle: "the game crashed whilst",
        title: "游戏崩溃",
        detail: "Minecraft 已经写出一份崩溃报告，具体原因见下方报错。",
        suggestion: "查看下方摘出的报错行，或把崩溃报告发给模组作者。",
        severity: "error",
    },
];

/// 某些结论可以直接转成修复入口：能一步做完的，就不要让用户自己找菜单。
fn action_for(rule: &Rule) -> Option<(&'static str, &'static str)> {
    match rule.needle {
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
    match rule.needle {
        "nosuchmethoderror" | "nosuchfielderror" | "classnotfoundexception" => "medium",
        _ => "high",
    }
}

/// 对一段日志做诊断。
pub fn analyze(text: &str) -> CrashAnalysis {
    let lower = text.to_lowercase();
    let mut findings = Vec::new();
    let mut seen_titles = std::collections::HashSet::new();

    for rule in RULES {
        if !lower.contains(rule.needle) {
            continue;
        }
        // 同一标题只保留第一条（例如 NoSuchMethod 与 NoSuchField 都是「版本不匹配」）
        if !seen_titles.insert(rule.title) {
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
