<script setup lang="ts">
import { computed, ref } from "vue";
import { AlertCircle, Check, CheckCircle2, ListTodo, Loader2, XCircle } from "@lucide/vue";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppEmptyState from "@/components/common/AppEmptyState.vue";
import PageShell from "@/components/layout/PageShell.vue";
import { cancelDownload } from "@/api/resource";
import { useToastStore } from "@/stores/toast";
import { formatBytes, formatRemaining, formatSpeed } from "@/utils/format";
import { useTaskStore } from "@/stores/task";
import type { TaskItem } from "@/stores/task";

const taskStore = useTaskStore();
const toast = useToastStore();
const cancellingId = ref<string | null>(null);

const tasks = computed(() => Object.values(taskStore.tasks));
const active = computed(() => taskStore.activeTask);

async function cancelTask(task: TaskItem): Promise<void> {
  if (cancellingId.value) return;
  cancellingId.value = task.id;
  try {
    const found = await cancelDownload(task.id);
    if (found) {
      taskStore.markCancelled(task.id);
      toast.info("任务已取消", task.name);
    } else {
      toast.warning("任务已经结束", task.name);
    }
  } catch (error) {
    toast.error("取消失败", error instanceof Error ? error.message : String(error));
  } finally {
    cancellingId.value = null;
  }
}

function percentOf(task: TaskItem): number {
  const summary = task.summary;
  if (!summary || summary.totalFiles === 0) return task.status === "done" ? 100 : 0;
  return Math.min(100, Math.floor((summary.doneFiles / summary.totalFiles) * 100));
}

function statusLabel(task: TaskItem): string {
  if (task.status === "done") return "已完成";
  if (task.status === "cancelled") return "已取消";
  if (task.status === "error") return "失败";
  return "进行中";
}

function rowMeta(task: TaskItem): string {
  if (task.status === "cancelled") return "任务已取消";
  if (task.status === "error") return task.errorMsg ?? "任务失败";
  if (task.status === "done") return "任务已完成";
  const summary = task.summary;
  if (!summary) return "准备中…";
  return `${formatBytes(summary.downloadedBytes)} / ${formatBytes(summary.totalBytes)}`;
}

/**
 * 预计剩余时间。
 *
 * 只说「2.4 MB/s」是不够的——用户真正想知道的是「还要等多久」。
 * 没有速度或没有总量时返回 `—`，不要显示「0 秒」这种假信息。
 */
function remainingLabel(task: TaskItem): string {
  const summary = task.summary;
  if (!summary || task.speedBps <= 0) return "—";
  const remaining = summary.totalBytes - summary.downloadedBytes;
  if (remaining <= 0) return "—";
  return formatRemaining((remaining / task.speedBps) * 1000);
}

async function retryTask(task: TaskItem): Promise<void> {
  if (!task.retry) return;
  await task.retry();
}
</script>

