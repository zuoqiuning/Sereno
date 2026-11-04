import { commands } from "@/bindings";
import type { AccountState, AuthlibProfile, DeviceCodeInfo, MicrosoftConfig } from "@/types";

import { callCommand } from "./invoke";

/**
 * 账户域接口。
 *
 * 微软登录走设备码流程：先 `start` 拿短码，再 `complete` 轮询授权结果。
 * 两次调用都在后端完成网络与令牌链路，前端只负责展示。
 */

/** 当前登录态。 */
export function getAccountState(): Promise<AccountState> {
  return callCommand(() => commands.getAccountState());
}

/** 微软登录第一步：申请设备码。 */
export function startMicrosoftLogin(): Promise<DeviceCodeInfo> {
  return callCommand(() => commands.startMicrosoftLogin());
}

/**
 * 微软登录第二步：轮询直到用户在浏览器里完成授权。
 *
 * 这里会挂起到授权完成或超时，调用方需要展示等待态。
 */
export function completeMicrosoftLogin(
  deviceCode: string,
  interval: number,
  expiresIn: number,
): Promise<AccountState> {
  return callCommand(() => commands.completeMicrosoftLogin(deviceCode, interval, expiresIn));
}

/** 退出微软账户（保留离线用户名）。 */
export function logoutAccount(): Promise<AccountState> {
  return callCommand(() => commands.logoutAccount());
}

/** 切到离线账户。 */
export function setOfflineAccount(name: string): Promise<AccountState> {
  return callCommand(() => commands.setOfflineAccount(name));
}

/** 新建离线档案并切换过去。 */
export function addOfflineAccount(name: string): Promise<AccountState> {
  return callCommand(() => commands.addOfflineAccount(name));
}

/** 切换到已有离线档案。 */
export function switchOfflineAccount(id: string): Promise<AccountState> {
  return callCommand(() => commands.switchOfflineAccount(id));
}

/** 删除离线档案（至少保留一个）。 */
export function removeOfflineAccount(id: string): Promise<AccountState> {
  return callCommand(() => commands.removeOfflineAccount(id));
}

/** 切回已登录过的微软账户（令牌仍在系统密钥库）。 */
export function activateMicrosoftAccount(): Promise<AccountState> {
  return callCommand(() => commands.activateMicrosoftAccount());
}

/** 打开微软设备码授权页（后端只放行微软域名）。 */
export async function openMicrosoftLogin(url: string): Promise<void> {
  await callCommand(() => commands.openMicrosoftLogin(url));
}

/** 取消正在进行的设备码轮询（关闭登录弹窗时调用）。 */
export async function cancelMicrosoftLogin(): Promise<void> {
  await callCommand(() => commands.cancelMicrosoftLogin());
}

/** 微软登录配置（客户端 ID 是否就绪）。 */
export function getMicrosoftConfig(): Promise<MicrosoftConfig> {
  return callCommand(() => commands.getMicrosoftConfig());
}

/**
 * 第三方登录（Yggdrasil / authlib-injective）。
 *
 * 密码只在这一次调用里出现——后端用它换取访问令牌后立即丢弃，
 * 令牌则存进系统凭据管理器。
 */
export function authlibLogin(
  apiRoot: string,
  username: string,
  password: string,
  serverName: string | null,
): Promise<AccountState> {
  return callCommand(() => commands.authlibLogin(apiRoot, username, password, serverName));
}

/** 切换到已登录过的第三方档案。 */
export function switchAuthlibAccount(id: string): Promise<AccountState> {
  return callCommand(() => commands.switchAuthlibAccount(id));
}

/** 退出第三方账户：删除本机令牌并移除档案。 */
export function logoutAuthlibAccount(id: string): Promise<AccountState> {
  return callCommand(() => commands.logoutAuthlibAccount(id));
}

/** 已登录的第三方档案列表。 */
export function listAuthlibAccounts(): Promise<AuthlibProfile[]> {
  return callCommand(() => commands.listAuthlibAccounts());
}

/** 启动时预热微软会话，失败不阻塞。 */
export async function warmAccountSession(): Promise<void> {
  try {
    await commands.warmAccountSession();
  } catch {
    // 预热失败不影响启动，真正启动时会重试
  }
}
