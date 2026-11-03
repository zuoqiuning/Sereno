import { defineStore } from "pinia";

/**
 * 界面状态。
 *
 * **stores/ 只存 UI 状态和缓存。**
 * 实例列表、账户信息的「事实源」在 Rust（`instance.json` / keyring），
 * store 只是前端镜像，刷新时以 `api` 返回值为准，不在 store 里做持久化。
 */
export const useUiStore = defineStore("ui", {
  state: () => ({
    /** 当前主题：`light` / `dark` */
    theme: "light" as "light" | "dark",
    /** 侧边导航是否折叠（预留） */
    navCollapsed: false,
  }),

  actions: {
    /** 把主题写到 `<html data-theme>`，CSS 变量随之切换。 */
    applyTheme(): void {
      document.documentElement.setAttribute("data-theme", this.theme);
    },

    setTheme(theme: "light" | "dark"): void {
      this.theme = theme;
      this.applyTheme();
    },
  },
});
