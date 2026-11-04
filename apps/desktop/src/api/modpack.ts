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

/**
 * CurseForge 整合包概览：游戏版本、加载器与附加文件数。
 *
 * CF 整合包在 CurseForge 上是一个普通文件（`projectId` + `fileId`），
 * 概览需要先把它下载下来解析 `manifest.json`，因此比 Modrinth 慢。
 */
export function getCfModpackInfo(projectId: number, fileId: number): Promise<ModpackInfo> {
  return callCommand(() => commands.getCfModpackInfo(projectId, fileId));
}

/**
 * 安装 CurseForge 整合包内容：原版 → 加载器 → MOD → 覆盖文件。
 *
 * 与 Modrinth 的关键差异：CF 的清单里没有下载地址，
 * 后端会先回查 CurseForge API 把 `fileId` 解析成下载地址与校验值。
 */
export async function installCfModpack(
  instanceId: string,
  projectId: number,
  fileId: number,
  onProgress: (progress: DownloadProgress) => void,
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;

  await callCommand(() => commands.installCfModpack(instanceId, projectId, fileId, channel));
}

/**
 * 导出实例为 Modrinth 整合包（`.mrpack`），返回导出路径。
 *
 * 能追溯到 Modrinth 来源的 Mod 写进索引条目（只含哈希与下载地址），
 * 其余内容与配置打进 `overrides/`。存档、日志与崩溃报告不导出。
 */
export function exportInstanceModpack(instanceId: string): Promise<string> {
  return callCommand(() => commands.exportInstanceModpack(instanceId));
}
