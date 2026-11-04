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

    // 3. `--export-bindings`：只导出类型后退出，不创建窗口（CI / 刷新类型用）。
    //
    //    导出本身只在 debug 构建里做（写文件不该发生在 release）。
    //    release 下如果传了这个参数，必须**明确失败**而不是静默成功——
    //    否则 CI 会拿到「退出码 0 但 bindings 没更新」的假绿。
    let export_bindings = std::env::args().any(|arg| arg == "--export-bindings");

    #[cfg(debug_assertions)]
    {
        builder
            .export(specta_typescript::Typescript::default(), bindings_path())
            .expect("导出 bindings.ts 失败");

        if export_bindings {
            tracing::info!("bindings.ts 已导出，进程退出");
            return;
        }
    }

    #[cfg(not(debug_assertions))]
    if export_bindings {
        eprintln!("[sereno] --export-bindings 只在 debug 构建可用（release 不写文件）");
        std::process::exit(2);
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // 更新通道：updater 负责拉清单、验 minisign 签名、下载并安装。
        // 安装完成后重启应用用的是 Tauri 核心的 `AppHandle::request_restart()`，
        // 因此**不需要** tauri-plugin-process（少一个插件就少一份权限面）。
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            // 状态初始化可能失败（状态目录不可写、元数据迁移失败）。
            // 放在 setup 里才能把错误变成可展示的启动失败，而不是在 manage() 处 panic。
            app.manage(state::AppState::new()?);
            builder.mount_events(app);

            // **启动画面阶段就开始检查更新**。
            //
            // 不等结果：界面照常显示，检测在后台跑，
            // 有新版了再通过 `UpdateAvailable` 事件让前端弹一张全局卡片。
            // 这样「打开软件就能看到有新版」，又不用为一个网络请求拖慢启动。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                commands::update::check_update_in_background(handle.clone()).await;
            });

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
