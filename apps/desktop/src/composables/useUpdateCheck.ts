import { onMounted, ref } from "vue";

import { checkAppUpdate, getUpdateStatus } from "@/api/update";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import type { AppUpdateInfo } from "@/types";

/**
 * 启动时的**静默更新检查**。
 *
 * ## 为什么放在 composable 而不是 App.vue 里直接写
 *
 * 静默检查有三件容易做错的事，集中在这里做才不会漏：
 *
 * 1. **必须在应用挂载后才发起**——放在 `main.ts` 顶部会拖慢首屏；
 * 2. **绝不能影响启动**——服务端挂了、证书有问题、网络不通，
 *    都只是「查不到更新」，不能抛到外层让应用起不来；
 * 3. **不要打扰用户**——检查是静默的，只在**真的有新版本**时才提示。
 *
 * ## 与「自动检查」开关的关系
 *
 * 用户在更新页关掉「启动时自动检查」后，这里就**完全不发请求**——
 * 不是「检查了但静默」，而是一次网络请求都不发。
 */
/** 把任意抛出物转成可读文案。 */
function toMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

export function useUpdateCheck() {
  const toast = useToastStore();

  /** 检查结果；`null` 表示还没查完或查失败。 */
  const info = ref<AppUpdateInfo | null>(null);
  /** 是否正在检查。 */
  const checking = ref(false);

  /**
   * 静默检查一次。
   *
   * **失败一律吞掉**：更新服务器不可用不是用户的问题，
   * 也不该在启动路径上抛出异常。
   */
  async function checkSilently(): Promise<void> {
    if (checking.value) return;
    checking.value = true;

    try {
      const result = await checkAppUpdate();
      info.value = result;

      if (result.available) {
        toast.success(
          `发现新版本 v${result.latestVersion}`,
          "可在「更新」页一键安装",
          5000,
        );
      }
    } catch (err) {
      // 只记日志，不提示用户——启动路径上的失败不该变成打扰。
      //
      // `String(err)` 对 API 错误对象会得到 "[object Object]"，
      // 排查时什么信息都没有——用 `isApiError` 取真正的消息。
      void log.warn("静默检查更新失败", {
        component: "useUpdateCheck",
        action: "checkSilently",
        error: toMessage(err),
      });
    } finally {
      checking.value = false;
    }
  }

  onMounted(async () => {
    // 先问用户要不要自动检查——关掉的话一次请求都不该发。
    try {
      const status = await getUpdateStatus();
      if (!status.autoCheck) return;
    } catch (err) {
      // 读不到设置时按「开启」处理：静默检查失败不该挡住启动。
      void log.warn("读取更新设置失败，仍将检查更新", {
        component: "useUpdateCheck",
        action: "readStatus",
        error: toMessage(err),
      });
    }

    // 挂载后再触发：先让首屏出来，再去问服务器。
    void checkSilently();
  });

  return { info, checking, checkSilently };
}
