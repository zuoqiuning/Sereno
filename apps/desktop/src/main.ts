import { createApp } from "vue";
import { createPinia } from "pinia";
import { getCurrentWindow, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";

import App from "./App.vue";
import { warmAccountSession } from "./api/account";
import { searchResources } from "./api/resource";
import { getSettings, finishStartup, log, saveWindowState } from "./api/settings";
import { router } from "./router";
import { useVersionStore } from "./stores/version";
import { applyTheme } from "./utils/theme";

/*
 * 昭源环方（Chiron GoRound TC，可变字重 200–900）。
 * Fontsource 已做好子集化与切片，浏览器只会加载页面实际用到的字符切片。
 */
import { confirmUpdateSuccess } from "@/api/update";
import "@fontsource-variable/chiron-goround-tc";

import "./styles/main.css";
import "./styles/theme.css";

/** 字体族名（可变字重版本带 "Variable" 后缀）。 */
const FONT_FAMILY = '"Chiron GoRound TC Variable"';

/** 预加载超时（毫秒）。超过就放行，宁可先回退字体也不卡住启动。 */
const FONT_LOAD_TIMEOUT = 2000;

/**
 * 强制加载并解析字体，消除首屏 FOUT/FOIT。
 *
 * `document.fonts.load()` 返回的 Promise 会等到字体真正可用才 resolve。
 * 这里加一层超时兜底：字体文件缺失或加载异常时不能让窗口一直不显示。
 */
async function preloadFonts(): Promise<void> {
  const weights = ["400", "500", "600", "700"];
  const loads = weights.map((weight) => document.fonts.load(`${weight} 16px ${FONT_FAMILY}`));

  const timeout = new Promise<void>((resolve) => {
    setTimeout(resolve, FONT_LOAD_TIMEOUT);
  });

  await Promise.race([Promise.all(loads), timeout]);
}

/**
 * 全局错误边界。
 *
 * Vue 组件里未捕获的异常会让整棵子树变成空白——**没有全局兜底时，
 * 用户看到的是白屏，而你连一行日志都拿不到**。
 *
 * 这里把三类异常统一送进后端的 `tracing` 管线（`target = sereno_ui`），
 * 与后端日志在同一份文件里，可以按时间顺序还原现场。
 */
function installErrorHandlers(app: ReturnType<typeof createApp>): void {
  // 1. Vue 组件内的异常
  app.config.errorHandler = (error, _instance, info) => {
    const message = error instanceof Error ? error.message : String(error);
    void log.error(`未捕获的组件异常: ${message}`, { component: "vue", action: info });
  };

  // 2. Vue 的 warn（不一定是错误，但值得记录）
  app.config.warnHandler = (message, _instance, trace) => {
    void log.warn(`Vue 警告: ${message}`, { component: "vue", action: trace });
  };

  // 3. Promise 未处理的 rejection
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    const message = reason instanceof Error ? reason.message : String(reason);
    void log.error(`未处理的 Promise 拒绝: ${message}`, {
      component: "window",
      action: "unhandledrejection",
    });
  });

  // 4. 兜底：捕获监听不到的资源加载错误
  window.addEventListener("error", (event) => {
    const message = event.message || String(event.error ?? "");
    void log.error(`未捕获的全局错误: ${message}`, {
      component: "window",
      action: "error",
    });
  });
}

/** 启动预加载的兜底上限：网络再慢也不能让启动画面无限等待。 */
const PRELOAD_TIMEOUT = 7000;

/**
 * 启动画面期间的预加载。
 *
 * 这里只做「用户很快会用到、又必须走网络/扫描」的事：
 * - 版本清单（下载页/新建实例都要）；
 * - MOD 分类默认第一页（下载页最常用的资源入口）。
 *
 * **不再预热全部分类**：每个聚合分类第一页都会向 CurseForge 发 2 个请求，
 * 四个分类就是 8 个；启动阶段并发时最容易打满共享 Key 配额。
 * 资源包 / 光影等分类改为用户点开时再请求，页面本身仍会缓存。
 *
 * **加载器版本不预热**：不同游戏版本的加载器列表不同，预热命中率低，
 * 只在用户真正打开加载器弹窗时按版本请求。
 *
 * 所有失败都被 `allSettled` 吞掉，或在超时后放行，绝不影响主界面出现。
 */
