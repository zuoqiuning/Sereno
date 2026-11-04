import { defineStore } from "pinia";

export type ToastTone = "success" | "error" | "warning" | "info";

export interface ToastItem {
  id: number;
  title: string;
  message: string;
  tone: ToastTone;
}

/** 定时器放在模块级 Map，避免被 Pinia 的响应式代理处理。 */
const timers = new Map<number, number>();
let nextId = 1;

export const useToastStore = defineStore("toast", {
  state: () => ({
    items: [] as ToastItem[],
  }),

  actions: {
    push(options: {
      title: string;
      message?: string;
      tone?: ToastTone;
      duration?: number;
    }): number {
      const id = nextId++;
      this.items.push({
        id,
        title: options.title,
        message: options.message ?? "",
        tone: options.tone ?? "info",
      });
      this.schedule(id, options.duration ?? 3000);
      return id;
    },

    success(title: string, message = "", duration = 2600): number {
      return this.push({ title, message, tone: "success", duration });
    },

    error(title: string, message = "", duration = 5200): number {
      return this.push({ title, message, tone: "error", duration });
    },

    warning(title: string, message = "", duration = 4000): number {
      return this.push({ title, message, tone: "warning", duration });
    },

    info(title: string, message = "", duration = 3000): number {
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
     * 移除一条通知。
     *
     * 直接从列表移除即可：离场动画由 `TransitionGroup` 负责，
     * 它会让还在排队的卡片平滑补位，不会出现空位或瞬间跳位。
     */
    dismiss(id: number): void {
      const timer = timers.get(id);
      if (timer) {
        window.clearTimeout(timer);
        timers.delete(id);
      }

      const index = this.items.findIndex((entry) => entry.id === id);
      if (index >= 0) this.items.splice(index, 1);
    },

    dismissAll(): void {
      for (const item of [...this.items]) {
        this.dismiss(item.id);
      }
    },
  },
});
