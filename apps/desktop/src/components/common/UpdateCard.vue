<script setup lang="ts">
import { computed, ref } from "vue";
import { ArrowDownToLine, CircleCheck, RefreshCw, ShieldCheck } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import { checkAppUpdate, downloadAppUpdate, launchAppUpdater } from "@/api/update";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import { APP_VERSION } from "@/constants/app";
import type { UpdateInfo } from "@/types";

/**
 * 更新检查。
 *
 * 走的是项目**已经建立的分发信任链**：从官方 Release 下载安装包，
 * 用随包发布的 `SHA256SUMS.txt` 校验 SHA-256，对不上就拒绝执行。
 */
const toast = useToastStore();

const checking = ref(false);
const downloading = ref(false);
const launching = ref(false);
const info = ref<UpdateInfo | null>(null);
const error = ref("");
/** 校验通过的安装包路径；为空表示还没下载成功。 */
const readyPath = ref("");

const hasUpdate = computed(() => info.value?.available ?? false);

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

async function download(): Promise<void> {
  if (!info.value?.available || downloading.value) return;
  downloading.value = true;
  error.value = "";
  readyPath.value = "";
  try {
    const result = await downloadAppUpdate(info.value);
    readyPath.value = result.path;
    toast.success(
      "更新包已下载并通过校验",
      `SHA-256 ${result.sha256.slice(0, 12)}…`,
    );
  } catch (err) {
    const message = toMessage(err);
    error.value = `下载失败：${message}`;
    void log.error(`下载更新失败: ${message}`, { component: "UpdateCard" });
  } finally {
    downloading.value = false;
  }
}

async function install(): Promise<void> {
  if (!readyPath.value || launching.value) return;
  launching.value = true;
  try {
    await launchAppUpdater(readyPath.value);
    toast.success("已启动安装程序", "安装完成后重新打开启动器即可");
  } catch (err) {
    const message = toMessage(err);
    error.value = `启动安装程序失败：${message}`;
    void log.error(`启动安装程序失败: ${message}`, { component: "UpdateCard" });
  } finally {
    launching.value = false;
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
            <span class="font-mono font-medium text-ink">v{{ info?.latestVersion }}</span>
            <span
              v-if="info?.prerelease"
              class="rounded-xs bg-warning-soft px-1.5 py-0.5 text-micro text-warning"
            >
              预发布
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
          :disabled="downloading || launching"
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

      <!-- 安装包信息 -->
      <div v-if="hasUpdate && info?.assetName" class="flex items-center gap-2 text-micro text-ink-muted">
        <span class="truncate font-mono">{{ info.assetName }}</span>
        <span v-if="info.assetSize > 0">· {{ formatSize(info.assetSize) }}</span>
      </div>

      <!-- 操作 -->
      <div v-if="hasUpdate" class="flex items-center gap-2">
        <AppButton
          v-if="!readyPath"
          variant="primary"
          size="sm"
          :loading="downloading"
          :disabled="checking"
          @click="download"
        >
          <ArrowDownToLine :size="13" />
          下载更新
        </AppButton>
        <AppButton
          v-else
          variant="primary"
          size="sm"
          :loading="launching"
          @click="install"
        >
          <ArrowDownToLine :size="13" />
          启动安装程序
        </AppButton>

        <span
          v-if="readyPath"
          class="flex items-center gap-1.5 text-micro text-success"
        >
          <ShieldCheck :size="13" />
          已通过 SHA-256 校验
        </span>
      </div>

      <!-- 说明 -->
      <p class="text-micro leading-5 text-ink-muted">
        更新包从官方 GitHub Releases 下载，并与随包发布的
        <code class="font-mono">SHA256SUMS.txt</code> 逐字节核对；
        校验不通过则拒绝安装。
      </p>

      <p v-if="error" class="text-caption text-danger">{{ error }}</p>
    </div>
  </AppCard>
</template>
