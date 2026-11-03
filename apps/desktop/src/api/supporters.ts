import { commands } from "@/bindings";

import { callCommand } from "./invoke";

export type SupporterTarget = "pcl" | "bmclapi";

/** 打开鸣谢页白名单内的赞助链接。 */
export async function openSupporterPage(target: SupporterTarget): Promise<void> {
  await callCommand(() => commands.openSupporterPage(target));
}
