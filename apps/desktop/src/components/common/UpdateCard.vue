<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ArrowDownToLine, CircleCheck, RefreshCw, ShieldCheck } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import { checkAppUpdate, installAppUpdate, onUpdateProgress } from "@/api/update";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import { APP_VERSION } from "@/constants/app";
import type { AppUpdateInfo } from "@/types";

/**
 * 更新卡片。
 *
 * 走的是官方 `tauri-plugin-updater` 通道：
 * 端点写死在配置里，**前端不能指定下载地址**；
 * 下载的产物必须通过 **minisign 验签**，对不上就不会被安装。
 *
 * 交互上分三段：检查 → 下载（带进度）→ 安装。
 * 下载与安装在插件内部是一体的（先验签再启动安装器），
 * 因此这里只呈现「下载中」与「安装中」两种进行态。
 */
const toast = useToastStore();

const checking = ref(false);
const installing = ref(false);
/** 下载进度（0–100）；总量未知时为 null，走不确定态动画。 */
const percent = ref<number | null>(null);
/** 已下载字节数，用于显示具体体积。 */
const downloaded = ref(0);
const total = ref(0);
const info = ref<AppUpdateInfo | null>(null);
const error = ref("");

const hasUpdate = computed(() => info.value?.available ?? false);

let stopProgress: (() => void) | null = null;

onMounted(() => {
  stopProgress = onUpdateProgress((progress) => {
    downloaded.value = progress.downloaded;
    total.value = progress.total;
    percent.value = progress.percent;
  });
});

onBeforeUnmount(() => {
  stopProgress?.();
  stopProgress = null;
});

function toMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

async function check(): Promise<void> {
  if (checking.value) return;
  checking.value = true;
  error.value = "";
  try {
    info.value = await checkAppUpdate();
    if (info.value.available) {
      toast.success(
        `发现新版本 v${info.value.latestVersion}`,
        `当前版本 v${APP_VERSION}`,
      );
    } else {
      toast.success("已是最新版本", `v${APP_VERSION}`);
    }
  } catch (err) {
    const message = toMessage(err);
    error.value = `检查更新失败：${message}`;
    void log.error(`检查更新失败: ${message}`, { component: "UpdateCard" });
  } finally {
    checking.value = false;
  }
}

/**
 * 下载并安装更新。
 *
 * **Windows 上安装器启动后应用会自动退出**——命令不会正常返回，
 * 所以「安装完成」由安装器自己接手。这里的 catch 只会命中
 * 「下载失败 / 验签失败 / 安装器启动失败」这几种真正出错的情况。
 */
async function install(): Promise<void> {
  if (!hasUpdate.value || installing.value) return;
  installing.value = true;
  error.value = "";
  percent.value = 0;
  downloaded.value = 0;
  total.value = 0;

  try {
    await installAppUpdate();

    // 能走到这里说明应用没有退出（例如安装器启动失败）。
    // 给用户一个明确的下一步，而不是让界面停在「安装中」。
    toast.info("安装程序已启动", "安装完成后重新打开启动器即可");
  } catch (err) {
    const message = toMessage(err);
    error.value = `更新失败：${message}`;
    void log.error(`更新失败: ${message}`, { component: "UpdateCard" });
    toast.error("更新失败", message);
  } finally {
    installing.value = false;
    percent.value = null;
  }
}

/** 大小的人类可读格式。 */
function formatSize(bytes: number): string {
  if (bytes <= 0) return "";
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** 进度文字：总量未知时只报已下载量。 */
function progressText(): string {
  if (percent.value === null) return "正在准备…";
  if (total.value <= 0) return `已下载 ${formatSize(downloaded.value)}`;
  return `${formatSize(downloaded.value)} / ${formatSize(total.value)}`;
}
</script>

<template>
  <AppCard title="更新" padding="md">
    <template #icon>
      <ArrowDownToLine :size="16" class="text-brand-500" />
    </template>

    <div class="flex flex-col gap-3">
      <!-- 当前状态 -->
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex min-w-0 items-center gap-2 text-body">
          <span class="font-mono text-ink-muted">v{{ APP_VERSION }}</span>
          <template v-if="hasUpdate">
            <ArrowDownToLine :size="14" class="text-brand-500" />
            <span class="text-brand-500">→</span>
            <span class="font-mono font-medium text-ink">
              v{{ info?.latestVersion }}
            </span>
          </template>
          <span v-else-if="info" class="flex items-center gap-1.5 text-ink-muted">
            <CircleCheck :size="14" class="text-success" />
            已是最新版本
          </span>
        </div>

        <AppButton
          variant="secondary"
          size="sm"
          :loading="checking"
          :disabled="installing"
          @click="check"
        >
          <RefreshCw :size="13" :class="{ 'animate-spin': checking }" />
          检查更新
        </AppButton>
      </div>

      <!-- 更新说明 -->
      <div
        v-if="hasUpdate && info?.notes"
        class="max-h-40 overflow-y-auto whitespace-pre-wrap rounded-md bg-surface-muted px-3 py-2 font-mono text-micro leading-5 text-ink-muted"
      >
        {{ info.notes }}
      </div>

      <!-- 下载进度 -->
      <div v-if="installing" class="flex flex-col gap-1.5">
        <div class="h-1.5 w-full overflow-hidden rounded-full bg-surface-muted">
          <div
            v-if="percent !== null"
            class="h-full rounded-full bg-brand-500 transition-[width] duration-200"
            :style="{ width: `${percent}%` }"
          />
          <div v-else class="h-full w-1/3 animate-pulse rounded-full bg-brand-500/60" />
        </div>
        <div class="flex items-center justify-between text-micro text-ink-muted">
          <span>{{ progressText() }}</span>
          <span v-if="percent !== null" class="tabular">{{ percent }}%</span>
        </div>
      </div>

      <!-- 操作 -->
      <div v-if="hasUpdate && !installing" class="flex items-center gap-2">
        <AppButton variant="primary" size="sm" @click="install">
          <ArrowDownToLine :size="13" />
          下载并安装
        </AppButton>

        <span class="flex items-center gap-1.5 text-micro text-ink-muted">
          <ShieldCheck :size="13" class="text-success" />
          更新包必须通过数字签名校验
        </span>
      </div>

      <!-- 说明 -->
      <p class="text-micro leading-5 text-ink-muted">
        更新包从官方渠道下载，并用内置的 minisign 公钥验签——
        <strong class="font-medium text-ink">签名不匹配的更新包不会被安装</strong>。
      </p>

      <p v-if="error" class="text-caption text-danger">{{ error }}</p>
    </div>
  </AppCard>
</template>
