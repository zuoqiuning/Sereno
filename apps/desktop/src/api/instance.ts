import { commands, events } from "@/bindings";
import type { GameExited, ImportScan, InstanceLogs, InstanceMetadata } from "@/types";

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
