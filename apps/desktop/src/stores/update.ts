import { defineStore } from "pinia";

/**
 * 全局更新卡片的状态。
 *
 * ## 为什么用 store 而不是组件间通信
 *
 * 更新卡片挂在 `App.vue`（全局唯一），但**三个地方**都可能叫醒它：
 *
 * - 启动时 Rust 侧的后台检查（`update-available` 事件）；
 * - 设置页手动点「检查更新」；
 * - 将来任何别的地方想提示更新。
 *
 * 用 store 就不用层层传 ref，也不用引事件总线——
 * 任何地方 `useUpdateStore().show(...)` 就行。
 */
export interface UpdatePrompt {
  /** 新版本号。 */
  latestVersion: string;
  /** 当前版本号。 */
  currentVersion: string;
  /** 更新说明（可能为空）。 */
  notes?: string;
}

export const useUpdateStore = defineStore("update", {
  state: () => ({
    /** 待提示的新版本号；为空表示当前没有待处理的更新。 */
    latestVersion: "",
    /** 当前版本号。 */
    currentVersion: "",
    /** 更新说明。 */
    notes: "",
    /** 下载 / 安装中——由卡片组件写入，避免弹窗被关掉。 */
    installing: false,
    /** 下载进度（0–100）；总量未知时为 null。 */
    percent: null as number | null,
    /** 已下载字节。 */
    downloaded: 0,
    /** 总字节（服务端未提供时为 0）。 */
    total: 0,
  }),

  getters: {
    /** 是否应该显示卡片。 */
    visible: (state) => state.latestVersion !== "",
  },

  actions: {
    /**
     * 叫醒更新卡片。
     *
     * 已经是同一个版本就不重复弹——启动检查与手动检查可能同时命中。
     */
    show(payload: UpdatePrompt): void {
      if (this.latestVersion === payload.latestVersion) return;

      this.latestVersion = payload.latestVersion;
      this.currentVersion = payload.currentVersion;
      this.notes = payload.notes ?? "";
      this.percent = null;
      this.downloaded = 0;
      this.total = 0;
    },

    /** 收起卡片。 */
    dismiss(): void {
      if (this.installing) return;
      this.latestVersion = "";
      this.notes = "";
    },

    /** 重置下载进度。 */
    resetProgress(): void {
      this.percent = null;
      this.downloaded = 0;
      this.total = 0;
    },
  },
});
