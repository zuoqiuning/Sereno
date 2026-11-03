import { commands } from "@/bindings";
import type { AppInfo, AppSettings_Serialize, LogLevel } from "@/types";

import { callCommand, callCommandQuiet } from "./invoke";

/**
 * 设置与应用级接口。
 *
 * 这是唯一直接 import `bindings` 的地方之一——所有调用都经过
 * `callCommand` 拆包，业务代码见不到 `typedError` 的结果形态。
 */

/** 获取应用基本信息（版本、状态目录、平台）。 */
export function getAppInfo(): Promise<AppInfo> {
  return callCommand(() => commands.getAppInfo());
}

/** 读取当前设置（首页展示离线账户等信息用）。 */
export function getSettings(): Promise<AppSettings_Serialize> {
  return callCommand(() => commands.getSettings());
}

/** 预加载结束：关闭启动画面并显示主窗口。 */
export async function finishStartup(): Promise<void> {
  await callCommand(() => commands.finishStartup());
}

/**
 * 把前端日志写进后端的同一条 `tracing` 管线。
 *
 * `target` 固定为 `sereno_ui`。前端只允许发 `warn` / `error` / `info`，
 * 后端会把 `debug` 降级为 `info`。
 *
 * 日志上报**永远不能影响主流程**，因此失败时静默忽略。
 */
export function logFromFrontend(
  level: LogLevel,
  message: string,
  fields?: Record<string, string>,
): Promise<void> {
  return callCommandQuiet(
    () => commands.logFromFrontend(level, message, fields ?? null),
    null,
  ).then(() => undefined);
}

/**
 * 前端日志的便捷入口。
 *
 * 业务代码直接 `log.error("下载失败", { instanceId })`，不必每次都拼
 * level 字符串。字段名统一用下面的**标准字段**，避免各处各写各的
 * （`instanceId` / `component` / `action`），后端就能按字段做检索。
 */
export const log = {
  /** 生命周期节点，一次操作一条 */
  info: (message: string, fields?: Record<string, string>): Promise<void> =>
    logFromFrontend("info", message, fields),

  /** 已降级但流程继续 */
  warn: (message: string, fields?: Record<string, string>): Promise<void> =>
    logFromFrontend("warn", message, fields),

  /** 用户可感知的失败 */
  error: (message: string, fields?: Record<string, string>): Promise<void> =>
    logFromFrontend("error", message, fields),
} as const;

/** 标准日志字段名。 */
export const LOG_FIELDS = {
  /** 触发日志的组件名，如 `InstanceList` */
  component: "component",
  /** 触发日志的动作名，如 `create` */
  action: "action",
  /** 关联的实例 id */
  instanceId: "instanceId",
  /** 关联的下载会话 id */
  sessionId: "sessionId",
} as const;