async function preloadStartupData(pinia: ReturnType<typeof createPinia>): Promise<void> {
  const warmSearches = (["mod"] as const).map((kind) =>
    searchResources({
      query: "",
      source: "aggregate",
      kind,
      mcVersion: "",
      loader: "",
      sort: "downloads",
      page: 1,
      pageSize: 20,
    }),
  );

  const tasks = [useVersionStore(pinia).ensureLoaded(), ...warmSearches];
  await Promise.race([
    Promise.allSettled(tasks).then(() => undefined),
    new Promise<void>((resolve) => setTimeout(resolve, PRELOAD_TIMEOUT)),
  ]);

  // 微软账户：提前刷新令牌，让「点启动」不用再等三次网络往返。
  // 不加入上面的 Promise.race：授权链路偶尔要几秒，不该拖慢启动画面。
  void warmAccountSession();
}

/**
 * 显示主窗口。
 *
 * 正常路径由 Rust 一并关闭启动画面；浏览器预览没有窗口 API，
 * 因此失败时静默忽略（页面本身已经渲染好了）。
 */
async function showMainWindow(): Promise<void> {
  try {
    await finishStartup();
  } catch {
    try {
      await getCurrentWindow().show();
    } catch {
      // 浏览器预览（vite dev 直接开网页）下没有窗口 API，忽略即可
    }
  }
}

/**
 * 恢复上次的窗口尺寸与位置。
 *
 * 之前 `AppSettings.window` 字段定义了却没人用，用户每次启动都要重新拖一次窗口。
 * 这里把**逻辑像素**（物理像素 ÷ 缩放比）写进设置，跨 DPI 显示器时也不会失真。
 */
async function restoreWindowState(): Promise<void> {
  try {
    const settings = await getSettings();
    const { width, height, x, y } = settings.window;
    if (!width || !height) return;

    const win = getCurrentWindow();
    await win.setSize(new LogicalSize(width, height));
    if (x !== null && x !== undefined && y !== null && y !== undefined) {
      await win.setPosition(new LogicalPosition(x, y));
    }
  } catch {
    // 浏览器预览 / 首次启动：没有窗口 API 或没有历史值，保持默认尺寸
  }
}

/** 窗口尺寸变化后写回设置的防抖延迟：拖拽过程中不要频繁写盘。 */
const WINDOW_SAVE_DELAY = 600;

/** 监听窗口尺寸/位置变化并持久化。 */
function trackWindowState(): void {
  let timer: number | undefined;
  let last = "";

  const schedule = (): void => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      void (async () => {
        try {
          const win = getCurrentWindow();
          const size = await win.innerSize();
          // 最小化 / 隐藏时尺寸可能退化为 0，不要覆盖有效值
          if (size.width < 320 || size.height < 240) return;

          const scale = await win.scaleFactor();
          const logicalWidth = Math.round(size.width / scale);
          const logicalHeight = Math.round(size.height / scale);
          const position = await win.innerPosition();
          const logicalX = Math.round(position.x / scale);
          const logicalY = Math.round(position.y / scale);

          const signature = `${logicalWidth}x${logicalHeight}@${logicalX},${logicalY}`;
          if (signature === last) return;
          last = signature;

          await saveWindowState(logicalWidth, logicalHeight, logicalX, logicalY);
        } catch {
          // 浏览器预览下没有窗口 API，静默忽略
        }
      })();
    }, WINDOW_SAVE_DELAY);
  };

  try {
    const win = getCurrentWindow();
    void win.onResized(schedule);
    void win.onMoved(schedule);
  } catch {
    // 同上
  }
}

/**
 * 启动流程。
 *
 * 主窗口在 `tauri.conf.json` 里 `visible: false`，启动画面是独立的透明窗口。
 * 这里等「字体就绪 → Vue 挂载 → 数据预加载」完成后才显示主窗口，
 * 再用 `app-visible` 触发整体淡入上浮动画。
 */
async function bootstrap(): Promise<void> {
  await preloadFonts();

  const app = createApp(App);
  const pinia = createPinia();
  app.use(pinia);
  app.use(router);

  installErrorHandlers(app);

  app.mount("#app");

  // 增量更新：启动成功确认（极其重要）。
  // 告诉替换脚本「新版界面已渲染成功」，它才会删除备份文件、
  // 解除安全模式——没有这一步，替换脚本会在 30 秒后自动回滚。
  confirmUpdateSuccess().catch((error) => {
    console.error("[delta-updater] 启动确认失败", error);
  });

  // 主题与窗口尺寸在首屏挂载后立刻应用；读取失败就保持默认值。
  void getSettings()
    .then((settings) => applyTheme(settings.theme))
    .catch(() => undefined);
  void restoreWindowState();
  trackWindowState();

  await preloadStartupData(pinia);
  await showMainWindow();

  // 窗口显示后再开启入场动画；rAF 在隐藏窗口里可能被节流，补一个定时器兜底。
  const reveal = () => document.documentElement.classList.add("app-visible");
  requestAnimationFrame(reveal);
  setTimeout(reveal, 120);
}

void bootstrap();
