import { commands } from "@/bindings";

import { callCommand } from "./invoke";

export type SupporterTarget = "pcl" | "bmclapi" | "mcim" | "sereno" | "easytier";

/** 打开鸣谢页/关于页白名单内的赞助链接。 */
export async function openSupporterPage(target: SupporterTarget): Promise<void> {
  await callCommand(() => commands.openSupporterPage(target));
}

/** 关于页：直达 GitHub 仓库。 */
export async function openProjectRepo(): Promise<void> {
  await callCommand(() => commands.openProjectRepo());
}

/** 反馈页：直达 GitHub Issues。 */
export async function openProjectIssues(): Promise<void> {
  await callCommand(() => commands.openProjectIssues());
}
