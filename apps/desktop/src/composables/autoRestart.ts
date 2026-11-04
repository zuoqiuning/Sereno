/**
 * 崩溃自动重启（路线图 #4）。
 *
 * 全局监听 `game-exited`：非零退出码 + 设置开启 → 倒计时后自动重启，
 * 单实例单会话最多 3 次——防止「崩溃循环」反复拉起。
 *
 * 为什么放 App.vue 级而不是 HomeView：游戏退出事件在任何页面都可能到达，
 * 监听必须跟着应用生命周期而不是某个视图。
 */
import { launchGame, onGameExited } from "@/api/instance";
import { getSettings } from "@/api/settings";
import { useToastStore } from "@/stores/toast";

/** 单实例单会话的自动重启上限。 */
const MAX_AUTO_RESTARTS = 3;
/** 崩溃后等待几秒再重启（给用户看清 toast，也给系统收尾时间）。 */
const RESTART_DELAY_MS = 5000;

/** 各实例本会话的已自动重启次数。 */
const restartCounts = new Map<string, number>();
/** 待执行的重启定时器（同一实例不叠加）。 */
const pendingTimers = new Map<string, number>();

/** 用户手动启动实例时调用：重置该实例的自动重启计数。 */
export function resetRestartCount(instanceId: string): void {
  restartCounts.delete(instanceId);
  const timer = pendingTimers.get(instanceId);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    pendingTimers.delete(instanceId);
  }
}

/** 注册全局游戏退出监听（App.vue 挂载时调用一次）。 */
export function registerAutoRestart(): void {
  const toast = useToastStore();

  void onGameExited((payload) => {
    // 正常退出：清零计数，不给「偶尔崩一次」的用户留下陈旧计数
    if ((payload.exitCode ?? 0) === 0) {
      restartCounts.delete(payload.instanceId);
      return;
    }

    void (async () => {
      const settings = await getSettings().catch(() => null);
      if (!settings?.autoRestartOnCrash) return;

      const count = restartCounts.get(payload.instanceId) ?? 0;
      if (count >= MAX_AUTO_RESTARTS) {
        toast.warning(
          "游戏反复崩溃",
          `已自动重启 ${MAX_AUTO_RESTARTS} 次仍未恢复，请查看日志排查原因`,
          6000,
        );
        return;
      }

      restartCounts.set(payload.instanceId, count + 1);
      const attempt = count + 1;
      toast.warning(
        "游戏崩溃了",
        `${RESTART_DELAY_MS / 1000} 秒后自动重启（第 ${attempt}/${MAX_AUTO_RESTARTS} 次）`,
        RESTART_DELAY_MS,
      );

      const timer = window.setTimeout(() => {
        pendingTimers.delete(payload.instanceId);
        void launchGame(payload.instanceId).catch((error: unknown) => {
          toast.error("自动重启失败", String(error), 5200);
        });
      }, RESTART_DELAY_MS);
      pendingTimers.set(payload.instanceId, timer);
    })();
  });
}
