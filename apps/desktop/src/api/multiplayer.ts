import { commands } from "@/bindings";
import type { DetectedWorld, MultiplayerStatus } from "@/types";

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
export function joinMultiplayer(roomCode: string, mcPort: number): Promise<MultiplayerStatus> {
  return callCommand(() => commands.joinMultiplayer(roomCode, mcPort));
}

/** 停止当前联机会话。 */
export async function stopMultiplayer(): Promise<void> {
  await callCommand(() => commands.stopMultiplayer());
}

/** 自动探测本机游戏世界端口（Java 进程监听端口 + MC 状态探测）。 */
export function detectWorldPort(): Promise<DetectedWorld | null> {
  return callCommand(() => commands.detectWorldPort());
}

/** 查询联机状态（轮询用）。后端命令直接返回值、不返回 Result。 */
export function getMultiplayerStatus(): Promise<MultiplayerStatus> {
  return commands.getMultiplayerStatus();
}