<template>
  <PageShell
    title="任务"
    :description="
      taskStore.runningCount > 0 ? `${taskStore.runningCount} 个任务进行中` : '下载与安装进度'
    "
    :icon="ListTodo"
    width="lg"
  >
    <template #actions>
      <AppButton
        v-if="tasks.some((task) => task.status !== 'running')"
        variant="secondary"
        size="sm"
        @click="taskStore.clearFinished()"
      >
        清理已完成
      </AppButton>
    </template>

    <!-- 完全没有任务时：一个居中的空状态就够，不要左右两个「各自为空的框」 -->
    <div
      v-if="tasks.length === 0"
      class="flex min-h-0 flex-1 items-center justify-center rounded-lg border border-line bg-surface"
    >
      <AppEmptyState
        :icon="ListTodo"
        title="还没有任务"
        description="安装游戏或下载整合包时，进度会出现在这里，可以随时离开这个页面。"
        action-label="去下载版本"
        @action="$router.push('/downloads')"
      />
    </div>

    <div v-else class="grid min-h-0 flex-1 gap-4 md:grid-cols-[320px_1fr]">
      <!-- 任务列表 -->
      <div class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-line bg-surface">
        <div class="min-h-0 flex-1 overflow-y-auto">
          <div class="flex flex-col divide-y divide-line/50">
            <button
              v-for="task in tasks"
              :key="task.id"
              type="button"
              class="flex flex-col gap-2 px-3.5 py-3 text-left transition-fast"
              :class="
                taskStore.activeId === task.id
                  ? 'bg-brand-500/8'
                  : 'hover:bg-surface-muted/70'
              "
              @click="taskStore.setActive(task.id)"
            >
              <div class="flex items-center gap-2">
                <Loader2
                  v-if="task.status === 'running'"
                  :size="14"
                  class="shrink-0 animate-spin text-brand-500"
                />
                <Check v-else-if="task.status === 'done'" :size="14" class="shrink-0 text-success" />
                <XCircle
                  v-else
                  :size="14"
                  class="shrink-0"
                  :class="task.status === 'cancelled' ? 'text-warning' : 'text-danger'"
                />

                <span class="min-w-0 flex-1 truncate text-caption font-medium text-ink">
                  {{ task.name }}
                </span>
                <span class="shrink-0 font-mono text-micro text-ink-muted tabular">
                  {{ percentOf(task) }}%
                </span>
              </div>

              <div class="h-1.5 overflow-hidden rounded-round bg-ink/10">
                <div
                  class="h-full rounded-round transition-all duration-300"
                  :class="[
                    task.status === 'error'
                      ? 'bg-danger'
                      : task.status === 'cancelled'
                        ? 'bg-warning'
                        : task.status === 'done'
                          ? 'bg-success'
                          : 'bg-brand-500',
                    task.status === 'running' ? 'progress-shine' : '',
                  ]"
                  :style="{ width: `${percentOf(task)}%` }"
                />
              </div>

              <div class="truncate text-micro text-ink-muted">{{ rowMeta(task) }}</div>
            </button>
          </div>
        </div>
      </div>

      <!-- 任务详情 -->
      <div class="min-h-0 overflow-y-auto">
        <AppCard v-if="!active" padding="lg" class="h-full">
          <AppEmptyState
            :icon="ListTodo"
            title="选择一个任务查看详情"
            description="左侧列表里点任意一条，就能看到速度、剩余时间与文件级进度。"
          />
        </AppCard>

        <AppCard v-else padding="lg" class="flex flex-col gap-5">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <AppBadge
                :variant="
                  active.status === 'running'
                    ? 'brand'
                    : active.status === 'done'
                      ? 'success'
                      : active.status === 'cancelled'
                        ? 'warning'
                        : 'danger'
                "
                size="sm"
              >
                {{ statusLabel(active) }}
              </AppBadge>
              <h2 class="mt-2 truncate font-display text-h1 font-semibold text-ink">
                {{ active.name }}
              </h2>
            </div>

            <div class="flex flex-col items-end gap-2 text-right">
              <div class="metric text-brand-500">
                {{ percentOf(active) }}<span class="text-h1">%</span>
              </div>
              <div class="text-micro text-ink-muted">
                {{ formatBytes(active.summary?.downloadedBytes ?? 0) }} /
                {{ formatBytes(active.summary?.totalBytes ?? 0) }}
              </div>
              <AppButton
                v-if="active.status === 'running' && active.cancellable"
                variant="ghost"
                size="sm"
                :loading="cancellingId === active.id"
                @click="cancelTask(active)"
              >
                取消任务
              </AppButton>
            </div>
          </div>

          <div class="h-3 overflow-hidden rounded-round bg-ink/10">
            <div
              class="h-full rounded-round transition-all duration-300"
              :class="[
                active.status === 'error'
                  ? 'bg-danger'
                  : active.status === 'cancelled'
                    ? 'bg-warning'
                    : active.status === 'done'
                      ? 'bg-success'
                      : 'bg-brand-500',
                active.status === 'running' ? 'progress-shine' : '',
              ]"
              :style="{ width: `${percentOf(active)}%` }"
            />
          </div>

          <div class="grid grid-cols-3 gap-3">
            <div class="rounded-md bg-surface-muted px-3 py-3">
              <div class="font-mono text-h1 font-semibold text-ink tabular">
                {{ formatSpeed(active.speedBps) }}
              </div>
              <div class="mt-1 text-micro text-ink-muted">实时速度</div>
            </div>
            <div class="rounded-md bg-surface-muted px-3 py-3">
              <div class="font-mono text-h1 font-semibold text-ink tabular">
                {{ remainingLabel(active) }}
              </div>
              <div class="mt-1 text-micro text-ink-muted">预计剩余</div>
            </div>
            <div class="rounded-md bg-surface-muted px-3 py-3">
              <div class="font-mono text-h1 font-semibold text-ink tabular">
                {{ active.summary?.doneFiles ?? 0 }}/{{ active.summary?.totalFiles ?? 0 }}
              </div>
              <div class="mt-1 text-micro text-ink-muted">已完成文件</div>
            </div>
          </div>

          <div
            v-if="active.status === 'running' && active.currentFile"
            class="rounded-md border border-line/60 bg-surface-muted px-3 py-2.5"
          >
            <div class="mb-1 text-micro text-ink-muted">正在下载</div>
            <div class="truncate text-caption text-ink">{{ active.currentFile.fileName }}</div>
          </div>

          <div
            v-if="active.status === 'error'"
            class="flex items-start gap-2.5 rounded-md border border-danger/25 bg-danger-soft px-3 py-2.5 text-danger"
          >
            <AlertCircle :size="16" class="mt-0.5 shrink-0" />
            <div class="min-w-0 text-caption leading-5">
              <div class="font-medium">任务失败</div>
              <div class="mt-0.5 opacity-90">{{ active.errorMsg }}</div>
              <AppButton
                v-if="active.retry"
                variant="secondary"
                size="xs"
                class="mt-2"
                @click="retryTask(active)"
              >
                重试
              </AppButton>
            </div>
          </div>

          <div
            v-else-if="active.status === 'cancelled'"
            class="flex items-center gap-2 rounded-md border border-warning/20 bg-warning-soft px-3 py-2.5 text-caption text-warning"
          >
            <XCircle :size="15" />
            任务已取消，回到对应页面重新发起即可（已下载的分片会续传）。
          </div>

          <div
            v-else-if="active.status === 'done'"
            class="flex items-center gap-2 rounded-md border border-success/20 bg-success-soft px-3 py-2.5 text-caption text-success"
          >
            <CheckCircle2 :size="15" />
            任务已完成，可以关闭这个页面。
          </div>

          <p v-else class="text-micro text-ink-muted">
            可以离开此页面，任务会在后台继续进行。
          </p>
        </AppCard>
      </div>
    </div>
  </PageShell>
</template>

<style scoped>
/* 关键数值：等宽 + 大字号，让「速度 / 剩余 / 百分比」成为详情区的视觉锚点 */
.metric {
  font-family: var(--font-mono);
  font-size: var(--text-metric);
  line-height: var(--text-metric--line-height);
  letter-spacing: var(--text-metric--letter-spacing);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.metric :deep(span) {
  font-size: var(--text-h1);
  font-weight: 500;
}
</style>
