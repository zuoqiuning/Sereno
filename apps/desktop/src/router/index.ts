import { createRouter, createWebHashHistory } from "vue-router";

import HomeView from "@/views/HomeView.vue";

/*
 * 路由表：`/`、`/instances`、`/downloads`、`/accounts`、`/settings`。
 *
 * 用 `createWebHashHistory`：打包后没有服务端 rewrite，hash 路由刷新不会 404。
 *
 * 新增页面 = views/ 加文件 → 这里加一条 → 若为主功能再往
 * components/layout/nav.ts 的 primaryItems 加一项。**不修改已有页面。**
 */
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "home", component: HomeView, meta: { title: "启动" } },
    {
      path: "/instances",
      name: "instances",
      component: () => import("@/views/InstancesView.vue"),
      meta: { title: "实例" },
    },
    {
      path: "/downloads",
      name: "downloads",
      component: () => import("@/views/DownloadsView.vue"),
      meta: { title: "下载" },
    },
    {
      path: "/tasks",
      name: "tasks",
      component: () => import("@/views/TasksView.vue"),
      meta: { title: "任务" },
    },

    {
      path: "/settings",
      name: "settings",
      component: () => import("@/views/SettingsView.vue"),
      meta: { title: "设置" },
    },
    // 404 兜底：用户手输 #/foobar 时回到主页，而不是白屏
    {
      path: "/:pathMatch(.*)*",
      redirect: "/",
    },
  ],
});
