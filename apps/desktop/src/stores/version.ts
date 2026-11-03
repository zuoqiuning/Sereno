import { defineStore } from "pinia";
import { getVersionManifest } from "@/api/version";
import type { VersionManifest } from "@/types";

/**
 * 版本清单缓存。
 *
 * 与后端 `AppState.manifest_cache` 各管一层：
 * - 后端缓存 10 分钟，避免页面切换重新走网络；
 * - 前端 store 也缓存 10 分钟，避免"点一下页面就要等"。
 */
const TTL_MS = 10 * 60 * 1000;

export const useVersionStore = defineStore("version", {
  state: () => ({
    manifest: null as VersionManifest | null,
    fetchedAt: 0,
    loading: false,
  }),

  getters: {
    isFresh(state): boolean {
      return state.manifest !== null && Date.now() - state.fetchedAt < TTL_MS;
    },
  },

  actions: {
    /** 需要清单时调用：fresh 直接返回，否则拉取。 */
    async ensureLoaded(force = false): Promise<VersionManifest> {
      if (!force && this.isFresh && this.manifest) {
        return this.manifest;
      }
      if (this.loading) {
        // 简单等待：另一个调用正在拉
        while (this.loading) {
          await new Promise((r) => setTimeout(r, 100));
        }
        return this.manifest!;
      }
      this.loading = true;
      try {
        const m = await getVersionManifest();
        this.manifest = m;
        this.fetchedAt = Date.now();
        return m;
      } finally {
        this.loading = false;
      }
    },

    invalidate(): void {
      this.manifest = null;
      this.fetchedAt = 0;
    },
  },
});
