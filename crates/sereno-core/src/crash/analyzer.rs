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
    // ---- 以下为第二轮扩充（对照 PCL ModCrash 的常见场景）----
    Rule {
        needle: "gc overhead limit exceeded",
        title: "内存不足（GC 超时）",
        detail: "JVM 花了 98% 的时间做垃圾回收却回收不出空间——内存分配严重不足或存在泄漏。",
        suggestion: "把实例内存调大到 6144 MB 以上；若整合包很大，考虑关闭其它程序。",
        severity: "error",
    },
    Rule {
        needle: "net.minecraftforge.fml.loading.immediatedevexception",
        title: "Forge 开发版文件误入正式环境",
        detail: "mods 里混入了仅用于开发环境的文件，正式游戏无法加载。",
        suggestion: "删除 mods 目录里名字带 -dev 或 -sources 的 jar。",
        severity: "error",
    },
    Rule {
        needle: "invalid mods.toml",
        title: "mods.toml 文件损坏",
        detail: "某个模组的 mods.toml 描述文件格式不合法（下载不完整或文件被改坏）。",
        suggestion: "重新下载报错的那个模组；无法定位时全部重装。",
        severity: "error",
    },
    Rule {
        needle: "missing or invalid mandatory dependencies",
        title: "前置模组版本不匹配",
        detail: "有前置装了，但版本不满足要求（过低或过高）。",
        suggestion: "按崩溃报告列出的期望版本更新前置模组。",
        severity: "error",
    },
    Rule {
        needle: "shaders mod detected",
        title: "光影与模组冲突",
        detail: "检测到光影加载器与其它渲染类模组（如 OptiFine / Iris 组合不当）冲突。",
        suggestion: "只保留一种渲染方案：要么 OptiFine + 光影，要么 Iris + 光影。",
        severity: "error",
    },
    Rule {
        needle: "java.lang.module.resolutionexception",
        title: "模块系统冲突（JPMS）",
        detail: "两个模组声明了同一个 Java 模块名，通常是其中一个的旧版本没删干净。",
        suggestion: "检查崩溃报告提到的两个 jar，删除不再使用的那个。",
        severity: "error",
    },
    Rule {
        needle: "lwjgl version mismatch",
        title: "LWJGL 版本不匹配",
        detail: "模组自带的 LWJGL 与游戏所需的版本冲突。",
        suggestion: "删除自带 LWJGL 的模组，或使用与游戏版本匹配的替代品。",
        severity: "error",
    },
    Rule {
        needle: "address already in use",
        title: "端口被占用",
        detail: "游戏或本地服务端要监听的端口已被其它程序占用。",
        suggestion: "关闭占用该端口的程序，或更换端口后重试。",
        severity: "error",
    },
    Rule {
        needle: "hs_err_pid",
        title: "Java 进程异常终止",
        detail: "JVM 本身崩溃并写出了 hs_err 日志（非游戏逻辑错误），通常与显卡驱动或内存超配有关。",
        suggestion: "更新显卡驱动；把内存从上限下调一档再试。",
        severity: "error",
    },
    Rule {
        needle: "javaw.exe has stopped working",
        title: "Java 进程被系统中止",
        detail: "Windows 报告 Java 进程停止响应，多为驱动兼容性问题。",
        suggestion: "更新显卡驱动；检查是否混用了不匹配的 natives。",
        severity: "error",
    },
    Rule {
        needle: "com.electronwill.nightconfig",
        title: "配置文件解析失败",
        detail: "某个模组的 TOML 配置损坏（写入中断或手工编辑出错）。",
        suggestion: "删除 config 目录里报错提到的那个配置文件，重启游戏会重新生成。",
        severity: "error",
    },
    Rule {
        needle: "registry remapping failed",
        title: "注册表合并失败",
        detail: "模组向游戏注册表注册了重复或非法的条目，常见于模组版本与游戏版本不匹配。",
        suggestion: "确认所有模组都支持当前游戏版本；移除最近新装的那个。",
        severity: "error",
    },
    Rule {
        needle: "failed to create shader",
        title: "光影着色器编译失败",
        detail: "显卡不支持该光影所需的特性，或驱动过于陈旧。",
        suggestion: "更新显卡驱动；更换低配版光影。",
        severity: "error",
    },
    Rule {
        needle: "io.netty.channel",
        title: "网络通道初始化失败",
        detail: "联机相关组件初始化出错，可能是模组改动了网络协议栈。",
        suggestion: "移除与网络/联机相关的模组后重试。",
        severity: "warning",
    },
    Rule {
        needle: "ticking entity",
        title: "实体逻辑崩溃",
        detail: "某个游戏内实体的更新逻辑出错，常由存档损坏或模组 bug 触发。",
        suggestion: "移动到崩溃发生的坐标之外；或用备份恢复该区域存档。",
        severity: "error",
    },
    Rule {
        needle: "ticking block",
        title: "方块逻辑崩溃",
        detail: "某个方块更新逻辑出错，进服即崩通常是该坐标上的方块数据损坏。",
        suggestion: "用 MCE 等工具清理该坐标方块，或恢复存档备份。",
        severity: "error",
    },
    Rule {
        needle: "optifine is not compatible",
        title: "OptiFine 与当前环境不兼容",
        detail: "OptiFine 与加载器 / 其它模组组合不被支持。",
        suggestion: "按 OptiFine 官方说明选择与游戏版本、加载器匹配的组合。",
        severity: "error",
    },
    Rule {
        needle: "duplicate module",
        title: "模块重复声明",
        detail: "两个 jar 声明了同一个模块，多为同一个模组的新旧两个文件共存。",
        suggestion: "在 mods 目录里只保留每个模组的最新版本。",
        severity: "error",
    },
    Rule {
        needle: "modfile resolution failure",
        title: "模组文件解析失败",
        detail: "jar 结构损坏，无法作为模组加载（下载中断或被杀毒软件破坏）。",
        suggestion: "重新下载该模组，并把游戏目录加入杀毒白名单。",
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
