import { Channel } from "@tauri-apps/api/core";
import { commands } from "@/bindings";
import type { DownloadProgress, ModpackInfo } from "@/types";

import { callCommand } from "./invoke";

/**
 * 整合包域接口。
 */

/** 整合包概览：游戏版本、加载器与附加文件数。 */
export function getModpackInfo(projectId: string, versionId: string): Promise<ModpackInfo> {
  return callCommand(() => commands.getModpackInfo(projectId, versionId));
}

/**
 * 安装整合包内容：原版 → 加载器 → MOD → 覆盖文件。
 *
 * 实例需要先由前端创建（`createInstance`），后端只负责把内容装进去。
 */
export async function installModpack(
  instanceId: string,
  projectId: string,
  versionId: string,
  onProgress: (progress: DownloadProgress) => void,
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;

  await callCommand(() => commands.installModpack(instanceId, projectId, versionId, channel));
}
