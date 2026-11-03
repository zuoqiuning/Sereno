//! Sereno 桌面壳（Tauri 适配层）。
//!
//! **职责边界**：本层只做四件事——
//! 1. 初始化 `AppState`（会话注册表与缓存）
//! 2. 注册 `dialog` 插件
//! 3. 构建 specta `Builder`
//! 4. 注册 `invoke_handler(builder.invoke_handler())`
//!
//! 业务逻辑一律在 `sereno-core`，本层只做类型转换与通道包装。

use tauri::Manager;

pub mod commands;
pub mod ipc;
pub mod logging;
pub mod state;

/// 生成的 `bindings.ts` 的绝对路径。
///
/// **不要依赖进程工作目录**：`tauri dev` 的 cwd 是 `apps/desktop/src-tauri`，
/// 而 `cargo run -- --export-bindings` 的 cwd 是仓库根；
/// 同一个相对路径会写到两个地方。用编译期常量拿绝对路径。
fn bindings_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts")
}

/// 应用入口。
pub fn run() {
    // 1. 日志最先初始化：后面任何 panic 都要能被记录下来
    logging::init();

    tracing::info!(version = sereno_core::APP_VERSION, "Sereno 启动");

    // 2. specta Builder：命令与事件的唯一注册点
    let builder = tauri_specta::Builder::<tauri::Wry>::new()
        .commands(commands::all_commands!())
        .events(ipc::events::all_events!());

    // 3. debug 构建导出 bindings.ts（release 不做文件写入）
    //    `export` 接收 &self，返回 Result<()>，因此不改变 builder 本身
    #[cfg(debug_assertions)]
    builder
        .export(specta_typescript::Typescript::default(), bindings_path())
        .expect("导出 bindings.ts 失败");

    // 4. 仅导出类型后退出，不创建窗口（CI / 脚手架刷新类型用）
    if std::env::args().any(|a| a == "--export-bindings") {
        tracing::info!("bindings.ts 已导出，进程退出");
        return;
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            // 状态初始化可能失败（状态目录不可写、元数据迁移失败）。
            // 放在 setup 里才能把错误变成可展示的启动失败，而不是在 manage() 处 panic。
            app.manage(state::AppState::new()?);
            builder.mount_events(app);

            // 兜底：前端异常或预加载超时后仍没调用 finish_startup 时，
            // 12 秒后强制显示主窗口，避免用户面对一个永远不出现的窗口。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(12)).await;
                if let Some(main) = handle.get_webview_window("main")
                    && !main.is_visible().unwrap_or(false)
                {
                    let _ = main.show();
                    let _ = main.set_focus();
                }
                if let Some(splash) = handle.get_webview_window("splash") {
                    let _ = splash.close();
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri 启动失败");

    // 退出钩子：正常退出时把日志缓冲刷干净，不丢最后几行
    app.run(|_handle, event| {
        if let tauri::RunEvent::Exit = event {
            logging::flush();
        }
    });
}
