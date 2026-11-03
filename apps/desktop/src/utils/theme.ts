/**
 * 主题切换：只改 `<html data-theme>`，颜色全部由 CSS 变量接管。
 */
export function applyTheme(theme: string): void {
  document.documentElement.dataset.theme = theme === "dark" ? "dark" : "light";
}
