import { createRouter, createWebHashHistory } from "vue-router";

import { updateNavDirection } from "@/composables/useNavDirection";
import HomeView from "@/views/HomeView.vue";

/*
 * 路由表：`/`、`/instances`、`/multiplayer`、`/downloads`、`/tasks`、`/settings`、`/more`。
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
      path: "/multiplayer",
      name: "multiplayer",
      component: () => import("@/views/MultiplayerView.vue"),
      meta: { title: "联机" },
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
    {
      path: "/more",
      name: "more",
      component: () => import("@/views/MoreView.vue"),
      meta: { title: "更多" },
    },
    // 404 兜底：用户手输 #/foobar 时回到主页，而不是白屏
    {
      path: "/:pathMatch(.*)*",
      redirect: "/",
    },
  ],
});

/*
 * 记录切换方向：让页面过渡知道「从哪来」。
 *
 * 必须在 **beforeEach** 里更新：过渡名在渲染时就被求值，
 * afterEach 已经晚于新页面开始渲染。
 */
router.beforeEach((to, from) => {
  updateNavDirection(from.path, to.path);
});
