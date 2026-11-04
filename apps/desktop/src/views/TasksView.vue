<script setup lang="ts">
import { computed, ref } from "vue";
import { AlertCircle, CheckCircle2, FileDown, ListTodo, Loader2, XCircle } from "@lucide/vue";

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

/** 进度条配色：按任务状态。 */
function barClass(task: TaskItem): string {
  if (task.status === "error") return "bg-danger";
  if (task.status === "cancelled") return "bg-warning";
  if (task.status === "done") return "bg-success";
  return "bg-brand-500";
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

    <!-- 完全没有任务：一个大空状态就够，不要左右两个空框 -->
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

    <div v-else class="grid min-h-0 flex-1 gap-4 md:grid-cols-[340px_1fr]">
      <!-- ============ 左侧：任务列表（卡片式） ============ -->
      <div class="flex min-h-0 flex-col gap-2 overflow-y-auto pr-1">
        <button
          v-for="task in tasks"
          :key="task.id"
          type="button"
          class="task-card"
          :class="taskStore.activeId === task.id ? 'is-active' : ''"
          @click="taskStore.setActive(task.id)"
        >
          <div class="flex items-center gap-2">
            <Loader2
              v-if="task.status === 'running'"
              :size="14"
              class="shrink-0 animate-spin text-brand-500"
            />
            <CheckCircle2
              v-else-if="task.status === 'done'"
              :size="14"
              class="shrink-0 text-success"
            />
            <XCircle
              v-else-if="task.status === 'cancelled'"
              :size="14"
              class="shrink-0 text-warning"
            />
            <AlertCircle v-else :size="14" class="shrink-0 text-danger" />

            <span class="min-w-0 flex-1 truncate text-body font-semibold text-ink">
              {{ task.name }}
            </span>
            <span class="shrink-0 font-mono text-caption font-medium text-ink-muted tabular">
              {{ percentOf(task) }}%
            </span>
          </div>

          <div class="h-1.5 overflow-hidden rounded-round bg-ink/10">
            <div
              class="h-full rounded-round transition-all duration-300"
              :class="[barClass(task), task.status === 'running' ? 'progress-stripe' : '']"
              :style="{ width: `${percentOf(task)}%` }"
            />
          </div>

          <div class="flex items-center justify-between gap-2 text-micro text-ink-muted tabular">
            <span class="truncate">{{ rowMeta(task) }}</span>
            <span v-if="task.status === 'running'" class="shrink-0">
              {{ task.summary?.doneFiles ?? 0 }}/{{ task.summary?.totalFiles ?? 0 }} 文件
            </span>
          </div>
        </button>
      </div>

      <!-- ============ 右侧：任务详情（仪表盘） ============ -->
      <div class="flex min-h-0 flex-col overflow-y-auto">
        <AppCard v-if="!active" padding="lg" class="flex h-full items-center justify-center">
          <AppEmptyState
            :icon="ListTodo"
            title="选择一个任务查看详情"
            description="左侧列表里点任意一条，就能看到速度、剩余时间与文件级进度。"
          />
        </AppCard>

        <AppCard v-else padding="lg" class="flex h-full flex-col gap-5">
          <!-- 标题 + 大百分比 -->
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
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
                <h2 class="truncate font-display text-h1 font-semibold text-ink">
                  {{ active.name }}
                </h2>
              </div>
              <p class="mt-1.5 font-mono text-caption text-ink-muted tabular">
                已下载 {{ formatBytes(active.summary?.downloadedBytes ?? 0) }} /
                {{ formatBytes(active.summary?.totalBytes ?? 0) }}
              </p>
            </div>

            <div class="shrink-0 text-right">
              <div class="text-brand-500">
                <template v-if="active.status === 'running' && percentOf(active) >= 100">
                  <span class="text-h3 font-semibold">安装器运行中…</span>
                </template>
                <template v-else>{{ percentOf(active) }}<span>%</span></template>
              </div>
              <div
                v-if="active.status === 'running' && percentOf(active) >= 100"
                class="text-micro text-ink-muted"
              >
                安装文件已就绪，正在执行安装
              </div>
            </div>
          </div>

          <!-- 粗壮的主进度条 -->
          <div class="h-3 overflow-hidden rounded-round bg-ink/10">
            <div
              class="h-full rounded-round transition-all duration-300"
              :class="[barClass(active), active.status === 'running' ? 'progress-stripe' : '']"
              :style="{ width: `${percentOf(active)}%` }"
            />
          </div>

          <!-- 仪表盘数据区：完成后速度/剩余毫无意义（0 B/s、-），只留文件数 -->
          <div class="grid gap-3" :class="active.status === 'done' ? 'grid-cols-1' : 'grid-cols-3'">
            <div v-if="active.status !== 'done'" class="metric-tile">
              <span class="text-micro text-ink-muted">实时速度</span>
              <div class="mt-1 font-mono text-h1 font-semibold text-ink tabular">
                {{ formatSpeed(active.speedBps) }}
              </div>
            </div>
            <div v-if="active.status !== 'done'" class="metric-tile">
              <span class="text-micro text-ink-muted">预计剩余</span>
              <div class="mt-1 font-mono text-h1 font-semibold text-ink tabular">
                {{ remainingLabel(active) }}
              </div>
            </div>
            <div class="metric-tile">
              <span class="text-micro text-ink-muted">已完成文件</span>
              <div class="mt-1 flex items-baseline gap-1">
                <span class="font-mono text-h1 font-semibold text-ink tabular">
                  {{ active.summary?.doneFiles ?? 0 }}
                </span>
                <span class="font-mono text-caption text-ink-muted">
                  / {{ active.summary?.totalFiles ?? 0 }}
                </span>
              </div>
            </div>
          </div>

          <!-- 底部：当前文件与状态 -->
          <div class="mt-auto flex flex-col gap-3 pt-1">
            <div
              v-if="active.status === 'running' && active.currentFile"
              class="flex items-center gap-3 rounded-md border border-line bg-surface-muted px-4 py-3"
            >
              <div
                class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-surface text-brand-500 shadow-sm"
              >
                <FileDown :size="16" />
              </div>
              <div class="min-w-0 flex-1">
                <p class="text-micro font-medium text-brand-600">正在下载</p>
                <p class="truncate text-body text-ink" :title="active.currentFile.fileName">
                  {{ active.currentFile.fileName }}
                </p>
              </div>
            </div>

            <div
              v-else-if="active.status === 'error'"
              class="flex items-start gap-3 rounded-md bg-danger-soft px-4 py-3 text-danger"
            >
              <AlertCircle :size="16" class="mt-0.5 shrink-0" />
              <div class="min-w-0 flex-1 text-caption leading-5">
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
              class="flex items-center gap-2 rounded-md bg-warning-soft px-4 py-3 text-caption text-warning"
            >
              <XCircle :size="15" class="shrink-0" />
              任务已取消，回到对应页面重新发起即可（已下载的分片会续传）。
            </div>

            <p
              v-else-if="active.status === 'done'"
              class="flex items-center gap-1.5 text-caption text-success"
            >
              <CheckCircle2 :size="14" class="shrink-0" />
              安装完成：{{ active.summary?.doneFiles ?? 0 }} 个文件 ·
              {{ formatBytes(active.summary?.downloadedBytes ?? 0) }}——可以离开此页。
            </p>

            <p v-else class="text-micro text-ink-muted">可以离开此页面，任务会在后台继续进行。</p>

            <div v-if="active.status === 'running'" class="flex justify-end">
              <AppButton
                variant="ghost"
                size="sm"
                :loading="cancellingId === active.id"
                @click="cancelTask(active)"
              >
                取消任务
              </AppButton>
            </div>
          </div>
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

/* 仪表盘数据块 */
.metric-tile {
  display: flex;
  flex-direction: column;
  border-radius: var(--radius-md);
  background: var(--color-surface-muted);
  padding: 12px 14px;
}

/* 左侧任务卡片：白底 + 细边 + 柔和阴影，选中时给一圈品牌色 */
.task-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  border: 1px solid color-mix(in srgb, var(--color-line) 85%, transparent);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  padding: 12px 14px;
  text-align: left;
  box-shadow: var(--shadow-sm);
  transition:
    border-color var(--duration-fast) var(--ease-out-soft),
    box-shadow var(--duration-base) var(--ease-out-soft),
    transform var(--duration-base) var(--ease-out-soft);
}

.task-card:hover {
  border-color: color-mix(in srgb, var(--color-brand-500) 30%, transparent);
  box-shadow: var(--shadow-md);
  transform: translateY(calc(var(--shift-xs) * -1));
}

.task-card.is-active {
  border-color: color-mix(in srgb, var(--color-brand-500) 45%, transparent);
  box-shadow:
    0 0 0 1px color-mix(in srgb, var(--color-brand-500) 18%, transparent),
    var(--shadow-md);
}
</style>
