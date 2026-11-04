import { isApiError } from "@/api/invoke";

/**
 * 把任意抛出物规整成人类可读的一句话。
 *
 * 覆盖顺序：
 * 1. `ApiError` / 后端 `LauncherError`（带 `message` 字段的对象）→ 直接用文案
 * 2. 标准 `Error` → `message`
 * 3. 字符串（Tauri 底层偶发直接抛字符串）→ 原样
 * 4. 其余 → JSON 序列化，绝不输出 `[object Object]`
 */
export function toMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message || String(value);
  if (typeof value === "string") return value;
  if (value !== null && typeof value === "object") {
    const message = (value as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}
