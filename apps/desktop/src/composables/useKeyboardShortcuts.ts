import { onBeforeUnmount, onMounted } from "vue";
import { useRouter } from "vue-router";

/** 点击命令面板的打开事件名（避免为了一个布尔值引入状态库）。 */
export const PALETTE_EVENT = "sereno:command-palette";

/** 主导航顺序，与 `nav.ts` / `useNavDirection.ts` 保持一致。 */
const ROUTES = ["/", "/instances", "/downloads", "/tasks", "/settings", "/more"];

/**
 * 全局键盘快捷键。
 *
 * 桌面应用有一大半的「快」来自键盘：用户每天切几十次页面，
 * 全靠鼠标点导航是低效的。这里只做三件最值钱的事：
 *
 * - `Ctrl/Cmd + 1~6`：直达主导航（与 PCL 的版本/下载/设置切换同理）；
 * - `Ctrl/Cmd + K`：命令面板，一条路径覆盖「切页面 + 跳设置项」；
 * - `Ctrl/Cmd + ,`：打开设置（macOS 惯例）。
 *
 * 输入框里不抢键：只有在非输入状态下才响应数字与逗号，
 * 但 `Ctrl+K` 在输入框里也生效——那正是用户想「跳走」的时候。
 */
export function useKeyboardShortcuts(): void {
  const router = useRouter();

  function isTyping(target: EventTarget | null): boolean {
    const element = target as HTMLElement | null;
    if (!element) return false;
    const tag = element.tagName;
    return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || element.isContentEditable;
  }

  function onKeydown(event: KeyboardEvent): void {
    const mod = event.ctrlKey || event.metaKey;
    if (!mod) return;

    const key = event.key.toLowerCase();

    if (key === "k") {
      event.preventDefault();
      window.dispatchEvent(new CustomEvent(PALETTE_EVENT));
      return;
    }

    if (isTyping(event.target)) return;

    if (key === ",") {
      event.preventDefault();
      void router.push("/settings");
      return;
    }

    const index = Number.parseInt(event.key, 10);
    if (index >= 1 && index <= ROUTES.length) {
      event.preventDefault();
      void router.push(ROUTES[index - 1]);
    }
  }

  onMounted(() => window.addEventListener("keydown", onKeydown));
  onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown));
}
