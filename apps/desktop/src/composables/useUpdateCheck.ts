import { onMounted, ref } from "vue";

import { checkAppUpdate } from "@/api/update";
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
 * ## 返回值
 *
 * `info` 供「关于」页的更新卡片复用——它自己也会调 `checkAppUpdate()`，
 * 但静默检查的结果可以省掉一次往返。
 */
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
          "可在「关于」页一键更新",
          5000,
        );
      }
    } catch (err) {
      // 只记日志，不提示用户——启动路径上的失败不该变成打扰。
      void log.warn("静默检查更新失败", {
        component: "useUpdateCheck",
        action: "checkSilently",
        error: String(err),
      });
    } finally {
      checking.value = false;
    }
  }

  onMounted(() => {
    // 挂载后再触发：先让首屏出来，再去问服务器。
    void checkSilently();
  });

  return { info, checking, checkSilently };
}
