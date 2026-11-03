import type { LauncherError } from "@/types";

/**
 * IPC 封装层：**唯一允许接触 `bindings.ts` 的地方**。
 *
 * 业务代码（`composables/`、`components/`、`views/`）一律调用本目录下的封装函数，
 * 既不拼运行时命令字符串，也不直接 import 生成的 `commands`。
 *
 * 好处：错误处理、日志、类型收窄集中在一处；绕过这一层，
 * `undefined` 作为「空值」被序列化边界拒绝这类问题会推迟到运行时才暴露。
 */

/**
 * `tauri-specta` 生成的命令返回形态。
 *
 * `typedError` 把后端的 `Result<T, LauncherError>` 包成
 * `{ status: "ok", data }` / `{ status: "error", error }`，
 * 前端在这里统一拆开，业务代码只见到 `T` 或异常。
 */
export type CommandResult<T, E> = { status: "ok"; data: T } | { status: "error"; error: E };

/**
 * 统一错误信封。
 *
 * **关键设计**：它与后端的 `LauncherError` **判别联合**同构——
 * 用交叉类型 `E & { message, raw }` 保留全部判别字段。
 * 业务层可以按 `error.type` 收窄到具体变体，并直接读到该变体的字段：
 *
 * ```ts
 * catch (e) {
 *   if (isApiError(e) && e.type === "javaNotFound") {
 *     showToast(`需要 Java ${e.requiredMajor}+`);   // ✓ 拿得到判别字段
 *   }
 * }
 * ```
 *
 * 如果只留 `type: string`，业务层就只能写 `if (error.type === ...)` 拿不到任何
 * 附加信息，各写各的判断——地基阶段必须定死。
 */
export type ApiError<E extends LauncherError = LauncherError> = E & {
  /** 人类可读描述（前端本地化文案，含「下一步」提示） */
  message: string;
  /** 原始后端错误对象，未做任何加工 */
  raw: E;
};

/**
 * 错误类型 → 中文文案。
 *
 * Rust 侧 `thiserror` 的文案是给日志看的；界面文案放在前端，
 * 是为了可本地化，也避免把内部实现细节漏给用户。
 * 每个文案都要包含**用户能执行的下一步**。
 */
const ERROR_MESSAGES: Record<string, string> = {
  javaNotFound: "未找到可用的 Java，请在设置中指定 Java 路径",
  javaArchMismatch: "Java 架构与当前系统不匹配，请在设置中换一个",
  javaDownloadFailed: "Java 下载失败，请稍后重试或手动安装后指定路径",
  versionNotFound: "该版本不存在，可能已被 Mojang 下架",
  loaderNotFound: "该加载器版本不存在或不受支持",
  loaderInstallFailed: "加载器安装失败，请查看日志获取详情",
  downloadFailed: "下载失败，请稍后重试或切换镜像源",
  checksumMismatch: "文件校验失败，请重试下载",
  downloadCancelled: "下载已取消",
  allSourcesUnavailable: "所有镜像源均不可用，请检查网络连接",
  instanceNotFound: "实例不存在",
  instanceBusy: "实例正在运行，无法执行此操作",
  instanceAlreadyExists: "已存在同名实例",
  notAuthenticated: "尚未登录微软账号，请先在「账户」页完成登录",
  credentialStoreUnavailable: "系统密钥库不可用，请检查权限设置",
  tokenRefreshFailed: "登录已过期，请重新登录",
  curseForgeKeyMissing: "CurseForge API Key 未配置",
  resourceNotFound: "未找到该资源",
  io: "文件读写失败，请查看日志获取详情",
  parse: "数据格式错误",
  invalidConfig: "配置项无效",
  cancelled: "操作已取消",
  internal: "发生未知错误，请查看日志获取详情",
};

/**
 * 判断是否为后端的业务错误（带 `type` 判别标签）。
 *
 * 判别依据是**原始错误对象**上的 `type` 字段——Rust 侧用
 * `#[serde(tag = "type")]` 生成，前端直接可判。
 */
export function isLauncherError(error: unknown): error is LauncherError {
  return (
    error !== null &&
    typeof error === "object" &&
    "type" in error &&
    typeof (error as { type: unknown }).type === "string"
  );
}

/**
 * 判断是否为已包装的 `ApiError`。
 */
export function isApiError(error: unknown): error is ApiError {
  return error !== null && typeof error === "object" && "raw" in error;
}

/**
 * 把任意抛出物规整成 `ApiError`。
 *
 * 保留原始错误对象（`raw`）与它的全部判别字段，同时补上前端文案。
 */
export function toApiError(error: unknown): ApiError {
  if (isLauncherError(error)) {
    const raw = error as LauncherError;
    // 后端带了可读 message 时优先展示它（安装器 stdout/stderr 都在这里）
    let msg = ERROR_MESSAGES[raw.type] ?? ERROR_MESSAGES.internal;
    const backendMessage = (raw as { message?: unknown }).message;
    if (typeof backendMessage === "string" && backendMessage.trim()) {
      msg = backendMessage;
    }
    return {
      ...(raw as object),
      type: raw.type,
      message: msg,
      raw,
    } as ApiError;
  }

  if (error instanceof Error) {
    return {
      type: "internal",
      message: error.message || ERROR_MESSAGES.internal,
      raw: { type: "internal" } as LauncherError,
    } as ApiError;
  }

  return {
    type: "internal",
    message: ERROR_MESSAGES.internal,
    raw: { type: "internal" } as LauncherError,
  } as ApiError;
}

/**
 * 调用一个后端命令：拆开 `typedError` 的结果，把业务错误转成异常。
 *
 * 用法：`api/settings.ts` 里 `return callCommand(() => commands.getAppInfo())`。
 */
export async function callCommand<T>(
  fn: () => Promise<CommandResult<T, LauncherError>>,
): Promise<T> {
  const result = await fn();
  if (result.status === "ok") {
    return result.data;
  }
  throw toApiError(result.error);
}

/**
 * 调用一个后端命令，并把业务错误**就地处理**（不抛异常）。
 *
 * 用于「失败时不需要中断流程」的场景，例如前端日志上报。
 */
export async function callCommandQuiet<T>(
  fn: () => Promise<CommandResult<T, LauncherError>>,
  fallback: T,
): Promise<T> {
  try {
    const result = await fn();
    return result.status === "ok" ? result.data : fallback;
  } catch {
    return fallback;
  }
}
