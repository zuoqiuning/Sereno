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
