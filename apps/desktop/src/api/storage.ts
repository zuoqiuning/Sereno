import { commands } from "@/bindings";
import type { StorageUsage } from "@/types";

import { callCommand } from "./invoke";

/** 统计实例 / 缓存 / 日志的磁盘占用。 */
export function getStorageUsage(): Promise<StorageUsage> {
  return callCommand(() => commands.getStorageUsage());
}

/** 清空元数据缓存（实例、存档与日志不受影响）。 */
export async function clearStorageCache(): Promise<void> {
  await callCommand(() => commands.clearStorageCache());
}
