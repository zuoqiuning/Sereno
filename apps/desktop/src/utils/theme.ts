/**
 * 主题切换：只改 `<html data-theme>`，颜色全部由 CSS 变量接管。
 *
 * ## 为什么要给切换加过渡
 *
 * 不加过渡时，浅色 → 深色是**整屏瞬间反色**，像闪了一下。
 * 这里在切换的 280ms 内给根节点挂一个 `.theme-transition`，
 * 让背景 / 边框 / 文字颜色平滑过渡，结束后立刻摘掉——
 * 否则 `!important` 会把之后所有 hover 都拖慢到 280ms。
 */
let timer: number | null = null;

export function applyTheme(theme: string): void {
  const next = theme === "dark" ? "dark" : "light";
  const root = document.documentElement;
  const current = root.dataset.theme;

  // 首次应用（启动时）不需要过渡，否则首屏会先「淡入一次颜色」
  if (current && current !== next) {
    root.classList.add("theme-transition");
    if (timer) window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      root.classList.remove("theme-transition");
      timer = null;
    }, 320);
  }

  root.dataset.theme = next;
}
