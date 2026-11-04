<script setup lang="ts">
import { computed } from "vue";
import { ArrowDownToLine, RefreshCw, X } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import { installAppUpdate, onUpdateProgress } from "@/api/update";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import { useUpdateStore } from "@/stores/update";

/**
 * 全局更新卡片。
 *
 * ## 为什么是卡片而不是 Toast
 *
 * Toast 几秒就消失——但更新是要用户**做决定**的事：
 * 看到 → 判断要不要现在装 → 点「立即更新」。
 * 一闪而过的提示等于没提示。卡片常驻，直到用户处理或关掉。
 *
 * ## 什么时候出现
 *
 * 启动时（splash 阶段）Rust 侧就在后台检查更新了，**不等结果**——
 * 界面照常显示。等检测回来，通过 `update-available` 事件通知前端。
 *
 * 设置页手动点「检查更新」发现新版本时，也会叫醒同一张卡片。
 */
const toast = useToastStore();
const update = useUpdateStore();

const visible = computed(() => update.visible);

function toMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

/** 立即更新：下载 → 验签 → 启动安装器。 */
async function install(): Promise<void> {
  if (update.installing) return;
  update.installing = true;
  update.resetProgress();

  const stop = onUpdateProgress((progress) => {
    update.downloaded = progress.downloaded;
    update.total = progress.total;
    update.percent = progress.percent;
  });

  try {
    await installAppUpdate();
    // Windows 上安装器启动后应用会退出，走不到这里；
    // 能走到说明安装器启动失败。
    toast.info("安装程序已启动", "安装完成后重新打开启动器即可");
    update.latestVersion = "";
    update.notes = "";
  } catch (err) {
    const message = toMessage(err);
    toast.error("更新失败", message);
    void log.error(`更新失败: ${message}`, { component: "UpdateCard" });
  } finally {
    stop();
    update.installing = false;
    update.resetProgress();
  }
}

/** 大小的人类可读格式。 */
function formatSize(bytes: number): string {
  if (bytes <= 0) return "";
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
</script>

<template>
  <!--
    固定在右下角：贴着边缘但不挡内容，用户该做什么一目了然。
    z-index 高于普通内容，但低于模态框——模态框打开时不该被卡片压住。
  -->
  <Transition name="update-card">
    <div v-if="visible" class="update-card" role="status" aria-live="polite">
      <div class="flex items-start gap-3">
        <span
          class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand-soft text-brand-600"
        >
          <RefreshCw :size="17" :class="{ 'animate-spin': update.installing }" />
        </span>

        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <p class="text-h2 font-semibold text-ink">发现新版本</p>
            <span class="font-mono text-caption font-semibold text-brand-600">
              v{{ update.latestVersion }}
            </span>
          </div>
          <p class="mt-0.5 text-caption text-ink-muted">
            当前版本 v{{ update.currentVersion }}
          </p>

          <!-- 更新说明：只显示前几行，完整内容在设置页看 -->
          <p
            v-if="update.notes"
            class="mt-2 max-h-24 overflow-hidden whitespace-pre-wrap rounded-md bg-surface-muted px-2.5 py-2 text-micro leading-5 text-ink-soft"
          >
            {{ update.notes }}
          </p>

          <!-- 下载进度 -->
          <div v-if="update.installing" class="mt-2.5 flex flex-col gap-1.5">
            <div class="h-1.5 w-full overflow-hidden rounded-full bg-surface-muted">
              <div
                v-if="update.percent !== null"
                class="h-full rounded-full bg-brand-500 transition-[width] duration-200"
                :style="{ width: `${update.percent}%` }"
              />
              <div v-else class="h-full w-1/3 animate-pulse rounded-full bg-brand-500/60" />
            </div>
            <p class="text-micro text-ink-muted">
              <template v-if="update.percent !== null && update.total > 0">
                {{ formatSize(update.downloaded) }} / {{ formatSize(update.total) }} ·
                {{ update.percent }}%
              </template>
              <template v-else-if="update.downloaded > 0">
                已下载 {{ formatSize(update.downloaded) }}
              </template>
              <template v-else>正在准备…</template>
            </p>
          </div>
        </div>

        <!-- 关闭按钮：只在空闲时能点，安装中关掉会留下半个安装器 -->
        <button
          type="button"
          class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-ink/6 hover:text-ink disabled:cursor-not-allowed disabled:opacity-40"
          title="稍后再说"
          :disabled="update.installing"
          @click="update.dismiss()"
        >
          <X :size="14" />
        </button>
      </div>

      <div class="mt-3 flex items-center justify-end gap-2">
        <AppButton
          variant="ghost"
          size="sm"
          :disabled="update.installing"
          @click="update.dismiss()"
        >
          稍后
        </AppButton>
        <AppButton
          variant="primary"
          size="sm"
          :loading="update.installing"
          :disabled="update.installing"
          @click="install"
        >
          <ArrowDownToLine :size="13" />
          立即更新
        </AppButton>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.update-card {
  position: fixed;
  right: 20px;
  bottom: 20px;
  z-index: 40;
  width: 340px;
  padding: 14px;
  border: 1px solid var(--color-line);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  box-shadow: var(--shadow-lg, 0 12px 32px rgb(0 0 0 / 18%));
}

/* 从右下角淡入：跟「从边缘提示」的方向一致 */
.update-card-enter-active,
.update-card-leave-active {
  transition:
    opacity 0.22s var(--ease-out-soft),
    transform 0.22s var(--ease-out-soft);
}

.update-card-enter-from,
.update-card-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.98);
}
</style>
