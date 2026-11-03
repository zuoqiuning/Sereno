// Windows release 构建不弹控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 加载 `.env`。
    //
    // **必须放在 `logging::init()` 之前**：`.env` 里可以覆盖 `RUST_LOG`，
    // 而日志级别正是在 `logging::init()` 里读的。
    //
    // 失败不致命——`.env` 是可选的（真实凭据通常来自系统密钥库）。
    let _ = dotenvy::dotenv();

    sereno_desktop_lib::run();
}
