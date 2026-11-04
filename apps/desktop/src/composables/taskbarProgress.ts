/**
 * 系统任务栏进度（路线图 #16）。
 *
 * Windows 任务栏会出现与下载进度一致的绿色进度条——用户切到任何页面
 * 都能瞥见任务是否完成。聚合逻辑在 task store，本模块只负责把数字
 * 写到窗口上。
 *
 * `getCurrentWindow()` 惰性创建（与 useWindowControls 同一条硬规则：
 * 不能在模块求值期触碰 Tauri 注入）；失败静默——任务栏进度纯属锦上添花，
 * 任何错误（权限缺失、平台不支持）都不该影响任务本身。
 */
import { getCurrentWindow, ProgressBarStatus } from "@tauri-apps/api/window";

let handle: ReturnType<typeof getCurrentWindow> | null = null;

function windowHandle(): ReturnType<typeof getCurrentWindow> {
  if (handle === null) {
    handle = getCurrentWindow();
  }
  return handle;
}

/**
 * 更新任务栏进度。
 *
 * @param percent 0–100 的整数；`null` = 隐藏（没有进行中的任务）。
 */
export async function setTaskbarProgress(percent: number | null): Promise<void> {
  try {
    if (percent === null) {
      await windowHandle().setProgressBar({ status: ProgressBarStatus.None });
    } else {
      const clamped = Math.max(0, Math.min(100, Math.round(percent)));
      await windowHandle().setProgressBar({
        status: ProgressBarStatus.Normal,
        progress: clamped,
      });
    }
  } catch {
    // 静默：任务栏进度失败不影响任何业务
  }
}
