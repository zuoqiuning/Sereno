<script setup lang="ts">
import { computed } from "vue";
import { AlertCircle, Check, CheckCircle2, Loader2, Package, Play, XCircle } from "@lucide/vue";
import { useTaskStore } from "@/stores/task";
import type { TaskItem } from "@/stores/task";
import AppBadge from "@/components/common/AppBadge.vue";

const taskStore = useTaskStore();

const task = computed(() => taskStore.activeTask);

const totalPercent = computed(() => {
  if (!task.value?.summary || task.value.summary.totalFiles === 0) return 0;
  return Math.min(
    100,
    Math.floor((task.value.summary.doneFiles / task.value.summary.totalFiles) * 100),
  );
});

const remainingFiles = computed(() => {
  if (!task.value?.summary) return 0;
  return Math.max(0, task.value.summary.totalFiles - task.value.summary.doneFiles);
});

const speedText = computed(() => {
  if (!task.value || task.value.speedBps <= 0 || task.value.summary?.stage === 3) return "0 KB/s";
  const mb = task.value.speedBps / 1024 / 1024;
  if (mb < 1) return `${(task.value.speedBps / 1024).toFixed(0)} KB/s`;
  return `${mb.toFixed(1)} MB/s`;
});

function formatMB(bytes?: number): string {
  if (!bytes || bytes <= 0) return "0 MB";
  const mb = bytes / 1024 / 1024;
  if (mb < 1) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${mb.toFixed(1)} MB`;
}

function getTaskPercent(t: TaskItem): number {
  if (!t.summary || t.summary.totalFiles === 0) return 0;
  return Math.min(100, Math.floor((t.summary.doneFiles / t.summary.totalFiles) * 100));
}

const steps = computed(() => {
  if (!task.value) return [];
  const s = task.value.summary;
  const stage = s?.stage || 1;
  const isDone = task.value.status === "done";
  const isError = task.value.status === "error";

  let step2Status: "pending" | "active" | "done" = "pending";
  if (isDone || stage === 3) step2Status = "done";
  else if (stage === 1 || stage === 2) step2Status = "active";

  let step3Status: "pending" | "active" | "done" | "error" = "pending";
  if (isDone) step3Status = "done";
  else if (isError) step3Status = "error";
  else if (stage === 3) step3Status = "active";

  return [
    { label: "解析版本元数据", status: "done" },
    { label: "下载游戏支持库与资源", status: step2Status },
    { label: "执行安装与加载器注入", status: step3Status },
  ];
});
</script>

<template>
  <div class="flex h-full w-full gap-2">
    <!-- 左侧：任务队列 -->
    <aside class="flex w-[280px] shrink-0 flex-col pt-8 pb-4 pl-10 pr-2">
      <div class="mb-3 flex items-center justify-between px-2">
        <h2 class="text-[12px] font-semibold tracking-widest text-ink-muted">任务队列</h2>
        <button
          v-if="taskStore.hasTasks"
          class="text-[11px] text-ink-muted transition-colors hover:text-brand-500"
          @click="taskStore.clearFinished()"
        >
          清理已完成
        </button>
      </div>

      <div class="flex flex-1 flex-col gap-1.5 overflow-y-auto pr-2">
        <!-- 空状态 -->
        <div
          v-if="!taskStore.hasTasks"
          class="flex h-full flex-col items-center justify-center text-ink-muted opacity-60"
        >
          <CheckCircle2 :size="28" class="mb-2" />
          <p class="text-[12px]">队列为空</p>
        </div>

        <!-- 任务卡片 -->
        <button
          v-for="(t, id) in taskStore.tasks"
          :key="id"
          class="group relative flex flex-col gap-2.5 rounded-lg p-3 text-left transition-colors duration-200"
          :class="
            taskStore.activeId === id
              ? 'bg-surface shadow-sm ring-1 ring-line/50'
              : 'hover:bg-ink/[0.04]'
          "
          @click="taskStore.setActive(id)"
        >
          <div class="flex items-center justify-between gap-2">
            <span
              class="truncate text-[13px] font-medium transition-colors"
              :class="taskStore.activeId === id ? 'text-ink' : 'text-ink-soft group-hover:text-ink'"
            >
              {{ t.name }}
            </span>

            <!-- 状态指示器 -->
            <div class="flex h-5 w-5 shrink-0 items-center justify-center">
              <Loader2
                v-if="t.status === 'running'"
                :size="14"
                class="animate-spin text-brand-500"
              />
              <Check v-else-if="t.status === 'done'" :size="14" class="text-success" />
              <XCircle v-else :size="14" class="text-danger" />
            </div>
          </div>

          <!-- 进度条 -->
          <div class="flex items-center gap-2.5">
            <div class="h-1 flex-1 overflow-hidden rounded-full bg-ink/5">
              <div
                class="h-full rounded-full transition-all duration-300"
                :class="t.status === 'error' ? 'bg-danger' : 'bg-brand-500'"
                :style="{ width: `${getTaskPercent(t)}%` }"
              />
            </div>
            <span class="w-7 text-right font-mono text-[10px] tabular-nums text-ink-muted">
              {{ getTaskPercent(t) }}%
            </span>
          </div>
        </button>
      </div>
    </aside>

    <main class="relative flex flex-1 flex-col overflow-y-auto pb-10 pl-6 pr-10 pt-8">
      <div v-if="!task" class="flex h-full flex-col items-center justify-center text-ink-muted">
        <div
          class="flex h-24 w-24 items-center justify-center rounded-full bg-ink/5 mb-6 shadow-inner"
        >
          <Play :size="40" class="opacity-20 translate-x-1" />
        </div>
        <p class="text-[15px] font-medium text-ink">未选择任务</p>
        <p class="text-[13px] mt-1 opacity-60">请从左侧列表选择一个任务查看详情</p>
      </div>

      <div v-else class="mx-auto flex w-full max-w-3xl flex-col gap-6">
        <!-- 顶部主视觉卡片 -->
        <div
          class="relative flex flex-col overflow-hidden rounded-xl border border-brand-500/15 bg-brand-500/5 p-8 shadow-sm"
        >
          <div class="relative z-10 flex items-start justify-between">
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center gap-3">
                <AppBadge
                  :variant="
                    task.status === 'running'
                      ? 'brand'
                      : task.status === 'done'
                        ? 'success'
                        : 'danger'
                  "
                  :pulse="task.status === 'running'"
                >
                  {{
                    task.status === "running"
                      ? "正在安装"
                      : task.status === "done"
                        ? "安装完成"
                        : "安装失败"
                  }}
                </AppBadge>
                <span class="font-mono text-[12px] text-ink-muted">{{ task.id }}</span>
              </div>
              <h1 class="mt-1 truncate font-display text-2xl font-semibold text-ink">
                {{ task.name }}
              </h1>
            </div>

            <div class="mt-1 flex items-center gap-6">
              <div class="flex flex-col items-end">
                <span
                  class="font-mono text-2xl font-semibold leading-none text-brand-500 transition-opacity"
                  :class="{ 'opacity-40': task.summary?.stage === 3 }"
                >
                  {{ speedText }}
                </span>
                <span class="mt-2 text-[11px] text-ink-muted">实时速度</span>
              </div>
              <div class="h-10 w-px bg-line/60" />
              <div class="flex flex-col items-end">
                <span class="font-mono text-2xl font-semibold leading-none text-ink">{{
                  remainingFiles
                }}</span>
                <span class="mt-2 text-[11px] text-ink-muted">剩余文件</span>
              </div>
            </div>
          </div>

          <!-- 进度条区域 -->
          <div class="relative z-10 mt-10 flex flex-col gap-3">
            <div class="flex items-end justify-between">
              <span class="font-mono text-5xl font-light leading-none text-brand-500">
                {{ totalPercent }}<span class="text-2xl">%</span>
              </span>
              <span
                class="rounded-md border border-line/40 bg-surface/80 px-2.5 py-1 font-mono text-[13px] text-ink-muted shadow-sm"
              >
                {{ formatMB(task.summary?.downloadedBytes) }} /
                {{ formatMB(task.summary?.totalBytes) }}
              </span>
            </div>
            <div class="h-3 w-full overflow-hidden rounded-full bg-ink/5 shadow-inner">
              <div
                class="relative h-full rounded-full bg-brand-500 transition-all duration-300"
                :style="{ width: `${totalPercent}%` }"
              >
                <div
                  v-if="task.status === 'running' && task.summary?.stage !== 3"
                  class="absolute inset-0 animate-pulse bg-white/20"
                />
              </div>
            </div>
          </div>
        </div>

        <!-- 错误提示 -->
        <div
          v-if="task.status === 'error'"
          class="flex items-start gap-3 rounded-lg border border-danger/30 bg-danger-soft p-4 text-danger shadow-sm"
        >
          <AlertCircle :size="20" class="mt-0.5 shrink-0" />
          <div class="flex flex-col gap-1">
            <span class="font-medium">任务执行失败</span>
            <span class="text-[13px] opacity-90">{{ task.errorMsg }}</span>
          </div>
        </div>

        <!-- 步骤列表 -->
        <div class="sereno-glass rounded-xl p-6">
          <div class="relative flex flex-col gap-0">
            <div class="absolute bottom-5 left-[11px] top-3 w-px bg-line/60" />

            <div
              v-for="(step, i) in steps"
              :key="i"
              class="group relative flex gap-4 pb-6 last:pb-0"
            >
              <!-- 节点圆圈 -->
              <div
                class="relative z-10 mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full border-2 transition-colors duration-300"
                :class="{
                  'border-brand-500 bg-surface text-brand-500': step.status === 'done',
                  'border-brand-500 bg-brand-500 text-white shadow-[0_0_12px_rgba(15,100,181,0.3)]':
                    step.status === 'active',
                  'border-danger bg-danger-soft text-danger': step.status === 'error',
                  'border-line bg-surface-muted text-ink-muted/30': step.status === 'pending',
                }"
              >
                <Check v-if="step.status === 'done'" :size="12" :stroke-width="3" />
                <Loader2 v-else-if="step.status === 'active'" :size="12" class="animate-spin" />
                <AlertCircle v-else-if="step.status === 'error'" :size="12" />
                <div v-else class="h-1.5 w-1.5 rounded-full bg-current" />
              </div>

              <!-- 文本与状态 -->
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-3">
                  <span
                    class="text-[14px] font-medium transition-colors"
                    :class="{
                      'text-ink': step.status === 'done',
                      'font-semibold text-brand-500': step.status === 'active',
                      'font-medium text-danger': step.status === 'error',
                      'text-ink-muted/60': step.status === 'pending',
                    }"
                  >
                    {{ step.label }}
                  </span>

                  <div
                    v-if="
                      step.status === 'active' && task.summary?.stage === 3 && !task.currentFile
                    "
                    class="flex items-center gap-1.5 rounded-md border border-warning/20 bg-warning-soft/50 px-2.5 py-1 text-[11px] text-warning shadow-sm"
                  >
                    <Loader2 :size="10" class="shrink-0 animate-spin" />
                    <span>执行安装器，需 1-3 分钟…</span>
                  </div>

                  <div
                    v-else-if="step.status === 'active' && task.currentFile"
                    class="flex max-w-[300px] items-center gap-1.5 rounded-md border border-brand-500/15 bg-brand-500/8 px-2.5 py-1 text-[11px] text-brand-600 shadow-sm"
                  >
                    <Package :size="10" class="shrink-0" />
                    <span class="truncate">{{ task.currentFile.fileName }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </main>
  </div>
</template>
