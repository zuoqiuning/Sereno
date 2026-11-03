import { defineStore } from "pinia";
import type { DownloadSessionSummary, DownloadSnapshot } from "@/types";

export interface TaskItem {
  id: string; // instanceId
  name: string; // instanceName
  status: "running" | "done" | "error";
  errorMsg: string | null;
  summary: DownloadSessionSummary | null;
  currentFile: DownloadSnapshot | null;
  speedBps: number;
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
  },

  actions: {
    start(instanceId: string, instanceName: string) {
      this.tasks[instanceId] = {
        id: instanceId,
        name: instanceName,
        status: "running",
        errorMsg: null,
        summary: null,
        currentFile: null,
        speedBps: 0,
        _lastBytes: 0,
        _lastTime: 0,
      };
      this.activeId = instanceId;
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
        const instant = db / dt;
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
      if (task) {
        task.status = "done";
        task.errorMsg = null;
      }
    },

    setError(instanceId: string, msg: string) {
      const task = this.tasks[instanceId];
      if (task) {
        task.status = "error";
        task.errorMsg = msg;
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
