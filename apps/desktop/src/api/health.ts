import { commands } from "@/bindings";
import type { HealthReport } from "@/types";

import { callCommand } from "./invoke";

/** 实例健康检查：启动预检 + Mod 依赖图 + 崩溃诊断。 */
export function checkInstanceHealth(instanceId: string): Promise<HealthReport> {
  return callCommand(() => commands.checkInstanceHealth(instanceId));
}
