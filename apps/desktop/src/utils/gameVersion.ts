/**
 * 把具体游戏版本归到「主次版本」分组。
 *
 * - `1.21` / `1.21.1` / `1.21.2` → `1.21`
 * - `26.1` / `26.2` / `26.3` → 保持独立（本身就只有两段）
 * - 快照或无法解析的字符串原样返回
 */
export function gameVersionGroup(value: string): string {
  const [major = "", minor = ""] = value.split(".");
  return /^\d+$/.test(major) && /^\d+$/.test(minor) ? `${major}.${minor}` : value;
}
