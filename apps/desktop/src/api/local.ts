import { commands } from "@/bindings";
import type { LocalResource, LocalResourceKind, LocalResourceScan } from "@/types";

import { callCommand } from "./invoke";

/** 实例本地资源管理。 */

export function listLocalResources(
  instanceId: string,
  kind: LocalResourceKind,
): Promise<LocalResource[]> {
  return callCommand(() => commands.listLocalResources(instanceId, kind));
}

export function setLocalResourceEnabled(
  instanceId: string,
  kind: LocalResourceKind,
  fileName: string,
  enabled: boolean,
): Promise<void> {
  return callCommand(() =>
    commands.setLocalResourceEnabled(instanceId, kind, fileName, enabled),
  ).then(() => undefined);
}

/** 批量启用 / 禁用本地资源，返回实际处理数量。 */
export function setLocalResourcesEnabled(
  instanceId: string,
  kind: LocalResourceKind,
  fileNames: string[],
  enabled: boolean,
): Promise<number> {
  return callCommand(() =>
    commands.setLocalResourcesEnabled(instanceId, kind, fileNames, enabled),
  );
}

/** 删除本地资源（单个或批量），返回实际删除数量。 */
export function removeLocalResources(
  instanceId: string,
  kind: LocalResourceKind,
  fileNames: string[],
): Promise<number> {
  return callCommand(() => commands.removeLocalResources(instanceId, kind, fileNames));
}

export function openLocalResourceFolder(
  instanceId: string,
  kind: LocalResourceKind,
): Promise<void> {
  return callCommand(() => commands.openLocalResourceFolder(instanceId, kind)).then(
    () => undefined,
  );
}

export function importLocalResources(instanceId: string, kind: LocalResourceKind): Promise<number> {
  return callCommand(() => commands.importLocalResources(instanceId, kind));
}

/** 按绝对路径导入本地资源（拖拽导入用），返回导入数量。 */
export function importLocalResourcesFromPaths(
  instanceId: string,
  kind: LocalResourceKind,
  paths: string[],
): Promise<number> {
  return callCommand(() => commands.importLocalResourcesFromPaths(instanceId, kind, paths));
}

export function checkLocalResourceUpdates(
  instanceId: string,
  kind: LocalResourceKind,
): Promise<LocalResourceScan> {
  return callCommand(() => commands.checkLocalResourceUpdates(instanceId, kind));
}

export async function updateLocalResource(
  instanceId: string,
  kind: LocalResourceKind,
  oldFileName: string,
  newFileName: string,
  fileUrl: string,
  sha1: string | null,
): Promise<void> {
  await callCommand(() =>
    commands.updateLocalResource(instanceId, kind, oldFileName, newFileName, fileUrl, sha1),
  );
}
