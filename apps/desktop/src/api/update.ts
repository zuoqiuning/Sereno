import { commands, events } from "@/bindings";
import type { AppUpdateInfo, UpdateProgress } from "@/types";

import { callCommand } from "./invoke";

/**
 * 启动器自身的更新接口。
 *
 * **下载地址不由前端传入**：端点写死在 `tauri.conf.json` 的
 * `plugins.updater.endpoints` 里，前端只能触发「检查」与「安装」。
 *
 * 签名校验在 Rust 侧由 `tauri-plugin-updater` 完成——
 * 下载的产物必须通过 minisign 验签，否则插件直接拒绝安装。
 */

/** 检查是否有新版本。 */
export function checkAppUpdate(): Promise<AppUpdateInfo> {
  return callCommand(() => commands.checkAppUpdate());
}

/**
 * 下载更新、验签并安装。
 *
 * **Windows 上安装器启动后应用会自动退出**，因此这个 Promise
 * 大概率不会正常 resolve——这是预期行为，不是错误。
 *
 * 返回 `Promise<null>` 而不是 `Promise<void>`：Specta 把 Rust 侧的
 * `Result<(), E>` 映射成 `null`，这里保持与生成类型一致。
 */
export function installAppUpdate(): Promise<null> {
  return callCommand(() => commands.installAppUpdate());
}

/** 重启应用（安装完成后调用）。 */
export function relaunchApp(): Promise<null> {
  return callCommand(() => commands.relaunchApp());
}

/**
 * 订阅更新下载进度。
 *
 * 返回取消订阅函数。
 *
 * 用事件而不是 Channel：Windows 上安装器启动后进程立刻退出，
 * Channel 的发送端会随命令一起被 drop，最后一个进度事件可能送不到。
 */
export function onUpdateProgress(
  handler: (progress: UpdateProgress) => void,
): () => void {
  const unlisten = events.updateProgress.listen((event) => {
    handler(event.payload);
  });

  return () => {
    void unlisten.then((stop) => stop());
  };
}
