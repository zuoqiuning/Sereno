<script setup lang="ts">
import { computed } from "vue";
import { AlertCircle, Check, CheckCircle2, ListTodo, Loader2, XCircle } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import { formatBytes, formatSpeed } from "@/utils/format";
import { useTaskStore } from "@/stores/task";
import type { TaskItem } from "@/stores/task";

const taskStore = useTaskStore();

const tasks = computed(() => Object.values(taskStore.tasks));
const active = computed(() => taskStore.activeTask);

function percentOf(task: TaskItem): number {
  const summary = task.summary;
  if (!summary || summary.totalFiles === 0) return task.status === "done" ? 100 : 0;
  return Math.min(100, Math.floor((summary.doneFiles / summary.totalFiles) * 100));
}

function statusLabel(task: TaskItem): string {
  if (task.status === "done") return "已完成";
  if (task.status === "error") return "失败";
  return "进行中";
}

function rowMeta(task: TaskItem): string {
  if (task.status === "error") return task.errorMsg ?? "任务失败";
  if (task.status === "done") return "任务已完成";
  const summary = task.summary;
  if (!summary) return "准备中…";
  return `${formatBytes(summary.downloadedBytes)} / ${formatBytes(summary.totalBytes)}`;
}
</script>

<template>
  <div class="mx-auto flex h-full w-full max-w-5xl flex-col px-10 pt-8 pb-10">
    <header class="flex shrink-0 items-center justify-between">
      <div class="flex items-center gap-3">
        <ListTodo :size="22" class="text-brand-500" />
        <h1 class="font-display text-2xl font-semibold text-ink">任务</h1>
        <span
          v-if="taskStore.runningCount > 0"
          class="flex items-center gap-1.5 rounded-full bg-brand-500/10 px-2 py-0.5 text-[11px] font-medium text-brand-600"
        >
          <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-brand-500" />
          {{ taskStore.runningCount }} 个进行中
        </span>
      </div>

      <AppButton
        v-if="tasks.some((task) => task.status !== 'running')"
        variant="secondary"
        size="sm"
        @click="taskStore.clearFinished()"
      >
        清理已完成
      </AppButton>
    </header>

    <div class="mt-6 grid min-h-0 flex-1 gap-4 md:grid-cols-[340px_1fr]">
      <!-- 任务列表 -->
      <div class="flex min-h-0 flex-col gap-2 overflow-y-auto pr-1">
        <div
          v-if="tasks.length === 0"
          class="flex h-full flex-col items-center justify-center rounded-xl border border-dashed border-line/70 bg-glass/30 text-ink-muted"
        >
          <ListTodo :size="36" class="mb-3 opacity-25" />
          <p class="text-[13px]">还没有任务</p>
          <p class="mt-1 text-[11px]">安装游戏或下载整合包时会在这里显示进度</p>
        </div>

        <button
          v-for="task in tasks"
          :key="task.id"
          type="button"
          class="group flex flex-col gap-2.5 rounded-lg border px-3.5 py-3 text-left transition-colors"
          :class="
            taskStore.activeId === task.id
              ? 'border-brand-500/35 bg-brand-500/[0.06]'
              : 'border-line/50 bg-surface/45 hover:border-line hover:bg-surface/70'
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
            <XCircle v-else :size="14" class="shrink-0 text-danger" />

            <span class="min-w-0 flex-1 truncate text-[13px] font-medium text-ink">
              {{ task.name }}
            </span>
            <span class="shrink-0 font-mono text-[11px] tabular-nums text-ink-muted">
              {{ percentOf(task) }}%
            </span>
          </div>

          <div class="h-1 overflow-hidden rounded-full bg-ink/5">
            <div
              class="h-full rounded-full transition-all duration-300"
              :class="
                task.status === 'error'
                  ? 'bg-danger'
                  : task.status === 'done'
                    ? 'bg-success'
                    : 'bg-brand-500'
              "
              :style="{ width: `${percentOf(task)}%` }"
            />
          </div>

          <div class="truncate text-[11px] text-ink-muted">{{ rowMeta(task) }}</div>
        </button>
      </div>

      <!-- 任务详情 -->
      <div class="min-h-0 overflow-y-auto">
        <div
          v-if="!active"
          class="flex h-full min-h-[320px] flex-col items-center justify-center rounded-xl border border-dashed border-line/70 bg-glass/30 text-ink-muted"
        >
          <p class="text-[13px]">选择一个任务查看详情</p>
        </div>

        <div v-else class="sereno-glass flex flex-col gap-5 rounded-xl p-6">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span
                  class="rounded-md px-1.5 py-0.5 text-[10px] font-medium"
                  :class="
                    active.status === 'running'
                      ? 'bg-brand-500/10 text-brand-600'
                      : active.status === 'done'
                        ? 'bg-success-soft text-success'
                        : 'bg-danger-soft text-danger'
                  "
                >
                  {{ statusLabel(active) }}
                </span>
              </div>
              <h2 class="mt-2 truncate font-display text-xl font-semibold text-ink">
                {{ active.name }}
              </h2>
            </div>

            <div class="text-right">
              <div class="font-mono text-3xl font-light leading-none text-brand-500">
                {{ percentOf(active) }}<span class="text-lg">%</span>
              </div>
              <div class="mt-1.5 text-[11px] text-ink-muted">
                {{ formatBytes(active.summary?.downloadedBytes ?? 0) }} /
                {{ formatBytes(active.summary?.totalBytes ?? 0) }}
              </div>
            </div>
          </div>

          <div class="h-2 overflow-hidden rounded-full bg-ink/5">
            <div
              class="h-full rounded-full transition-all duration-300"
              :class="
                active.status === 'error'
                  ? 'bg-danger'
                  : active.status === 'done'
                    ? 'bg-success'
                    : 'bg-brand-500'
              "
              :style="{ width: `${percentOf(active)}%` }"
            />
          </div>

          <div class="grid grid-cols-3 gap-3">
            <div class="rounded-lg bg-surface/50 px-3 py-2.5">
              <div class="font-mono text-[15px] text-ink">
                {{ formatSpeed(active.speedBps) }}
              </div>
              <div class="mt-1 text-[11px] text-ink-muted">实时速度</div>
            </div>
            <div class="rounded-lg bg-surface/50 px-3 py-2.5">
              <div class="font-mono text-[15px] text-ink">
                {{
                  Math.max(
                    0,
                    (active.summary?.totalFiles ?? 0) - (active.summary?.doneFiles ?? 0),
                  )
                }}
              </div>
              <div class="mt-1 text-[11px] text-ink-muted">剩余文件</div>
            </div>
            <div class="rounded-lg bg-surface/50 px-3 py-2.5">
              <div class="font-mono text-[15px] text-ink">
                {{ active.summary?.doneFiles ?? 0 }}/{{ active.summary?.totalFiles ?? 0 }}
              </div>
              <div class="mt-1 text-[11px] text-ink-muted">已完成文件</div>
            </div>
          </div>

          <div
            v-if="active.status === 'running' && active.currentFile"
            class="rounded-lg border border-line/50 bg-surface/50 px-3 py-2.5"
          >
            <div class="mb-1 text-[11px] text-ink-muted">正在下载</div>
            <div class="truncate text-[12px] text-ink">{{ active.currentFile.fileName }}</div>
          </div>

          <div
            v-if="active.status === 'error'"
            class="flex items-start gap-2.5 rounded-lg border border-danger/25 bg-danger-soft px-3 py-2.5 text-danger"
          >
            <AlertCircle :size="16" class="mt-0.5 shrink-0" />
            <div class="min-w-0 text-[12px] leading-5">
              <div class="font-medium">任务失败</div>
              <div class="mt-0.5 opacity-90">{{ active.errorMsg }}</div>
            </div>
          </div>

          <div
            v-else-if="active.status === 'done'"
            class="flex items-center gap-2 rounded-lg border border-success/20 bg-success-soft px-3 py-2.5 text-[12px] text-success"
          >
            <CheckCircle2 :size="15" />
            任务已完成，可以关闭这个页面。
          </div>

          <p v-else class="text-[11px] text-ink-muted">
            可以离开此页面，任务会在后台继续进行。
          </p>
        </div>
      </div>
    </div>
  </div>
</template>
