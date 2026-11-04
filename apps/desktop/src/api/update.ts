import { commands } from "@/bindings";
import type { UpdateDownload, UpdateInfo } from "@/types";

import { callCommand } from "./invoke";

/**
 * 启动器自身的更新接口。
 *
 * **下载地址不由前端传入**：后端从官方 Release 的资产列表里解析，
 * 前端只能指定「要下哪个已检查到的版本」。
 */

/** 检查是否有新版本。 */
export function checkAppUpdate(): Promise<UpdateInfo> {
  return callCommand(() => commands.checkAppUpdate());
}

/**
 * 下载更新并校验 SHA-256。
 *
 * 校验不通过时后端会报错，**不会**返回可执行的安装包路径。
 */
export function downloadAppUpdate(info: UpdateInfo): Promise<UpdateDownload> {
  return callCommand(() => commands.downloadAppUpdate(info));
}

/** 启动已校验的安装程序。 */
export async function launchAppUpdater(path: string): Promise<void> {
  await callCommand(() => commands.launchAppUpdater(path));
}
