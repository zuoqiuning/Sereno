import { commands } from "@/bindings";
import type { VersionDetail, VersionManifest } from "@/types";

import { callCommand } from "./invoke";

/**
 * 版本域接口。
 *
 * 后端已经把 Mojang 的 JSON 解析成结构体，前端**不需要也不应该**再解析一遍 JSON。
 * 两条命令的差别很重要：
 *
 * - `getVersionManifest` —— 只拿清单（版本号 + 类型 + 详情 JSON 的地址），
 *   数据量小，用于「选择要装哪个版本」的列表；
 * - `getVersionDetail` —— 拿某个版本的完整详情（依赖库、主类、Java 版本要求），
 *   **已经把 `inheritsFrom` 继承链合并好了**，拿到的依赖库可以直接算 classpath。
 */

/** 获取 Mojang 版本清单。 */
export function getVersionManifest(): Promise<VersionManifest> {
  return callCommand(() => commands.getVersionManifest());
}

/** 获取指定版本的详情（已合并继承链）。 */
export function getVersionDetail(id: string): Promise<VersionDetail> {
  return callCommand(() => commands.getVersionDetail(id));
}
