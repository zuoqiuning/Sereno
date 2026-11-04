import { isTauri } from "@tauri-apps/api/core";

/**
 * 是否运行在 Tauri 运行时内。
 *
 * 浏览器里直接开 vite dev 预览时没有 Tauri 运行时，
 * 依赖窗口能力的代码（拖放、窗口控制）必须先问一句，否则会抛异常。
 */
export function inTauri(): boolean {
  return isTauri();
}
