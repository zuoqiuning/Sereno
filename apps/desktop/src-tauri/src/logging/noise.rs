//! 日志噪音过滤：让「预期内」的 ERROR 不进 formatter。
//!
//! ## 为什么要自己加一层
//!
//! `EnvFilter` 只能按 target 与级别过滤，**没法按消息内容过滤**。
//! 而有些 ERROR 是**预期内**的——它们不代表故障，只是每次都会出现。
//! 让它们占着 ERROR 的位置，真正的问题反而被淹没。
//!
//! ## 当前处理的一条
//!
//! 仓库还没发过第一个 Release 时，两个更新端点都返回 404，
//! `tauri-plugin-updater` 内部会打一条：
//!
//! ```text
//! update endpoint did not respond with a successful status code
//! ```
//!
//! 但这**不是故障**——插件把 404 归为 `ReleaseNotFound`，
//! 命令层（`commands/update.rs`）已经把它翻译成「已是最新版本」。
//! 用户看到满屏红色会以为启动器坏了。
//!
//! ## 为什么在 formatter 层拦
//!
//! 试过两个更"正统"的位置，都拦不住：
//!
//! - **`Layer::on_event`**：`Layered::on_event` 在调用外层 `layer.on_event()`
//!   之后，会**无条件**把事件继续传给内层。在 `on_event` 里 `return`
//!   只会跳过后面的代码，事件照样到得了 formatter。
//! - **`Filter::event_enabled`**：它只对**自己那一层**生效，
//!   文档明说「如果别的层仍然想要这个事件，它照样会被记录」。
//!
//! `FormatEvent::format_event` 才是真正的最后一关——它拿到完整事件，
//! 不写任何东西就等于这条日志从未存在。
//!
//! ## 线索去哪了
//!
//! 噪音被吞掉后，命令层（`commands/update.rs`）会在正常事件流里
//! 补一条 DEBUG——那里不受 formatter 重入限制。

use tracing::Subscriber;
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::registry::LookupSpan;

/// 包装一个 `FormatEvent`，跳过已知无害的事件。
pub struct SkipNoise<F> {
    inner: F,
}

impl<F> SkipNoise<F> {
    /// 包一层。
    pub fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<S, N, F> FormatEvent<S, N> for SkipNoise<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
    F: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        if is_updater_no_release(event) {
            // 一个字都不写——这条日志等于从未存在。
            //
            // **不要在这里补一条自己的日志**：formatter 内部再发事件会被
            // tracing 的重入保护挡掉（而且有递归风险）。
            // 线索由命令层在正常事件流里补，见 `commands/update.rs`。
            return Ok(());
        }

        self.inner.format_event(ctx, writer, event)
    }
}

/// 判断这条事件是不是「更新端点还没有可用版本」。
///
/// ## target 有两个来源，都要看
///
/// `tauri-plugin-updater` 用的是 **`log` crate** 的宏
/// （`log::error!`），经 `tracing-log` 的 `LogTracer` 桥接过来。
/// 桥接时：
///
/// - 事件本身的 target 被设成 `"log"`；
/// - 原始 target（`tauri_plugin_updater::updater`）放在 **`log.target` 字段**里。
///
/// 而直接用 `tracing::error!` 打的日志，target 就在事件元数据上。
/// 两条路径都得认，否则会漏掉其中一种。
fn is_updater_no_release(event: &tracing::Event<'_>) -> bool {
    let from_updater = event
        .metadata()
        .target()
        .starts_with("tauri_plugin_updater")
        || read_field(event, "log.target")
            .is_some_and(|target| target.starts_with("tauri_plugin_updater"));
    if !from_updater {
        return false;
    }

    read_field(event, "message").is_some_and(|text| text.contains("successful status code"))
}

