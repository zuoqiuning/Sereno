import { commands } from "@/bindings";
import type { WorldBackup } from "@/types";

import { callCommand } from "./invoke";

/** 创建一次世界存档备份。 */
export function createWorldBackup(instanceId: string): Promise<WorldBackup> {
  return callCommand(() => commands.createWorldBackup(instanceId));
}

/** 列出该实例的存档备份（新的在前）。 */
export function listWorldBackups(instanceId: string): Promise<WorldBackup[]> {
  return callCommand(() => commands.listWorldBackups(instanceId));
}

/** 用指定备份覆盖当前存档。 */
export async function restoreWorldBackup(instanceId: string, name: string): Promise<void> {
  await callCommand(() => commands.restoreWorldBackup(instanceId, name));
}
