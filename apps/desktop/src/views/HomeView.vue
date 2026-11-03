<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { RouterLink } from "vue-router";
import { Boxes, Coffee, Download, Play, UserRound } from "@lucide/vue";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppSkeleton from "@/components/common/AppSkeleton.vue";
import { getRunningInstances, listInstances, onGameExited } from "@/api/instance";
import { launchGame } from "@/api/launch";
import { getJavaCandidates } from "@/api/java";
import { getSettings, log } from "@/api/settings";
import { useTaskStore } from "@/stores/task";
import type { InstanceMetadata, JavaCandidate } from "@/types";

/** 按当前时段给一句问候。 */
const greeting = computed(() => {
  const hour = new Date().getHours();
  if (hour < 5) return "夜深了";
  if (hour < 11) return "早上好";
  if (hour < 13) return "中午好";
  if (hour < 18) return "下午好";
  return "晚上好";
});

const loading = ref(true);
const instances = ref<InstanceMetadata[]>([]);
const javas = ref<JavaCandidate[]>([]);
const runningIds = ref<string[]>([]);
const profileName = ref("Player");
const launchingId = ref<string | null>(null);
const errorMessage = ref("");
let unlistenExit: (() => void) | null = null;

const taskStore = useTaskStore();

/** 最近创建的实例（列表后端已按创建时间倒序）。 */
const latest = computed(() => instances.value[0] ?? null);
const bestJava = computed(() =>
  javas.value.reduce((max, java) => Math.max(max, java.majorVersion), 0),
);
const runningTasks = computed(() =>
  Object.values(taskStore.tasks).filter((task) => task.status === "running"),
);

function loaderLabel(type: string): string {
  switch (type) {
    case "fabric":
      return "Fabric";
    case "quilt":
      return "Quilt";
    case "forge":
      return "Forge";
    case "neoforge":
      return "NeoForge";
    case "optifine":
      return "OptiFine";
    default:
      return "原版";
  }
}

function lastPlayed(inst: InstanceMetadata): string {
  if (!inst.lastPlayedAt) return "尚未启动过";
  const date = new Date(Number.parseInt(inst.lastPlayedAt, 10) * 1000);
  return Number.isNaN(date.getTime())
    ? "尚未启动过"
    : `上次游玩：${date.toLocaleDateString("zh-CN")}`;
}

async function loadDashboard(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";

  const [instancesResult, runningResult, javaResult, settingsResult] = await Promise.allSettled([
    listInstances(),
    getRunningInstances(),
    getJavaCandidates(),
    getSettings(),
  ]);

  if (instancesResult.status === "fulfilled") {
    instances.value = instancesResult.value;
  }
  if (runningResult.status === "fulfilled") {
    runningIds.value = runningResult.value;
  }
  if (javaResult.status === "fulfilled") {
    javas.value = javaResult.value;
  }
  if (settingsResult.status === "fulfilled") {
    profileName.value = settingsResult.value.profile.name;
  }

  if (instancesResult.status === "rejected") {
    errorMessage.value = "实例列表读取失败";
    void log.error(errorMessage.value, { component: "HomeView", action: "loadInstances" });
  }

  loading.value = false;
}

async function handleLaunch(inst: InstanceMetadata): Promise<void> {
  if (launchingId.value) return;
  launchingId.value = inst.id;
  errorMessage.value = "";
  try {
    await launchGame(inst.id);
    void log.info(`游戏已启动: ${inst.id}`, {
      component: "HomeView",
      instanceId: inst.id,
      action: "launch",
    });
    await loadDashboard();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
    void log.error(`启动失败: ${String(error)}`, {
      component: "HomeView",
      instanceId: inst.id,
      action: "launch",
    });
  } finally {
    launchingId.value = null;
  }
}

onMounted(() => {
  void loadDashboard();
  // 游戏退出后刷新运行状态与游玩时长（首页没有这个监听就会一直显示“运行中”）
  void onGameExited(() => {
    void loadDashboard();
  }).then((unlisten) => {
    unlistenExit = unlisten;
  });
});

onBeforeUnmount(() => {
  unlistenExit?.();
});
</script>

