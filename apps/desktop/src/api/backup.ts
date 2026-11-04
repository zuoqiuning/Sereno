import { commands } from "@/bindings";
import type { ContentSnapshot, WorldBackup } from "@/types";

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

/**
 * 创建内容快照（mods / config / 资源包 / 光影）。
 *
 * `reason` 会显示在列表里，便于用户分辨「手动备份」与「装 Mod 前自动备份」。
 */
export function createContentSnapshot(
  instanceId: string,
  reason = "manual",
  pinned = false,
): Promise<ContentSnapshot> {
  return callCommand(() => commands.createContentSnapshot(instanceId, reason, pinned));
}

/** 列出内容快照（新的在前）。 */
export function listContentSnapshots(instanceId: string): Promise<ContentSnapshot[]> {
  return callCommand(() => commands.listContentSnapshots(instanceId));
}

/** 回滚到某个内容快照：会替换当前 mods / config。 */
export async function restoreContentSnapshot(
  instanceId: string,
  snapshotId: string,
): Promise<void> {
  await callCommand(() => commands.restoreContentSnapshot(instanceId, snapshotId));
}

/** 删除某个内容快照。 */
export async function deleteContentSnapshot(
  instanceId: string,
  snapshotId: string,
): Promise<void> {
  await callCommand(() => commands.deleteContentSnapshot(instanceId, snapshotId));
}

/** 切换「保留」标记（pinned 不参与自动淘汰）。 */
export async function pinContentSnapshot(
  instanceId: string,
  snapshotId: string,
  pinned: boolean,
): Promise<void> {
  await callCommand(() => commands.pinContentSnapshot(instanceId, snapshotId, pinned));
}
