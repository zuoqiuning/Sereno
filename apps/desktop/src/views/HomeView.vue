<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { BadgeCheck, Boxes, Coffee, Download, Play, Rocket, UserRound } from "@lucide/vue";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppEmptyState from "@/components/common/AppEmptyState.vue";
import AppSkeleton from "@/components/common/AppSkeleton.vue";
import PageShell from "@/components/layout/PageShell.vue";
import { getRunningInstances, listInstances, onGameExited } from "@/api/instance";
import { launchGame } from "@/api/launch";
import { getJavaCandidates } from "@/api/java";
import { getAccountState } from "@/api/account";
import { toApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useTaskStore } from "@/stores/task";
import { useToastStore } from "@/stores/toast";
import { formatAbsoluteDate, formatRelativeTime } from "@/utils/format";
import type { AccountState, InstanceMetadata, JavaCandidate } from "@/types";

/** 按当前时段给一句问候。 */
const greeting = computed(() => {
  const hour = new Date().getHours();
  if (hour < 5) return "夜深了";
  if (hour < 11) return "早上好";
  if (hour < 13) return "中午好";
  if (hour < 18) return "下午好";
  return "晚上好";
});

const today = computed(() =>
  new Date().toLocaleDateString("zh-CN", { month: "long", day: "numeric", weekday: "long" }),
);

const router = useRouter();
const loading = ref(true);
const instances = ref<InstanceMetadata[]>([]);
const javas = ref<JavaCandidate[]>([]);
const runningIds = ref<string[]>([]);
const account = ref<AccountState | null>(null);
const launchingId = ref<string | null>(null);
const errorMessage = ref("");
let unlistenExit: (() => void) | null = null;

const taskStore = useTaskStore();
const toast = useToastStore();

/**
 * 舞台中央的实例：**最近玩过的那个**，没玩过时退回最近创建的。
 *
 * 之前直接取 `instances[0]`，而后端按「创建时间」倒序返回——于是首页永远
 * 展示新建的那个空实例，而不是用户真正在玩的那个。README 写的是
 * "The last-played instance takes the stage"，实现必须一致。
 */
const latest = computed(() => {
  if (instances.value.length === 0) return null;
  const played = (item: InstanceMetadata): number => {
    const value = Number.parseInt(item.lastPlayedAt ?? "", 10);
    return Number.isNaN(value) ? 0 : value;
  };
  return [...instances.value].sort((a, b) => {
    if (played(a) !== played(b)) return played(b) - played(a);
    return Number(b.createdAt) - Number(a.createdAt);
  })[0];
});
const bestJava = computed(() =>
  javas.value.reduce((max, java) => Math.max(max, java.majorVersion), 0),
);
const runningTasks = computed(() =>
  Object.values(taskStore.tasks).filter((task) => task.status === "running"),
);
const latestRunning = computed(() =>
  latest.value ? runningIds.value.includes(latest.value.id) : false,
);
const latestReady = computed(() => latest.value?.status === "ready");

const launchLabel = computed(() => {
  const inst = latest.value;
  if (!inst) return "";
  if (inst.status !== "ready") return "未安装完成";
  if (latestRunning.value) return "运行中";
  if (launchingId.value === inst.id) return "启动中";
  return "启动游戏";
});

const taskSummary = computed(() => {
  if (runningTasks.value.length === 0) return "暂无进行中的任务";
  const task = runningTasks.value[0];
  const files = task.summary
    ? `${task.summary.doneFiles}/${task.summary.totalFiles}`
    : "准备中";
  return `${task.name} · ${files}`;
});

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

/** 累计游玩时长；不足一分钟就不显示，避免「0 分钟」的噪音。 */
function playtimeLabel(inst: InstanceMetadata): string {
  const secs = inst.playtimeSecs ?? 0;
  if (secs < 60) return "";
  const hours = Math.floor(secs / 3600);
  if (hours > 0) return `已游玩 ${hours} 小时`;
  return `已游玩 ${Math.floor(secs / 60)} 分钟`;
}

