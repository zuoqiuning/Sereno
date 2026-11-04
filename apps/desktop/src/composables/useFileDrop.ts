import { getCurrentWebview } from "@tauri-apps/api/webview";
import { onBeforeUnmount, onMounted, ref } from "vue";

import { inTauri } from "@/api/tauri";

/**
 * 全局文件拖放。
 *
 * PCL 与 HMCL 都支持「把 .jar / .zip / .mrpack 直接拖进窗口」。
 * 这在 Tauri 里是**内置**的 webview 事件（`onDragDropEvent`），
 * 不需要额外 capability —— 它不是 IPC 命令，走的是窗口级事件通道。
 *
 * 注意 `dragDropEnabled` 默认为 true；如果前端要用 HTML5 原生拖放，
 * 才需要在 `tauri.conf.json` 里关掉它。我们不用原生拖放，保持开启。
 */
/** 当前是否有文件悬停在窗口上（用于显示遮罩提示）。 */
export const dragging = ref(false);

/** 最近一次拖入的文件路径；空数组表示没有待处理的文件。 */
export const droppedPaths = ref<string[]>([]);

let unlisten: (() => void) | null = null;

/** 清空待处理的文件（用户处理完或取消后调用）。 */
export function clearDroppedFiles(): void {
  droppedPaths.value = [];
}

/**
 * 在根组件调用一次即可。
 *
 * 浏览器预览（vite dev 直接开网页）下没有 Tauri 运行时，这里静默跳过。
 */
export function useFileDrop(): void {
  onMounted(() => {
    if (!inTauri()) return;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          dragging.value = true;
        } else if (payload.type === "drop") {
          dragging.value = false;
          droppedPaths.value = payload.paths ?? [];
        } else {
          dragging.value = false;
        }
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        // 运行时不可用时保持静默：拖放只是增强，不是必需能力
      });
  });

  onBeforeUnmount(() => {
    unlisten?.();
    unlisten = null;
  });
}
