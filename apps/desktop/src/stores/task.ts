import { defineStore } from "pinia";
import type { DownloadSessionSummary, DownloadSnapshot } from "@/types";
import { useToastStore } from "./toast";

export interface TaskItem {
  id: string; // instanceId
  name: string; // instanceName
  status: "running" | "done" | "error" | "cancelled";
  errorMsg: string | null;
  summary: DownloadSessionSummary | null;
  currentFile: DownloadSnapshot | null;
  speedBps: number;
  /** 后端是否支持取消这个任务。 */
  cancellable: boolean;
  /** 失败后「重试」要做什么：由发起的页面注入，任务页不关心业务细节。 */
  retry?: (() => void | Promise<void>) | null;
  _lastBytes: number;
  _lastTime: number;
}

export const useTaskStore = defineStore("task", {
  state: () => ({
    tasks: {} as Record<string, TaskItem>,
    activeId: null as string | null,
  }),

  getters: {
    activeTask(state): TaskItem | null {
      if (!state.activeId) return null;
      return state.tasks[state.activeId] || null;
    },
    hasTasks(state): boolean {
      return Object.keys(state.tasks).length > 0;
    },
    runningCount(state): number {
      return Object.values(state.tasks).filter((task) => task.status === "running").length;
    },
  },

  actions: {
    start(instanceId: string, instanceName: string, cancellable = false) {
      this.tasks[instanceId] = {
        id: instanceId,
        name: instanceName,
        status: "running",
        errorMsg: null,
        summary: null,
        currentFile: null,
        speedBps: 0,
        cancellable,
        retry: null,
        _lastBytes: 0,
        _lastTime: 0,
      };
      this.activeId = instanceId;
    },

    /** 登记「失败后重试」的动作（任务页据此显示重试按钮）。 */
    setRetry(instanceId: string, retry: () => void | Promise<void>) {
      const task = this.tasks[instanceId];
      if (task) task.retry = retry;
    },

    updateProgress(
      instanceId: string,
      summary: DownloadSessionSummary | null,
      file: DownloadSnapshot | null,
    ) {
      const task = this.tasks[instanceId];
      if (!task) return;

      if (summary) task.summary = summary;
      if (file) task.currentFile = file;

      const current = task.summary;
      if (!current) return;

      const now = Date.now();
      if (task._lastTime === 0) {
        task._lastBytes = current.downloadedBytes;
        task._lastTime = now;
        return;
      }
      const dt = (now - task._lastTime) / 1000;
      if (dt >= 0.4) {
        const db = current.downloadedBytes - task._lastBytes;
        // 防御：后端重试/分片重置可能给出负数或极端瞬时值，
        // 平滑前先夹紧——负数截 0，上限 1 GB/s（现实中不可能更快）
        const instant = Math.min(Math.max(db / dt, 0), 1024 * 1024 * 1024);
        task.speedBps = task.speedBps > 0 ? task.speedBps * 0.3 + instant * 0.7 : instant;
        task._lastBytes = current.downloadedBytes;
        task._lastTime = now;
      }
    },

    setStage3Running(instanceId: string) {
      const task = this.tasks[instanceId];
      if (task) {
        task.currentFile = null;
        task.speedBps = 0; // 重置速度，避免下载残留
        if (task.summary) task.summary.stage = 3;
      }
    },

    finish(instanceId: string) {
      const task = this.tasks[instanceId];
      if (task && task.status !== "cancelled") {
        task.status = "done";
        task.errorMsg = null;
        // 完成通知放在 store 里：用户可能已经切到别的页面，
        // 只由发起的页面弹提示会漏掉。
        useToastStore().success("任务完成", task.name);
      }
    },

    setError(instanceId: string, msg: string) {
      const task = this.tasks[instanceId];
      // 用户主动取消后，后端会回 `Cancelled`；这不是错误，保持取消态。
      if (task && task.status !== "cancelled") {
        task.status = "error";
        task.errorMsg = msg;
        useToastStore().error("任务失败", `${task.name}：${msg}`);
      }
    },

    /** 用户主动取消：先落状态，避免随后到达的 `Cancelled` 被当成失败。 */
    markCancelled(instanceId: string) {
      const task = this.tasks[instanceId];
      if (task) {
        task.status = "cancelled";
        task.errorMsg = null;
        task.speedBps = 0;
      }
    },

    setActive(id: string) {
      this.activeId = id;
    },

    clearFinished() {
      for (const id in this.tasks) {
        if (this.tasks[id].status !== "running") {
          delete this.tasks[id];
          if (this.activeId === id) this.activeId = null;
        }
      }
      // 如果还有任务，自动切到第一个
      if (!this.activeId) {
        const keys = Object.keys(this.tasks);
        if (keys.length > 0) this.activeId = keys[0];
      }
    },

    clearAll() {
      this.tasks = {};
      this.activeId = null;
    },
  },
});
