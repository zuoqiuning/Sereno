import { commands } from "@/bindings";
import type { JavaCandidate } from "@/types";

import { callCommand } from "./invoke";

/**
 * Java 域接口。
 */

/** 扫描系统中可用的 Java 运行时。 */
export function getJavaCandidates(): Promise<JavaCandidate[]> {
  return callCommand(() => commands.getJavaCandidates());
}

/** 自动下载并安装指定主版本的 Temurin JRE。 */
export function installJavaRuntime(major: number): Promise<JavaCandidate> {
  return callCommand(() => commands.installJavaRuntime(major));
}