<template>
  <div class="mx-auto flex h-full w-full max-w-5xl flex-col gap-8 px-10 pt-8 pb-10">
    <!-- 顶部欢迎区 -->
    <header class="flex shrink-0 items-center justify-between gap-6">
      <h1 class="font-display text-[28px] font-semibold tracking-tight text-ink">
        {{ greeting }}，准备好开始了吗？
      </h1>
      <AppBadge v-if="errorMessage" variant="danger" size="md">{{ errorMessage }}</AppBadge>
    </header>

    <!-- Bento Grid -->
    <div class="grid min-h-0 flex-1 grid-cols-3 content-start gap-8">
      <!-- 1. 最近实例（2 列，大卡） -->
      <AppCard class="col-span-2 h-56" hoverable>
        <template #header>
          <h3 class="flex items-center gap-2 text-[16px] font-semibold text-ink">
            <Boxes :size="18" :stroke-width="2" class="text-brand-500" />
            最近实例
          </h3>
          <RouterLink
            to="/instances"
            class="text-sm text-ink-muted transition-colors hover:text-brand-500"
          >
            查看全部 →
          </RouterLink>
        </template>

        <div v-if="loading" class="flex items-center gap-5">
          <AppSkeleton width="h-16 w-16" rounded="rounded-lg" />
          <div class="flex flex-col gap-3">
            <AppSkeleton width="w-40" height="h-5" />
            <AppSkeleton width="w-24" height="h-4" />
          </div>
        </div>

        <div v-else-if="latest" class="flex h-full items-center justify-between gap-6">
          <div class="flex min-w-0 items-center gap-5">
            <div
              class="flex h-16 w-16 shrink-0 items-center justify-center rounded-lg bg-brand-500/10"
            >
              <Boxes :size="28" :stroke-width="1.8" class="text-brand-500/60" />
            </div>
            <div class="flex min-w-0 flex-col gap-2">
              <span class="truncate text-lg font-semibold text-ink">{{ latest.displayName }}</span>
              <div class="flex flex-wrap items-center gap-2">
                <AppBadge variant="brand">{{ latest.gameVersion }}</AppBadge>
                <AppBadge variant="default" outline>{{ loaderLabel(latest.loader.type) }}</AppBadge>
                <AppBadge v-if="runningIds.includes(latest.id)" variant="success" pulse>
                  运行中
                </AppBadge>
                <span class="text-[12px] text-ink-muted">{{ lastPlayed(latest) }}</span>
              </div>
            </div>
          </div>
          <AppButton
            variant="primary"
            :loading="launchingId === latest.id"
            :disabled="latest.status !== 'ready' || !!launchingId || runningIds.includes(latest.id)"
            @click="handleLaunch(latest)"
          >
            <Play v-if="launchingId !== latest.id && !runningIds.includes(latest.id)" :size="16" />
            {{
              latest.status !== "ready"
                ? "未安装完成"
                : runningIds.includes(latest.id)
                  ? "运行中"
                  : launchingId === latest.id
                    ? "启动中"
                    : "启动"
            }}
          </AppButton>
        </div>

        <div v-else class="flex h-full flex-col items-start justify-center gap-2">
          <p class="text-sm text-ink-muted">还没有实例</p>
          <RouterLink to="/downloads" class="text-sm text-brand-500 hover:underline">
            去下载一个版本 →
          </RouterLink>
        </div>
      </AppCard>

      <!-- 2. 账户（1 列） -->
      <AppCard class="col-span-1 h-56" hoverable>
        <template #header>
          <h3 class="flex items-center gap-2 text-[16px] font-semibold text-ink">
            <UserRound :size="18" :stroke-width="2" class="text-brand-500" />
            账户
          </h3>
        </template>

        <div class="flex items-center gap-4">
          <div
            class="flex h-12 w-12 items-center justify-center rounded-full bg-brand-500/10 font-display text-lg font-semibold text-brand-500"
          >
            {{ profileName.slice(0, 1).toUpperCase() }}
          </div>
          <div class="flex min-w-0 flex-col gap-1">
            <span class="truncate text-[15px] font-medium text-ink">{{ profileName }}</span>
            <span class="text-[12px] text-ink-muted">离线账户 · 无需登录</span>
          </div>
        </div>
      </AppCard>

      <!-- 3. Java 运行环境（1 列） -->
      <AppCard class="col-span-1 h-56" hoverable>
        <template #header>
          <h3 class="flex items-center gap-2 text-[16px] font-semibold text-ink">
            <Coffee :size="18" :stroke-width="2" class="text-brand-500" />
            Java 运行环境
          </h3>
        </template>

        <div v-if="loading" class="flex flex-col gap-3">
          <AppSkeleton width="w-full" height="h-1.5" rounded="rounded-full" />
          <AppSkeleton width="w-24" height="h-3" />
        </div>
        <p v-else class="text-sm text-ink-muted">
          {{
            javas.length > 0
              ? `已检测到 ${javas.length} 个环境，最高 Java ${bestJava}`
              : "未检测到 Java，启动前请先在设置中扫描"
          }}
        </p>

        <RouterLink to="/settings" class="text-sm text-brand-500 hover:underline">
          管理 Java →
        </RouterLink>
      </AppCard>

      <!-- 4. 下载队列（2 列） -->
      <AppCard class="col-span-2 h-56">
        <template #header>
          <h3 class="flex items-center gap-2 text-[16px] font-semibold text-ink">
            <Download :size="18" :stroke-width="2" class="text-brand-500" />
            下载队列
          </h3>
          <RouterLink
            to="/tasks"
            class="text-sm text-ink-muted transition-colors hover:text-brand-500"
          >
            查看全部 →
          </RouterLink>
        </template>

        <div v-if="runningTasks.length > 0" class="flex flex-col gap-2">
          <div
            v-for="task in runningTasks.slice(0, 2)"
            :key="task.id"
            class="flex items-center justify-between gap-3 text-[13px]"
          >
            <span class="truncate text-ink-soft">{{ task.name }}</span>
            <span class="font-mono text-ink-muted">
              {{ task.summary ? `${task.summary.doneFiles}/${task.summary.totalFiles}` : "准备中" }}
            </span>
          </div>
        </div>
        <p v-else class="text-sm text-ink-muted">暂无进行中的任务</p>
      </AppCard>
    </div>
  </div>
</template>
