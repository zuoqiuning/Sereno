<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ArrowDownToLine, CircleCheck, RefreshCw, ShieldCheck } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppSwitch from "@/components/common/AppSwitch.vue";
import SettingsRow from "@/components/common/SettingsRow.vue";
import PageShell from "@/components/layout/PageShell.vue";
import {
  checkAppUpdate,
  getUpdateStatus,
  installAppUpdate,
  onUpdateProgress,
  setUpdateAutoCheck,
} from "@/api/update";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import type { AppUpdateInfo, UpdateStatus } from "@/types";

/**
 * 更新页。
 *
 * 三件事：
 * 1. **自动检查**——启动时查一次（可在设置里关掉）；
 * 2. **手动检查**——随时点一下；
 * 3. **下载安装**——带进度，验签在 Rust 侧完成。
 *
 * 下载与安装是一体的（插件内部先验签再启动安装器），
 * 因此这里只呈现「下载中」与「安装中」两种进行态。
 */
const toast = useToastStore();

const status = ref<UpdateStatus | null>(null);
const info = ref<AppUpdateInfo | null>(null);
const checking = ref(false);
const installing = ref(false);
/** 下载进度（0–100）；总量未知时为 null，走不确定态动画。 */
const percent = ref<number | null>(null);
const downloaded = ref(0);
const total = ref(0);
const error = ref("");

const hasUpdate = computed(() => info.value?.available ?? false);
/** 自动检查开着、且还没查过 —— 页面载入时补一次检查。 */
const needInitialCheck = computed(
  () => status.value?.autoCheck === true && info.value === null,
);

let stopProgress: (() => void) | null = null;

onMounted(async () => {
  stopProgress = onUpdateProgress((progress) => {
    downloaded.value = progress.downloaded;
    total.value = progress.total;
    percent.value = progress.percent;
  });

  try {
    status.value = await getUpdateStatus();
  } catch (err) {
    error.value = `读取更新状态失败：${toMessage(err)}`;
  }
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

/** 上次检查时间的友好显示。 */
function lastCheckedText(): string {
  const at = status.value?.lastCheckedAt;
  if (!at) return "还没有检查过";
  const date = new Date(at * 1000);
  const now = Date.now();
  const diff = now - at * 1000;
  if (diff < 60_000) return "刚刚检查过";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前检查过`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前检查过`;
  const pad = (n: number) => `${n}`.padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} 检查过`;
}

/** 手动检查更新。 */
async function check(silent = false): Promise<void> {
  if (checking.value) return;
  checking.value = true;
  error.value = "";
  try {
    const result = await checkAppUpdate();
    info.value = result;
    // 顺手刷新「上次检查时间」
    status.value = await getUpdateStatus().catch(() => status.value);

    if (result.available) {
      if (!silent) {
        toast.success(
          `发现新版本 v${result.latestVersion}`,
          `当前版本 v${result.currentVersion}`,
        );
      }
    } else if (!silent) {
      toast.success("已是最新版本", `v${result.currentVersion}`);
    }
  } catch (err) {
    const message = toMessage(err);
    error.value = `检查更新失败：${message}`;
    if (!silent) {
      toast.error("检查更新失败", message);
      void log.error(`检查更新失败: ${message}`, { component: "UpdateView" });
    }
  } finally {
    checking.value = false;
  }
}

/** 切换「启动时自动检查」。 */
async function toggleAutoCheck(value: boolean): Promise<void> {
  const previous = status.value?.autoCheck ?? true;
  status.value = status.value ? { ...status.value, autoCheck: value } : null;
  try {
    await setUpdateAutoCheck(value);
    toast.success(value ? "已开启自动检查" : "已关闭自动检查");
    // 刚开启时立刻查一次，省得用户还要再点一下
    if (value) void check(true);
  } catch (err) {
    status.value = status.value ? { ...status.value, autoCheck: previous } : null;
    toast.error("保存设置失败", toMessage(err));
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
    toast.info("安装程序已启动", "安装完成后重新打开启动器即可");
  } catch (err) {
    const message = toMessage(err);
    error.value = `更新失败：${message}`;
    toast.error("更新失败", message);
    void log.error(`更新失败: ${message}`, { component: "UpdateView" });
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

// 自动检查开着才在载入时补一次检查
void (async () => {
  await new Promise((resolve) => setTimeout(resolve, 400));
  if (needInitialCheck.value) void check(true);
})();
</script>

<template>
  <PageShell title="更新" description="检查并安装启动器的新版本" width="md">
    <div class="flex flex-col gap-4">
      <!-- 当前版本 -->
      <AppCard title="当前版本" padding="lg">
        <template #icon>
          <RefreshCw :size="16" class="text-brand-500" />
        </template>

        <div class="flex flex-wrap items-center justify-between gap-4">
          <div class="flex min-w-0 items-center gap-2.5">
            <span class="font-mono text-title text-ink">
              v{{ status?.currentVersion ?? "—" }}
            </span>
            <template v-if="hasUpdate">
              <span class="text-ink-muted">→</span>
              <span class="font-mono text-title text-brand-600">
                v{{ info?.latestVersion }}
              </span>
              <span
                class="rounded-xs bg-brand-soft px-1.5 py-0.5 text-micro text-brand-600"
              >
                有新版本
              </span>
            </template>
            <span
              v-else-if="info"
              class="flex items-center gap-1.5 text-caption text-success"
            >
              <CircleCheck :size="14" />
              已是最新版本
            </span>
          </div>

          <AppButton
            variant="secondary"
            size="sm"
            :loading="checking"
            :disabled="installing"
            @click="check(false)"
          >
            <RefreshCw :size="13" :class="{ 'animate-spin': checking }" />
            检查更新
          </AppButton>
        </div>

        <p class="mt-3 text-micro text-ink-muted">{{ lastCheckedText() }}</p>
      </AppCard>

      <!-- 自动检查 -->
      <AppCard title="自动检查" padding="lg">
        <SettingsRow
          title="启动时自动检查更新"
          description="启动器打开时自动检查一次有新版本。只发一个请求，不影响启动速度。"
        >
          <AppSwitch
            :model-value="status?.autoCheck ?? true"
            label="启动时自动检查更新"
            @update:model-value="toggleAutoCheck"
          />
        </SettingsRow>
      </AppCard>

      <!-- 更新说明 -->
      <AppCard v-if="hasUpdate && info?.notes" title="更新说明" padding="lg">
        <div
          class="max-h-64 overflow-y-auto whitespace-pre-wrap rounded-md bg-surface-muted px-3.5 py-3 font-mono text-caption leading-6 text-ink-soft"
        >
          {{ info.notes }}
        </div>
      </AppCard>

      <!-- 下载与安装 -->
      <AppCard v-if="hasUpdate" title="下载并安装" padding="lg">
        <div class="flex flex-col gap-4">
          <!-- 进度 -->
          <div v-if="installing" class="flex flex-col gap-2">
            <div class="h-2 w-full overflow-hidden rounded-full bg-surface-muted">
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
          <div v-else class="flex items-center gap-3">
            <AppButton variant="primary" size="sm" @click="install">
              <ArrowDownToLine :size="13" />
              下载并安装
            </AppButton>
            <span class="flex items-center gap-1.5 text-micro text-ink-muted">
              <ShieldCheck :size="13" class="text-success" />
              更新包必须通过数字签名校验
            </span>
          </div>
        </div>
      </AppCard>

      <p v-if="error" class="text-caption text-danger">{{ error }}</p>
    </div>
  </PageShell>
</template>
