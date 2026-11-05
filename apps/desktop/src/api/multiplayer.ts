import { commands } from "@/bindings";
import type { DetectedWorld, MultiplayerStatus, WorldCheck } from "@/types";

import { callCommand } from "./invoke";

/**
 * 联机域接口。
 */

/** 房主开房。房间码为空时由后端生成新房间码。 */
export function hostMultiplayer(
  roomCode: string | null,
  mcPort: number,
): Promise<MultiplayerStatus> {
  return callCommand(() => commands.hostMultiplayer(roomCode, mcPort));
}

/** 房客加入房间（支持小写 / 混淆字符 / 缺分隔符，后端自动纠偏）。 */
export function joinMultiplayer(
  roomCode: string,
  mcPort: number,
  tunMode = false,
): Promise<MultiplayerStatus> {
  return callCommand(() => commands.joinMultiplayer(roomCode, mcPort, tunMode));
}

/** 停止当前联机会话。 */
export async function stopMultiplayer(): Promise<void> {
  await callCommand(() => commands.stopMultiplayer());
}

/** 自动探测本机游戏世界端口（Java 进程监听端口 + MC 状态探测）。 */
export function detectWorldPort(): Promise<DetectedWorld | null> {
  return callCommand(() => commands.detectWorldPort());
}

/**
 * 世界自检：TCP 连接层判定端口可达性，不依赖 MC 状态协议
 * （NeoForge 局域网世界会拒绝状态查询）。
 *
 * - `strict = false`（房主）：TCP 连上即算可达；
 * - `strict = true`（房客）：必须经受住进一步判定（转发监听永远
 *   接受连接，链路断了也会秒关）。
 */
export function checkWorldReachable(port: number, strict: boolean): Promise<WorldCheck> {
  return callCommand(() => commands.checkWorldReachable(port, strict));
}

/** 查询联机状态（轮询用）。后端命令直接返回值、不返回 Result。 */
export function getMultiplayerStatus(): Promise<MultiplayerStatus> {
  return commands.getMultiplayerStatus();
}
