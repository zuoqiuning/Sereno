import { Channel } from "@tauri-apps/api/core";
import { commands } from "@/bindings";
import type { DownloadProgress } from "@/types";

import { callCommand } from "./invoke";

/**
 * 安装原版游戏，并通过回调接收进度。
 *
 * Channel 的载荷是 `DownloadProgress` 判别联合：
 * `kind === "session"` 是会话汇总，`kind === "task"` 是单文件快照。
 */
export async function installGame(
  instanceId: string,
  onProgress: (progress: DownloadProgress) => void,
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;

  await callCommand(() => commands.installGame(instanceId, channel));
}

/**
 * 校验并修复实例：已存在且校验通过的文件会被跳过，只补缺失/损坏的部分。
 */
export async function repairInstance(
  instanceId: string,
  onProgress: (progress: DownloadProgress) => void,
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;

  await callCommand(() => commands.repairInstance(instanceId, channel));
}
