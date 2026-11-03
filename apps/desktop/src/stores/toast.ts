import { defineStore } from "pinia";

export type ToastTone = "success" | "error" | "warning" | "info";

export interface ToastItem {
  id: number;
  title: string;
  message: string;
  tone: ToastTone;
  /** 退场动画进行中：保留 DOM，避免卡片凭空消失。 */
  leaving: boolean;
}

/** 定时器放在模块级 Map，避免被 Pinia 的响应式代理处理。 */
const timers = new Map<number, number>();
let nextId = 1;

export const useToastStore = defineStore("toast", {
  state: () => ({
    items: [] as ToastItem[],
  }),

  actions: {
    push(
      options: { title: string; message?: string; tone?: ToastTone; duration?: number },
    ): number {
      const id = nextId++;
      const item: ToastItem = {
        id,
        title: options.title,
        message: options.message ?? "",
        tone: options.tone ?? "info",
        leaving: false,
      };
      this.items.push(item);
      this.schedule(id, options.duration ?? 4200);
      return id;
    },

    success(title: string, message = "", duration = 3600): number {
      return this.push({ title, message, tone: "success", duration });
    },

    error(title: string, message = "", duration = 6000): number {
      return this.push({ title, message, tone: "error", duration });
    },

    warning(title: string, message = "", duration = 5000): number {
      return this.push({ title, message, tone: "warning", duration });
    },

    info(title: string, message = "", duration = 4200): number {
      return this.push({ title, message, tone: "info", duration });
    },

    schedule(id: number, duration: number): void {
      const existing = timers.get(id);
      if (existing) window.clearTimeout(existing);

      const timer = window.setTimeout(() => {
        this.dismiss(id);
      }, duration);
      timers.set(id, timer);
    },

    /**
     * 标记退场，动画结束后再移除。
     *
     * 关键：前一张卡片还在动画中时，后面的卡片通过 TransitionGroup 的
     * `move` 过渡平滑上移，而不是瞬间跳位或凭空消失。
     */
    dismiss(id: number): void {
      const timer = timers.get(id);
      if (timer) {
        window.clearTimeout(timer);
        timers.delete(id);
      }

      const item = this.items.find((entry) => entry.id === id);
      if (!item || item.leaving) return;
      item.leaving = true;

      window.setTimeout(() => {
        const index = this.items.findIndex((entry) => entry.id === id);
        if (index >= 0) this.items.splice(index, 1);
      }, 240);
    },

    dismissAll(): void {
      for (const item of [...this.items]) {
        this.dismiss(item.id);
      }
    },
  },
});
