import { onBeforeUnmount, onMounted } from "vue";

import { events } from "@/bindings";
import { log } from "@/api/settings";
import type { UpdateAvailable } from "@/types";

/**
 * 启动时的更新提示接线。
 *
 * ## 谁在检查
 *
 * **Rust 侧**在启动画面（splash）阶段就发起了检查——
 * 见 `commands/update.rs` 的 `check_update_in_background`。
 * 它**不等结果**：界面照常显示，检测在后台跑。
 *
 * 所以前端**不再自己发请求**，只负责接结果：
 * 收到 `update-available` 事件就把全局更新卡片叫起来。
 *
 * ## 为什么不让前端自己查
 *
 * 前端发起的话要等 `onMounted` 之后才发得出去，
 * 而那时界面已经在显示了——检查再快也是「界面出来之后才有结果」。
 * 放到 Rust 的 setup 里，检查能在**窗口显示之前**就开始，
 * 结果回来时界面早就准备好了。
 */
export function useUpdateCheck(onAvailable: (payload: UpdateAvailable) => void): void {
  let unlisten: (() => void) | null = null;

  onMounted(() => {
    void events.updateAvailable
      .listen((event) => {
        onAvailable(event.payload);
      })
      .then((stop) => {
        unlisten = stop;
      })
      .catch((err) => {
        // 订阅失败不该影响启动——只是收不到更新提示而已。
        void log.warn("订阅更新事件失败", {
          component: "useUpdateCheck",
          action: "listen",
          error: String(err),
        });
      });
  });

  onBeforeUnmount(() => {
    unlisten?.();
    unlisten = null;
  });
}
