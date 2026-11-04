import { Channel } from "@tauri-apps/api/core";
import { commands } from "@/bindings";
import type {
  CfFile,
  DownloadProgress,
  ModVersion,
  ResourceDependencyInfo,
  ResourceKind,
  ResourceSearchPage,
  ResourceSearchQuery,
} from "@/types";

import { callCommand } from "./invoke";

/**
 * 社区资源搜索：单源分页或 Modrinth + CurseForge 聚合。
 */
export function searchResources(query: ResourceSearchQuery): Promise<ResourceSearchPage> {
  return callCommand(() => commands.searchResources(query));
}

/**
 * Modrinth 获取版本列表。
 *
 * `gameVersions` 可以一次传多个游戏版本（Modrinth 接口支持多值），
 * 把「某个版本前缀下的全部小版本」压成一次请求。
 */
export async function getModVersions(
  projectId: string,
  gameVersions: string[],
  loader: string,
): Promise<ModVersion[]> {
  return callCommand(() => commands.getModVersions(projectId, gameVersions, loader));
}

/**
 * 通用资源文件下载：按种类写入实例的 mods / resourcepacks / datapacks / shaderpacks。
 *
 * `instanceId` 传 `null` 时下载到启动器的下载目录（`%APPDATA%\Sereno\downloads\<kind>`）。
 */
export async function downloadResource(
  instanceId: string | null,
  kind: ResourceKind,
  fileUrl: string,
  filename: string,
  sha1: string | null,
  taskId: string,
  onProgress?: (progress: DownloadProgress) => void,
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  if (onProgress) channel.onmessage = onProgress;

  await callCommand(() =>
    commands.downloadResource(instanceId, kind, fileUrl, filename, sha1, taskId, channel),
  );
}

/** 取消一个资源下载任务；返回后端是否真的找到了这个任务。 */
export function cancelDownload(taskId: string): Promise<boolean> {
  return callCommand(() => commands.cancelDownload(taskId));
}

/**
 * CurseForge 获取版本列表（返回 CfFile 结构）
 */
export async function getCfModVersions(
  modId: string,
  mcVersion: string,
  loader: string,
): Promise<CfFile[]> {
  return callCommand(() => commands.getCfModVersions(modId, mcVersion, loader));
}

/**
 * 批量补全 CurseForge 的版本分支。
 *
 * CurseForge 的文件列表按上传时间排序，只查一次会漏掉较早发布的分支，
 * 必须按具体游戏版本逐个查询；这些查询在后端并发执行。
 */
export async function getCfModVersionsBatch(
  modId: string,
  versions: string[],
  loader: string,
): Promise<CfFile[]> {
  return callCommand(() => commands.getCfModVersionsBatch(modId, versions, loader));
}

/**
 * 批量解析依赖项目名称（Modrinth project id / CurseForge modId）。
 */
export function getResourceDependencies(
  source: string,
  ids: string[],
): Promise<ResourceDependencyInfo[]> {
  return callCommand(() => commands.getResourceDependencies(source, ids));
}

/**
 * 安装单个必需前置（后端按实例版本 / 加载器挑选文件）。
 */
export function installResourceDependency(
  instanceId: string,
  source: string,
  projectId: string,
  versionId: string | null,
): Promise<string> {
  return callCommand(() =>
    commands.installResourceDependency(instanceId, source, projectId, versionId),
  );
}

/**
 * 在系统浏览器打开资源项目页（后端只接受合法 slug）。
 */
export async function openResourcePage(source: string, slug: string, kind: string): Promise<void> {
  await callCommand(() => commands.openResourcePage(source, slug, kind));
}