/// 读出事件的某个字段内容。
///
/// ## 为什么不能用 `Field::to_string()`
///
/// 那个方法返回的是**字段名**（永远是 `"message"`），不是字段值。
/// 字段值只能通过 `Visit` 协议读——这正是 tracing 的设计：
/// 值要在事件发生的瞬间由调用方格式化，不在元数据里存。
fn read_field(event: &tracing::Event<'_>, want: &'static str) -> Option<String> {
    // `Visit` 的方法不能捕获外部变量，所以把要找的字段名放进 visitor 自己。
    struct FieldVisitor {
        want: &'static str,
        found: Option<String>,
    }

    impl tracing::field::Visit for FieldVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == self.want {
                self.found = Some(value.to_string());
            }
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == self.want {
                // `format_args!` 走的是 Debug 路径，值可能带引号。
                let text = format!("{value:?}");
                self.found = Some(text.trim_matches('"').to_string());
            }
        }
    }

    let mut visitor = FieldVisitor { want, found: None };
    event.record(&mut visitor);
    visitor.found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::layer::SubscriberExt;

    /// 收集事件消息的 writer。
    #[derive(Clone, Default)]
    struct CollectWriter(Arc<Mutex<Vec<String>>>);

    impl std::io::Write for CollectWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let text = String::from_utf8_lossy(buf).to_string();
            self.0.lock().unwrap().push(text);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CollectWriter {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// 安装 `LogTracer`——`log` crate 的事件要经它桥接才能进 tracing。
    ///
    /// 全局只能装一次，用 `OnceLock` 保证。
    /// **生产环境里这一步由 Tauri 完成**，这里只是让测试能覆盖
    /// `tauri-plugin-updater` 实际使用的 `log::error!` 路径。
    fn install_log_tracer() {
        static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            let _ = tracing_log::LogTracer::init();
        });
    }

    /// 跑一段日志收集，返回所有输出行。
    fn capture(f: impl FnOnce()) -> String {
        install_log_tracer();
        let writer = CollectWriter::default();
        let sink = writer.0.clone();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
                .event_format(SkipNoise::new(
                    tracing_subscriber::fmt::format::Format::default(),
                )),
        );
        let _guard = tracing::subscriber::set_default(subscriber);
        f();
        sink.lock().unwrap().join("")
    }

    #[test]
    fn updater_404_never_reaches_the_formatter() {
        let out = capture(|| {
            tracing::error!(
                target: "tauri_plugin_updater::updater",
                "update endpoint did not respond with a successful status code"
            );
        });

        assert!(
            !out.contains("ERROR"),
            "更新端点的 404 不该再打成 ERROR：{out}"
        );
        assert!(
            !out.contains("successful status code"),
            "噪音原文不该出现：{out}"
        );
    }

    #[test]
    fn other_errors_are_untouched() {
        let out = capture(|| {
            tracing::error!(target: "sereno_core::whatever", "真正的错误");
        });

        assert!(out.contains("ERROR"), "普通错误不该被吞掉：{out}");
        assert!(out.contains("真正的错误"));
    }

    /// 真实场景：`tauri-plugin-updater` 用的是 **`log` crate** 的宏
    /// （不是 `tracing::error!`），经 `tracing-log` 桥接过来。
    /// 两条路径的 target 与消息记录方式可能不同，都得能拦住。
    #[test]
    fn updater_noise_from_log_crate_is_swallowed() {
        let out = capture(|| {
            // 与插件里的写法一致：`log::error!("...")`
            log::error!(
                target: "tauri_plugin_updater::updater",
                "update endpoint did not respond with a successful status code"
            );
        });

        assert!(
            !out.contains("ERROR"),
            "log crate 那条也不该再打成 ERROR：{out}"
        );
    }

    /// 对照组：确认 `log` crate 那条**确实**能发出事件——
    /// 否则上面那条测试就是「假通过」（没有 LogTracer 时 log 会静默丢弃）。
    #[test]
    fn log_crate_events_do_reach_the_formatter() {
        let out = capture(|| {
            log::error!(target: "some_other_crate", "来自 log crate 的普通错误");
        });

        assert!(
            out.contains("来自 log crate 的普通错误"),
            "log crate 的事件应当能到达 formatter（否则上一条测试无效）：{out}"
        );
    }

    #[test]
    fn other_updater_messages_are_untouched() {
        let out = capture(|| {
            tracing::error!(
                target: "tauri_plugin_updater::updater",
                "failed to check for updates: 网络超时"
            );
        });

        assert!(out.contains("ERROR"), "更新插件的其它错误不该被吞掉：{out}");
        assert!(out.contains("网络超时"));
    }
}