async function loadDashboard(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";

  const [instancesResult, runningResult, javaResult, accountResult] = await Promise.allSettled([
    listInstances(),
    getRunningInstances(),
    getJavaCandidates(),
    getAccountState(),
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
  if (accountResult.status === "fulfilled") {
    account.value = accountResult.value;
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
    toast.success("游戏已启动", `${inst.displayName} · ${inst.gameVersion}`);
    void log.info(`游戏已启动: ${inst.id}`, {
      component: "HomeView",
      instanceId: inst.id,
      action: "launch",
    });
    await loadDashboard();
  } catch (error) {
    // 统一走 ApiError：中文文案 + 「下一步」，而不是把
    // `error sending request for url (...)` 这种英文技术细节直接甩给用户
    errorMessage.value = toApiError(error).message;
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
  <PageShell
    :title="`${greeting}，准备好开始了吗？`"
    :description="`${today} · ${instances.length} 个实例`"
    :icon="Rocket"
    size="hero"
    width="lg"
  >
    <template #actions>
      <AppBadge v-if="errorMessage" variant="danger" size="md">{{ errorMessage }}</AppBadge>
      <AppButton variant="secondary" size="sm" @click="router.push('/instances')">
        <Boxes :size="14" />
        管理实例
      </AppButton>
    </template>

    <!-- 舞台：最近玩过的实例。不再叠径向渐变与水印，层次交给卡片 + 描边 -->
    <AppCard padding="lg" class="min-h-65 flex-1">
      <div v-if="loading" class="flex flex-1 items-center gap-6">
        <AppSkeleton width="h-16 w-16" rounded="rounded-lg" />
        <div class="flex flex-col gap-3">
          <AppSkeleton width="w-52" height="h-6" />
          <AppSkeleton width="w-32" height="h-4" />
        </div>
      </div>

      <div v-else-if="latest" class="stage relative flex flex-1 flex-col">
        <!-- 极淡的品牌色光：只暗示「这一块是焦点」，不做大面积渐变 -->
        <div class="stage-glow" aria-hidden="true" />

        <div class="relative flex items-center gap-2 text-micro font-medium text-ink-muted">
          <span class="relative flex h-1.5 w-1.5">
            <span
              v-if="latestRunning"
              class="absolute inset-0 animate-ping rounded-round bg-success/70"
            />
            <span
              class="relative h-1.5 w-1.5 rounded-round"
              :class="latestRunning ? 'bg-success' : 'bg-brand-500/70'"
            />
          </span>
          {{ latestRunning ? "正在运行" : "最近游玩" }}
          <span class="ml-auto font-normal">本地实例</span>
        </div>

        <div class="relative flex flex-1 items-center gap-6 py-6">
          <div class="stage-cover" :class="{ 'is-running': latestRunning }">
            <Boxes :size="40" :stroke-width="1.4" />
          </div>

          <div class="min-w-0 flex-1">
            <h2 class="hero-title truncate text-ink" :title="latest.displayName">
              {{ latest.displayName }}
            </h2>
            <div class="mt-3 flex flex-wrap items-center gap-2 text-caption text-ink-muted">
              <AppBadge variant="brand" size="sm">{{ latest.gameVersion }}</AppBadge>
              <AppBadge variant="default" size="sm" outline>{{
                loaderLabel(latest.loader.type)
              }}</AppBadge>
              <span :title="formatAbsoluteDate(new Date(Number(latest.lastPlayedAt ?? 0) * 1000))">
                {{
                  latest.lastPlayedAt
                    ? `上次游玩 ${formatRelativeTime(latest.lastPlayedAt)}`
                    : "尚未启动过"
                }}
              </span>
            </div>
          </div>

          <AppButton
            variant="primary"
            size="lg"
            class="shrink-0"
            :loading="launchingId === latest.id"
            :disabled="!latestReady || !!launchingId || latestRunning"
            @click="handleLaunch(latest)"
          >
            <Play v-if="launchingId !== latest.id && !latestRunning" :size="17" />
            {{ launchLabel }}
          </AppButton>
        </div>

        <!-- 底部数据条：把原来「标题下方到按钮之间」的空白换成可读信息 -->
        <div class="relative grid grid-cols-3 gap-3 border-t border-line/60 pt-4">
          <div class="min-w-0 rounded-md bg-surface-muted px-3 py-2.5">
            <p class="text-micro text-ink-muted">上次游玩</p>
            <p class="mt-0.5 truncate text-h3 font-semibold text-ink tabular">
              {{ latest.lastPlayedAt ? formatRelativeTime(latest.lastPlayedAt) : "尚未启动" }}
            </p>
          </div>
          <div class="min-w-0 rounded-md bg-surface-muted px-3 py-2.5">
            <p class="text-micro text-ink-muted">累计时长</p>
            <p class="mt-0.5 truncate text-h3 font-semibold text-ink tabular">
              {{ playtimeLabel(latest) || "不足 1 分钟" }}
            </p>
          </div>
          <div class="min-w-0 rounded-md bg-surface-muted px-3 py-2.5">
            <p class="text-micro text-ink-muted">创建于</p>
            <p class="mt-0.5 truncate text-h3 font-semibold text-ink tabular">
              {{ formatRelativeTime(latest.createdAt) }}
            </p>
          </div>
        </div>
      </div>

      <AppEmptyState
        v-else
        :icon="Boxes"
        title="还没有实例"
        description="先下载一个游戏版本，Sereno 会把它放在这里。"
        action-label="去下载版本"
        @action="router.push('/downloads')"
      />
    </AppCard>

    <!-- 底部状态：账户 / Java / 任务。三张卡结构完全一致 -->
    <div class="grid shrink-0 grid-cols-3 gap-4">
      <AppCard padding="sm">
        <div class="flex items-center gap-3">
          <div
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-round"
            :class="
              account?.kind === 'microsoft'
                ? 'bg-success-soft text-success'
                : 'bg-surface-muted text-ink-soft'
            "
          >
            <BadgeCheck v-if="account?.kind === 'microsoft'" :size="19" />
            <UserRound v-else :size="18" />
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-micro text-ink-muted">
              {{ account?.kind === "microsoft" ? "微软正版" : "离线档案" }}
            </p>
            <p class="truncate text-h2 font-semibold text-ink">
              {{ account?.name ?? "读取中…" }}
            </p>
          </div>
        </div>
      </AppCard>

      <RouterLink to="/settings" class="block">
        <AppCard padding="sm" hoverable>
          <div class="flex items-center gap-3">
            <div
              class="flex h-10 w-10 shrink-0 items-center justify-center rounded-round bg-surface-muted"
            >
              <Coffee :size="18" class="text-ink-soft" />
            </div>
            <div class="min-w-0 flex-1">
              <p class="text-micro text-ink-muted">Java 环境</p>
              <p class="truncate text-h2 font-semibold text-ink">
                {{ javas.length > 0 ? `${javas.length} 个 · 最高 Java ${bestJava}` : "未检测到 Java" }}
              </p>
            </div>
          </div>
        </AppCard>
      </RouterLink>

      <RouterLink to="/tasks" class="block">
        <AppCard padding="sm" hoverable>
          <div class="flex items-center gap-3">
            <div
              class="flex h-10 w-10 shrink-0 items-center justify-center rounded-round bg-surface-muted"
            >
              <Download :size="18" class="text-ink-soft" />
            </div>
            <div class="min-w-0 flex-1">
              <p class="text-micro text-ink-muted">下载队列</p>
              <p class="truncate text-h2 font-semibold text-ink">{{ taskSummary }}</p>
            </div>
            <span
              v-if="runningTasks.length"
              class="shrink-0 rounded-round bg-surface-muted px-1.5 py-0.5 text-micro font-medium text-ink-soft"
            >
              {{ runningTasks.length }}
            </span>
          </div>
        </AppCard>
      </RouterLink>
    </div>
  </PageShell>
</template>

<style scoped>
/* 舞台光：只在左上与右下各铺一层极淡品牌色，8% 以内不会出色带 */
.stage-glow {
  position: absolute;
  inset: 0;
  pointer-events: none;
  background:
    radial-gradient(560px 260px at 6% -10%, var(--alpha-brand-8), transparent 72%),
    radial-gradient(420px 220px at 100% 110%, var(--alpha-brand-4), transparent 72%);
}

/* 实例封面：88px 方块，渐变 + 内描边 + 外扩柔影，撑起整张卡片的视觉重心 */
.stage-cover {
  display: flex;
  height: 88px;
  width: 88px;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  border-radius: 20px;
  background: var(--color-surface-muted);
  color: var(--color-ink-soft);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-line) 70%, transparent);
}

.stage-cover.is-running {
  background: var(--color-success-soft);
  color: var(--color-success);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-success) 18%, transparent);
}

.hero-title {
  font-family: var(--font-display);
  font-size: var(--text-hero);
  line-height: var(--text-hero--line-height);
  letter-spacing: var(--text-hero--letter-spacing);
  font-weight: 600;
}
</style>
