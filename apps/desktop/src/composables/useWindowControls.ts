import { getCurrentWindow } from "@tauri-apps/api/window";
import { readonly, ref } from "vue";

/**
 * 缩放方向。
 *
 * `@tauri-apps/api/window` 内部声明了同名类型但**没有导出**，
 * 因此这里按其取值范围重新声明一份。
 * 它不属于 Rust 契约，所以不放进 `src/types/`（那里只 re-export bindings）。
 */
export type ResizeDirection =
  "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";

/**
 * 窗口控制 composable。
 *
 * ## 两条硬规则
 *
 * 1. **不要在模块顶层调用 `getCurrentWindow()`**。Tauri 注入的时机可能晚于
 *    模块求值，会让整个模块图加载失败，表现为整窗黑屏。这里用惰性单例解决。
 * 2. **不要在组件里散落 `getCurrentWindow()`**。窗口操作统一从这里走，
 *    否则权限、错误处理、状态同步会散成一片。
 */

/** 窗口句柄。惰性创建，避免模块求值期就触碰 Tauri 注入。 */
let handle: ReturnType<typeof getCurrentWindow> | null = null;

function windowHandle(): ReturnType<typeof getCurrentWindow> {
  if (handle === null) {
    handle = getCurrentWindow();
  }
  return handle;
}

/** 最大化状态。模块级单例，保证标题栏与缩放热区读到的是同一份。 */
const isMaximized = ref(false);

/** 是否已经挂过 `onResized` 监听，避免重复注册。 */
let resizeListenerBound = false;

export function useWindowControls() {
  /** 最小化。 */
  async function minimize(): Promise<void> {
    await windowHandle().minimize();
  }

  /** 切换最大化 / 还原。 */
  async function toggleMaximize(): Promise<void> {
    await windowHandle().toggleMaximize();
    await syncMaximized();
  }

  /** 关闭窗口。 */
  async function close(): Promise<void> {
    await windowHandle().close();
  }

  /** 从指定方向开始缩放拖拽。 */
  async function startResizeDragging(direction: ResizeDirection): Promise<void> {
    await windowHandle().startResizeDragging(direction);
  }

  /**
   * 读取并同步最大化状态。
   *
   * 用 `onResized` 而不是只在点击时读：系统快捷键（Win+↑）或贴边也会改变
   * 最大化状态，只在点击时读会让按钮图标不同步。
   */
  async function syncMaximized(): Promise<void> {
    try {
      isMaximized.value = await windowHandle().isMaximized();
    } catch {
      isMaximized.value = false;
    }
  }

  /** 绑定窗口尺寸变化监听（只绑一次）。 */
  function bindResizeListener(): void {
    if (resizeListenerBound) {
      return;
    }
    resizeListenerBound = true;
    void windowHandle().onResized(() => {
      void syncMaximized();
    });
  }

  return {
    isMaximized: readonly(isMaximized),
    minimize,
    toggleMaximize,
    close,
    startResizeDragging,
    syncMaximized,
    bindResizeListener,
  };
}
