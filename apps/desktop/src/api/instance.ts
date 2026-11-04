import { commands, events } from "@/bindings";
import type {
  GameExited,
  ImportScan,
  InstanceLogs,
  InstanceMetadata,
  SaveSummary,
  ServerSummary,
} from "@/types";

export type { SaveSummary, ServerSummary };

import { callCommand } from "./invoke";

/**
 * 实例域接口。
 */

/** 读取实例的最新日志与崩溃报告。 */
export function getInstanceLogs(instanceId: string): Promise<InstanceLogs> {
  return callCommand(() => commands.readInstanceLogs(instanceId));
}

/** 获取本地所有实例。 */
export function listInstances(): Promise<InstanceMetadata[]> {
  return callCommand(() => commands.listInstances());
}

/** 正在运行的实例 ID（仅内存态，重启启动器即清空）。 */
export function getRunningInstances(): Promise<string[]> {
  return callCommand(() => commands.getRunningInstances());
}

/** 订阅“游戏进程已退出”事件；返回取消订阅函数。 */
export function onGameExited(callback: (payload: GameExited) => void) {
  return events.gameExited.listen((event) => callback(event.payload));
}

/**
 * 导出诊断包（启动器日志 + 实例日志 / 崩溃报告），返回 zip 绝对路径。
 * 传 `null` 时只导出启动器级日志。
 */
export function exportDiagnostic(instanceId: string | null): Promise<string> {
  return callCommand(() => commands.exportDiagnostic(instanceId));
}

/** 创建一个新的原版实例。 */
export function createInstance(
  gameVersion: string,
  displayName: string,
): Promise<InstanceMetadata> {
  return callCommand(() => commands.createInstance(gameVersion, displayName));
}

/**
 * 更新实例级启动配置。
 *
 * `javaPath` 为 `null`、`memoryMb` 为 `null`、`jvmArgs` 为空数组时，
 * 表示回落到全局设置。
 */
export function updateInstanceOptions(
  instanceId: string,
  javaPath: string | null,
  memoryMb: number | null,
  jvmArgs: string[],
): Promise<InstanceMetadata> {
  return callCommand(() => commands.updateInstanceOptions(instanceId, javaPath, memoryMb, jvmArgs));
}

/** 选择实例专用的 Java 可执行文件；取消返回 `null`。 */
export function selectJavaExecutable(): Promise<string | null> {
  return callCommand(() => commands.selectJavaExecutable());
}

/** 收藏 / 取消收藏实例（列表里置顶显示）。 */
export function setInstanceFavorite(
  instanceId: string,
  favorite: boolean,
): Promise<InstanceMetadata> {
  return callCommand(() => commands.setInstanceFavorite(instanceId, favorite));
}

/** 设置 / 清除实例分组。传 `null` 或空串表示移出分组。 */
export function setInstanceGroup(
  instanceId: string,
  group: string | null,
): Promise<InstanceMetadata> {
  return callCommand(() => commands.setInstanceGroup(instanceId, group));
}

/**
 * 扫描外部目录，找出可导入的实例候选。
 *
 * 支持官方启动器 / PCL / HMCL（含 `versions/`）、MultiMC / Prism（含 `.minecraft/`）
 * 与裸游戏目录。
 */
export function scanImportCandidates(source: string): Promise<ImportScan> {
  return callCommand(() => commands.scanImportCandidates(source));
}

/** 选择要导入的外部实例目录；取消返回 `null`。 */
export function selectImportDirectory(): Promise<string | null> {
  return callCommand(() => commands.selectImportDirectory());
}

/**
 * 导入外部实例：新建实例 + 搬运用户数据（Mod、配置、存档）。
 *
 * 游戏本体与库文件**不复制**，走 Sereno 自己的安装流程重新下载。
 */
export function importInstance(
  source: string,
  candidateLabel: string,
  gameVersion: string,
  loaderType: string,
  loaderVersion: string | null,
  displayName: string,
): Promise<InstanceMetadata> {
  return callCommand(() =>
    commands.importInstance(
      source,
      candidateLabel,
      gameVersion,
      loaderType,
      loaderVersion,
      displayName,
    ),
  );
}

/** 列出实例的全部存档（按最后游玩时间倒序）。 */
export function listInstanceSaves(instanceId: string): Promise<SaveSummary[]> {
  return callCommand(() => commands.listInstanceSaves(instanceId));
}

/** 删除存档（回收站优先，失败自动回退物理删除）。 */
export function deleteInstanceSave(instanceId: string, saveName: string): Promise<null> {
  return callCommand(() => commands.deleteInstanceSave(instanceId, saveName));
}

/** 打开某个存档的目录。 */
export function openSaveFolder(instanceId: string, saveName: string): Promise<null> {
  return callCommand(() => commands.openSaveFolder(instanceId, saveName));
}

/** 列出实例的服务器列表（servers.dat，保持游戏内显示顺序）。 */
export function listInstanceServers(instanceId: string): Promise<ServerSummary[]> {
  return callCommand(() => commands.listInstanceServers(instanceId));
}

/** 添加服务器（追加到列表末尾）。 */
export function addInstanceServer(
  instanceId: string,
  name: string,
  address: string,
): Promise<null> {
  return callCommand(() => commands.addInstanceServer(instanceId, name, address));
}

/** 按下标删除服务器。 */
export function deleteInstanceServer(instanceId: string, index: number): Promise<null> {
  return callCommand(() => commands.deleteInstanceServer(instanceId, index));
}
