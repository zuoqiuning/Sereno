<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  Archive,
  Boxes,
  FileText,
  Folder,
  HeartPulse,
  Package,
  Play,
  Puzzle,
  RefreshCw,
  ScrollText,
  Search,
  Settings2,
  Star,
  Trash2,
  Wrench,
} from "@lucide/vue";
import { useRouter } from "vue-router";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppEmptyState from "@/components/common/AppEmptyState.vue";
import AppIconButton from "@/components/common/AppIconButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppMenu from "@/components/common/AppMenu.vue";
import AppModal from "@/components/common/AppModal.vue";
import AppSelect from "@/components/common/AppSelect.vue";
import AppSkeleton from "@/components/common/AppSkeleton.vue";
import PageShell from "@/components/layout/PageShell.vue";
import type { MenuItem } from "@/components/common/menu";
import InstanceResourceModal from "@/components/downloads/InstanceResourceModal.vue";
import WorldBackupModal from "@/components/instances/WorldBackupModal.vue";
import LogViewerModal from "@/components/instances/LogViewerModal.vue";
import HealthModal from "@/components/instances/HealthModal.vue";
import InstanceSettingsModal from "@/components/instances/InstanceSettingsModal.vue";
import {
  exportDiagnostic,
  getRunningInstances,
  listInstances,
  onGameExited,
  setInstanceFavorite,
} from "@/api/instance";
import { launchGame } from "@/api/launch";
import { installGame, repairInstance } from "@/api/install";
import { getLoaderVersions, installLoader, type LoaderType } from "@/api/loader";
import { commands } from "@/bindings";
import { callCommand, isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useTaskStore } from "@/stores/task";
import { useToastStore } from "@/stores/toast";
import { useModalState } from "@/composables/useModalState";
import { askConfirm } from "@/composables/useConfirm";
import { formatAbsoluteDate, formatRelativeTime } from "@/utils/format";
import type { DownloadProgress, InstanceMetadata, LocalResourceKind } from "@/types";

const router = useRouter();
const taskStore = useTaskStore();
const toast = useToastStore();
const loading = ref(false);
const instances = ref<InstanceMetadata[]>([]);
const runningIds = ref<string[]>([]);
const deletingId = ref<string | null>(null);
const launchingId = ref<string | null>(null);
const installingId = ref<string | null>(null);
const repairingId = ref<string | null>(null);
const exportingId = ref<string | null>(null);
const resourceModal = useModalState<InstanceMetadata>();
const backupModal = useModalState<InstanceMetadata>();
const logModal = useModalState<InstanceMetadata>();
const healthModal = useModalState<InstanceMetadata>();
const settingsModal = useModalState<InstanceMetadata>();
let unlistenExit: (() => void) | null = null;

/* ---------------- 搜索与排序 ---------------- */

type SortKey = "recent" | "name" | "created";

const keyword = ref("");
const sortKey = ref<SortKey>("recent");

const sortOptions: { value: SortKey; label: string }[] = [
  { value: "recent", label: "最近游玩" },
  { value: "name", label: "名称" },
  { value: "created", label: "创建时间" },
];

/** 「最近游玩」优先用 lastPlayedAt，没玩过时退回创建时间。 */
function activityOf(instance: InstanceMetadata): number {
  const played = Number.parseInt(instance.lastPlayedAt ?? "", 10);
  if (!Number.isNaN(played) && played > 0) return played;
  const created = Number.parseInt(instance.createdAt, 10);
  return Number.isNaN(created) ? 0 : created;
}

/**
 * 列表的展示顺序：**收藏永远置顶**，其余按所选排序。
 *
 * 不在后端排序：实例数量通常在几十个量级，前端排一次的成本可以忽略，
 * 切换排序也不需要重新扫描磁盘。
 */
const visibleInstances = computed(() => {
  const query = keyword.value.trim().toLowerCase();
  const filtered = query
    ? instances.value.filter((item) => {
        return (
          item.displayName.toLowerCase().includes(query) ||
          item.gameVersion.toLowerCase().includes(query) ||
          item.loader.type.toLowerCase().includes(query) ||
          (item.loader.version ?? "").toLowerCase().includes(query)
        );
      })
    : [...instances.value];

  filtered.sort((a, b) => {
    if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
    switch (sortKey.value) {
      case "name":
        return a.displayName.localeCompare(b.displayName, "zh-CN");
      case "created":
        return Number(b.createdAt) - Number(a.createdAt);
      default:
        return activityOf(b) - activityOf(a);
    }
  });
  return filtered;
});

/** 保存实例设置后就地更新卡片，不必整表重载。 */
function onInstanceSaved(updated: InstanceMetadata): void {
  const index = instances.value.findIndex((item) => item.id === updated.id);
  if (index >= 0) instances.value[index] = updated;
}

function browseResources(kind: LocalResourceKind): void {
  const instanceId = resourceModal.item?.id;
  resourceModal.close();
  void router.push({
    path: "/downloads",
    query: instanceId ? { category: kind, instance: instanceId } : { category: kind },
  });
}

type LoaderChoice = "neoforge" | "forge" | "fabric" | "quilt";
const loaderChoices: { id: LoaderChoice; name: string }[] = [
  { id: "neoforge", name: "NeoForge" },
  { id: "forge", name: "Forge" },
  { id: "fabric", name: "Fabric" },
  { id: "quilt", name: "Quilt" },
];

interface LoaderDialogState {
  instance: InstanceMetadata;
  type: LoaderChoice;
  versions: string[];
  version: string;
  loading: boolean;
  installing: boolean;
}

const loaderDialog = ref<LoaderDialogState | null>(null);
/** 加载器弹窗的显隐：关闭时先淡出，动画播完再清空内容。 */
const loaderVisible = ref(false);

function isRunning(instanceId: string): boolean {
  return runningIds.value.includes(instanceId);
}

async function loadInstances(): Promise<void> {
  loading.value = true;
  try {
    instances.value = await listInstances();
    runningIds.value = await getRunningInstances();
  } catch (error) {
    void log.error(`加载实例失败: ${String(error)}`, {
      component: "InstancesView",
      action: "loadInstances",
    });
  } finally {
    loading.value = false;
  }
}

/**
 * 统一的失败提示：Toast 带标题 + 一行可读原因，日志里留完整对象。
 *
 * 之前这里用 `alert()`：会阻塞渲染线程，样式也不受控。
 */
function notifyError(title: string, error: unknown): void {
  const message = isApiError(error)
    ? error.message
    : error instanceof Error
      ? error.message
      : String(error);
  toast.error(title, message);
}

/** 把「Java 缺失 / 登录失效」翻译成可执行的下一步。 */
function adviceFor(error: unknown): string | null {
  if (isApiError(error) && error.type === "javaNotFound") {
    return `找不到适合该版本的 Java ${error.requiredMajor}，可以到「设置 → 启动」扫描或自动安装。`;
  }
  if (
    isApiError(error) &&
    (error.type === "notAuthenticated" || error.type === "tokenRefreshFailed")
  ) {
    return "微软登录已失效，请点右上角的账户重新登录。";
  }
  return null;
}

async function toggleFavorite(instance: InstanceMetadata): Promise<void> {
  const next = !instance.favorite;
  // 乐观更新：收藏是低风险操作，先改 UI，失败再回滚
  instance.favorite = next;
  try {
    await setInstanceFavorite(instance.id, next);
  } catch (error) {
    instance.favorite = !next;
    notifyError("收藏失败", error);
  }
}

async function handleLaunch(inst: InstanceMetadata): Promise<void> {
  if (launchingId.value) return;
  launchingId.value = inst.id;
  try {
    await launchGame(inst.id);
    if (!runningIds.value.includes(inst.id)) {
      runningIds.value = [...runningIds.value, inst.id];
    }
    toast.success("游戏已启动", `${inst.displayName} · ${inst.gameVersion}`);
    void log.info(`游戏已启动: ${inst.id}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "launch",
    });
  } catch (error) {
    const advice = adviceFor(error);
    if (advice) {
      const goSettings = await askConfirm({
        title: "启动失败",
        message: advice,
        confirmText: "去设置",
        cancelText: "知道了",
      });
      if (goSettings) void router.push("/settings");
    } else {
      notifyError("启动失败", error);
    }
    void log.error(`启动失败: ${JSON.stringify(error)}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "launch",
    });
  } finally {
    launchingId.value = null;
  }
}

async function handleInstall(inst: InstanceMetadata): Promise<void> {
  if (installingId.value) return;
  installingId.value = inst.id;
  // 可取消：任务页的「取消」会命中后端同名令牌
  taskStore.start(inst.id, inst.displayName, true);
  taskStore.setRetry(inst.id, () => handleInstall(inst));
  router.push("/tasks");

  try {
    await installGame(inst.id, (progress: DownloadProgress) => {
      if (progress.kind === "session") {
        taskStore.updateProgress(inst.id, progress.summary, null);
      } else {
        taskStore.updateProgress(inst.id, null, progress.snapshot);
      }
    });
    taskStore.finish(inst.id);
    await loadInstances();
  } catch (error) {
    taskStore.setError(inst.id, error instanceof Error ? error.message : String(error));
    void log.error(`安装失败: ${String(error)}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "install",
    });
  } finally {
    installingId.value = null;
  }
}

async function handleRepair(inst: InstanceMetadata): Promise<void> {
  if (repairingId.value) return;
  const confirmed = await askConfirm({
    title: `修复「${inst.displayName}」？`,
    message: "会校验并补齐缺失或损坏的游戏文件，已经正确的文件会被跳过。",
    confirmText: "开始修复",
  });
  if (!confirmed) return;

  repairingId.value = inst.id;
  const taskId = `repair-${inst.id}`;
  taskStore.start(taskId, `修复 ${inst.displayName}`, true);
  taskStore.setRetry(taskId, () => handleRepair(inst));
  router.push("/tasks");

  try {
    await repairInstance(inst.id, (progress: DownloadProgress) => {
      if (progress.kind === "session") {
        taskStore.updateProgress(taskId, progress.summary, null);
      } else {
        taskStore.updateProgress(taskId, null, progress.snapshot);
      }
    });
    taskStore.finish(taskId);
    await loadInstances();
  } catch (error) {
    taskStore.setError(taskId, error instanceof Error ? error.message : String(error));
    void log.error(`修复失败: ${String(error)}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "repair",
    });
  } finally {
    repairingId.value = null;
  }
}

/** 健康检查里的「一键修复」：复用实例页已有的流程，不重复实现。 */
function handleHealthAction(kind: string): void {
  const inst = healthModal.item;
  if (!inst) return;
  healthModal.close();

  // 缺失前置：直接带着 mod id 跳到资源搜索，省掉用户手动输入
  if (kind.startsWith("download:")) {
    void router.push({
      path: "/downloads",
      query: { category: "mod", instance: inst.id, q: kind.slice("download:".length) },
    });
    return;
  }

  switch (kind) {
    case "repair":
      void handleRepair(inst);
      break;
    case "open-settings":
      settingsModal.open(inst);
      break;
    case "open-mods":
      void handleOpenFolder(inst.id);
      break;
    case "open-logs":
      logModal.open(inst);
      break;
  }
}

async function handleOpenFolder(instanceId: string): Promise<void> {
  try {
    await callCommand(() => commands.openInstanceFolder(instanceId));
  } catch (error) {
    void log.error(`打开文件夹失败: ${String(error)}`, { component: "InstancesView" });
  }
}

async function openLoaderDialog(inst: InstanceMetadata): Promise<void> {
  const current = inst.loader.type;
  const initial: LoaderChoice =
    current === "forge" || current === "neoforge" || current === "fabric" || current === "quilt"
      ? current
      : "neoforge";

  loaderDialog.value = {
    instance: inst,
    type: initial,
    versions: [],
    version: "",
    loading: true,
    installing: false,
  };
  loaderVisible.value = true;
  await loadLoaderVersions();
}

/** 关闭加载器弹窗：先淡出，动画结束后再清空。 */
function closeLoaderDialog(): void {
  loaderVisible.value = false;
  window.setTimeout(() => {
    if (!loaderVisible.value) loaderDialog.value = null;
  }, 200);
}

async function loadLoaderVersions(): Promise<void> {
  const dialog = loaderDialog.value;
  if (!dialog) return;

  dialog.loading = true;
  dialog.versions = [];
  dialog.version = "";
  try {
    const versions = await getLoaderVersions(dialog.instance.gameVersion, dialog.type);
    dialog.versions = versions;
    dialog.version = versions[0] ?? "";
  } catch (error) {
    void log.error(`获取加载器版本失败: ${String(error)}`, {
      component: "InstancesView",
      instanceId: dialog.instance.id,
      action: "loaderVersions",
    });
  } finally {
    dialog.loading = false;
  }
}

async function selectLoaderType(type: LoaderChoice): Promise<void> {
  if (!loaderDialog.value || loaderDialog.value.installing) return;
  loaderDialog.value.type = type;
  await loadLoaderVersions();
}

async function confirmLoaderInstall(): Promise<void> {
  const dialog = loaderDialog.value;
  if (!dialog || !dialog.version || dialog.installing) return;

  const loaderName = loaderChoices.find((choice) => choice.id === dialog.type)?.name ?? dialog.type;
  const taskId = `loader-${dialog.instance.id}`;
  dialog.installing = true;
  taskStore.start(taskId, `安装 ${loaderName}`, true);

  try {
    await installLoader(dialog.instance.id, dialog.type as LoaderType, dialog.version);
    taskStore.finish(taskId);
    closeLoaderDialog();
    await loadInstances();
    toast.success(`${loaderName} 安装完成`, "现在可以启动游戏或安装 Mod 了");
  } catch (error) {
    const message = isApiError(error) ? error.message : "加载器安装失败，请查看日志。";
    taskStore.setError(taskId, message);
    notifyError("加载器安装失败", error);
  } finally {
    if (loaderDialog.value) {
      loaderDialog.value.installing = false;
    }
  }
}

async function handleExportDiagnostic(inst: InstanceMetadata): Promise<void> {
  if (exportingId.value) return;
  exportingId.value = inst.id;
  try {
    const path = await exportDiagnostic(inst.id);
    toast.success("诊断包已导出", path);
    void log.info(`诊断包已导出: ${path}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "diagnostics",
    });
  } catch (error) {
    notifyError("导出诊断包失败", error);
    void log.error(`导出诊断包失败: ${String(error)}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "diagnostics",
    });
  } finally {
    exportingId.value = null;
  }
}

async function handleDelete(instanceId: string, displayName: string): Promise<void> {
  if (isRunning(instanceId)) return;
  const confirmed = await askConfirm({
    title: `删除实例「${displayName}」？`,
    message: "文件会被移入系统回收站，可以在回收站里还原。",
    confirmText: "删除",
    danger: true,
  });
  if (!confirmed) return;

  deletingId.value = instanceId;
  try {
    await callCommand(() => commands.deleteInstance(instanceId));
    void log.info(`实例已删除: ${instanceId}`, {
      component: "InstancesView",
      instanceId,
      action: "delete",
    });
    await loadInstances();
  } catch (e) {
    notifyError("删除失败", e);
    void log.error(`删除实例失败: ${String(e)}`, {
      component: "InstancesView",
      instanceId,
      action: "delete",
    });
  } finally {
    deletingId.value = null;
  }
}

/** 卡片菜单项：按「看 → 调 → 修 → 删」分组，危险操作单独隔开。 */
function actionsFor(inst: InstanceMetadata): MenuItem[] {
  const running = isRunning(inst.id);
  const ready = inst.status === "ready";
  const busy = !!deletingId.value || !!launchingId.value || repairingId.value !== null;
  const destructiveBlocked = busy || running;

  return [
    {
      id: "folder",
      label: "打开游戏目录",
      icon: Folder,
      disabled: !!deletingId.value || launchingId.value !== null,
    },
    {
      id: "settings",
      label: "实例设置",
      icon: Settings2,
      disabled: destructiveBlocked,
    },
    { id: "health", label: "健康检查", icon: HeartPulse },
    {
      id: "resources",
      label: "管理 Mod / 资源包",
      icon: Puzzle,
      disabled: !!deletingId.value || launchingId.value !== null,
    },
    {
      id: "backup",
      label: "备份与回滚",
      icon: Archive,
      disabled: destructiveBlocked,
    },
    { id: "logs", label: "查看日志与崩溃", icon: ScrollText },
    {
      id: "diagnostics",
      label: "导出诊断包",
      icon: FileText,
      disabled: exportingId.value !== null,
    },
    {
      id: "repair",
      label: "校验并修复文件",
      icon: Wrench,
      separatorBefore: true,
      disabled: !ready || destructiveBlocked,
    },
    {
      id: "loader",
      label: "安装 / 更换加载器",
      icon: Package,
      disabled: !ready || destructiveBlocked,
    },
    {
      id: "favorite",
      label: inst.favorite ? "取消收藏" : "收藏（置顶）",
      icon: Star,
    },
    {
      id: "delete",
      label: "删除实例",
      icon: Trash2,
      danger: true,
      separatorBefore: true,
      disabled: destructiveBlocked,
    },
  ];
}

function runAction(id: string, inst: InstanceMetadata): void {
  switch (id) {
    case "folder":
      void handleOpenFolder(inst.id);
      break;
    case "settings":
      settingsModal.open(inst);
      break;
    case "health":
      healthModal.open(inst);
      break;
    case "resources":
      resourceModal.open(inst);
      break;
    case "backup":
      backupModal.open(inst);
      break;
    case "logs":
      logModal.open(inst);
      break;
    case "diagnostics":
      void handleExportDiagnostic(inst);
      break;
    case "repair":
      void handleRepair(inst);
      break;
    case "loader":
      void openLoaderDialog(inst);
      break;
    case "favorite":
      void toggleFavorite(inst);
      break;
    case "delete":
      void handleDelete(inst.id, inst.displayName);
      break;
  }
}

onMounted(() => {
  void loadInstances();
  void onGameExited((payload) => {
    void loadInstances();
    // 非零退出码 = 大概率崩了。此时**主动**把崩溃分析推给用户，
    // 而不是等他自己想起来去点「日志」。
    if (payload.exitCode !== undefined && payload.exitCode !== 0) {
      const instance = instances.value.find((item) => item.id === payload.instanceId);
      toast.warning("游戏非正常退出", `退出码 ${payload.exitCode}，已为你打开崩溃分析`, 6000);
      if (instance) {
        window.setTimeout(() => logModal.open(instance), 600);
      }
      void log.warn(`游戏异常退出: ${payload.instanceId}（退出码 ${payload.exitCode}）`, {
        component: "InstancesView",
        instanceId: payload.instanceId,
        action: "game-exit",
      });
    }
  }).then((unlisten) => {
    unlistenExit = unlisten;
  });
});

onBeforeUnmount(() => {
  unlistenExit?.();
});

function loaderLabel(type: string): string {
  switch (type) {
    case "vanilla":
      return "原版";
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
      return type;
  }
}

/** 秒 → 「x 小时 y 分」，避免界面上出现裸秒数。 */
function formatPlaytime(secs: number): string {
  if (secs < 60) return `${secs} 秒`;
  const hours = Math.floor(secs / 3600);
  const minutes = Math.floor((secs % 3600) / 60);
  return hours > 0 ? `${hours} 小时 ${minutes} 分` : `${minutes} 分`;
}
</script>

<template>
  <PageShell
    title="游戏实例"
    :description="`${instances.length} 个实例${runningIds.length > 0 ? ` · ${runningIds.length} 个运行中` : ''}`"
    :icon="Boxes"
    width="lg"
  >
    <template #actions>
      <!-- 搜索与排序直接进页头：实例页最需要竖向空间，不为一排输入框单开一行 -->
      <div class="w-64">
        <AppInput v-model="keyword" clearable placeholder="搜索实例名称、版本或加载器…">
          <template #prefix>
            <Search :size="14" />
          </template>
        </AppInput>
      </div>
      <div class="w-32">
        <AppSelect v-model="sortKey" :options="sortOptions" />
      </div>
      <AppIconButton size="sm" label="刷新实例列表" :disabled="loading" @click="loadInstances">
        <RefreshCw :size="14" :class="{ 'animate-spin': loading }" />
      </AppIconButton>
    </template>

    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
      <!-- 列表加载用骨架：表达「这里将出现几行内容」，比整页 spinner 更准确 -->
      <div v-if="loading && instances.length === 0" class="grid grid-cols-2 gap-4">
        <AppCard v-for="i in 4" :key="i" padding="md">
          <div class="flex flex-col gap-3">
            <div class="flex gap-2">
              <AppSkeleton width="w-14" height="h-5" />
              <AppSkeleton width="w-16" height="h-5" />
            </div>
            <AppSkeleton width="w-40" height="h-6" />
            <AppSkeleton width="w-24" height="h-4" />
            <div class="mt-4 flex justify-end gap-2">
              <AppSkeleton width="w-8" height="h-8" rounded="rounded-md" />
              <AppSkeleton width="w-20" height="h-8" rounded="rounded-md" />
            </div>
          </div>
        </AppCard>
      </div>

      <AppEmptyState
        v-else-if="instances.length === 0"
        :icon="Boxes"
        title="尚未安装任何游戏版本"
        description="先到「下载」页安装一个版本，它会自动出现在这里。"
        action-label="去下载版本"
        @action="router.push('/downloads')"
      />

      <AppEmptyState
        v-else-if="visibleInstances.length === 0"
        :icon="Search"
        :title="`没有匹配「${keyword}」的实例`"
        description="换个关键词，或者清空搜索框。"
        action-label="清空搜索"
        @action="keyword = ''"
      />

      <TransitionGroup v-else name="list" tag="div" class="grid grid-cols-2 gap-4 pb-4">
        <AppCard
          v-for="(inst, index) in visibleInstances"
          :key="inst.id"
          hoverable
          padding="md"
          :style="{ '--stagger': `${Math.min(index, 8) * 30}ms` }"
        >
          <div class="flex h-full items-center gap-4">
            <!-- 封面位：64px 方块 + 极淡品牌渐变，让卡片有「物品栏」的识别度，
                 而不是一排长得完全一样的白框 -->
            <div class="instance-cover" :class="{ 'is-running': isRunning(inst.id) }">
              <Boxes :size="26" :stroke-width="1.6" />
            </div>

            <div class="flex min-w-0 flex-1 flex-col gap-1.5">
              <div class="flex min-w-0 items-center gap-1">
                <h3 class="truncate text-h2 font-semibold text-ink">{{ inst.displayName }}</h3>
                <AppIconButton
                  size="sm"
                  class="-ml-1"
                  :label="inst.favorite ? '取消收藏' : '收藏（置顶）'"
                  @click="toggleFavorite(inst)"
                >
                  <Star
                    :size="13"
                    :class="inst.favorite ? 'fill-warning text-warning' : 'text-ink-muted/50'"
                  />
                </AppIconButton>
              </div>

              <div class="flex flex-wrap items-center gap-1.5">
                <AppBadge variant="brand" size="sm">{{ inst.gameVersion }}</AppBadge>
                <AppBadge variant="default" size="sm" outline>{{
                  loaderLabel(inst.loader.type)
                }}</AppBadge>
                <AppBadge v-if="isRunning(inst.id)" variant="success" size="sm" pulse>
                  运行中
                </AppBadge>
                <AppBadge v-else-if="inst.status !== 'ready'" variant="warning" size="sm">
                  未安装完成
                </AppBadge>
              </div>

              <p
                class="truncate text-micro text-ink-muted"
                :title="`创建于 ${formatAbsoluteDate(new Date(Number(inst.createdAt) * 1000))}`"
              >
                <template v-if="inst.lastPlayedAt">
                  上次游玩 {{ formatRelativeTime(inst.lastPlayedAt) }}
                </template>
                <template v-else>尚未启动过</template>
                · 共 {{ formatPlaytime(inst.playtimeSecs ?? 0) }}
                · {{ formatRelativeTime(inst.createdAt) }}创建
              </p>
            </div>

            <div class="flex shrink-0 items-center gap-1.5">
              <AppMenu
                :items="actionsFor(inst)"
                :disabled="deletingId === inst.id || launchingId === inst.id"
                @select="(action) => runAction(action, inst)"
              />
              <AppButton
                v-if="inst.status === 'ready'"
                variant="primary"
                size="sm"
                :loading="launchingId === inst.id"
                :disabled="
                  !!launchingId || deletingId !== null || repairingId !== null || isRunning(inst.id)
                "
                @click="handleLaunch(inst)"
              >
                <Play v-if="launchingId !== inst.id && !isRunning(inst.id)" :size="14" />
                {{ isRunning(inst.id) ? "运行中" : launchingId === inst.id ? "启动中" : "启动" }}
              </AppButton>
              <AppButton
                v-else
                variant="primary"
                size="sm"
                :loading="installingId === inst.id"
                :disabled="
                  !!installingId || deletingId !== null || repairingId !== null || isRunning(inst.id)
                "
                @click="handleInstall(inst)"
              >
                <RefreshCw v-if="installingId !== inst.id" :size="14" />
                {{ inst.status === "installing" ? "继续安装" : "安装" }}
              </AppButton>
            </div>
          </div>
        </AppCard>
      </TransitionGroup>
    </div>

    <!-- 加载器安装弹窗 -->
    <AppModal :open="loaderVisible" width="sm" @close="closeLoaderDialog">
      <template #header>
        <div v-if="loaderDialog" class="min-w-0 flex-1">
          <h3 class="text-h2 font-semibold text-ink">安装加载器</h3>
          <p class="truncate text-micro text-ink-muted">
            {{ loaderDialog.instance.displayName }} · Minecraft
            {{ loaderDialog.instance.gameVersion }}
          </p>
        </div>
      </template>

      <template v-if="loaderDialog">
        <div class="grid grid-cols-2 gap-2">
          <button
            v-for="choice in loaderChoices"
            :key="choice.id"
            type="button"
            class="rounded-md border px-3 py-2 text-body font-medium transition-colors"
            :class="
              loaderDialog.type === choice.id
                ? 'border-brand-500 bg-brand-500 text-white'
                : 'border-line text-ink-soft hover:bg-ink/5'
            "
            :disabled="loaderDialog.installing"
            @click="selectLoaderType(choice.id)"
          >
            {{ choice.name }}
          </button>
        </div>

        <div class="mt-4">
          <div v-if="loaderDialog.loading" class="text-body text-ink-muted">正在获取版本列表…</div>
          <div v-else-if="loaderDialog.versions.length === 0" class="text-body text-danger">
            没有找到可用版本，请检查网络后重试。
          </div>
          <AppSelect
            v-else
            v-model="loaderDialog.version"
            :options="loaderDialog.versions.map((version) => ({ value: version, label: version }))"
          />
        </div>

        <p class="mt-4 text-caption text-ink-muted">
          安装会先通过镜像预下载支持库，再把本地文件交给加载器安装器，通常几十秒内完成。
        </p>
      </template>

      <template #footer>
        <div class="flex justify-end gap-2">
          <AppButton
            variant="secondary"
            :disabled="loaderDialog?.installing"
            @click="closeLoaderDialog"
          >
            取消
          </AppButton>
          <AppButton
            variant="primary"
            :loading="loaderDialog?.installing"
            :disabled="!loaderDialog || loaderDialog.loading || !loaderDialog.version"
            @click="confirmLoaderInstall"
          >
            开始安装
          </AppButton>
        </div>
      </template>
    </AppModal>

    <InstanceResourceModal
      v-if="resourceModal.item"
      :instance="resourceModal.item"
      :open="resourceModal.visible"
      @close="resourceModal.close"
      @browse="browseResources"
    />

    <WorldBackupModal
      v-if="backupModal.item"
      :instance="backupModal.item"
      :open="backupModal.visible"
      @close="backupModal.close"
    />

    <HealthModal
      v-if="healthModal.item"
      :instance="healthModal.item"
      :running="isRunning(healthModal.item.id)"
      :open="healthModal.visible"
      @close="healthModal.close"
      @action="handleHealthAction"
    />

    <LogViewerModal
      v-if="logModal.item"
      :instance="logModal.item"
      :running="isRunning(logModal.item.id)"
      :open="logModal.visible"
      @close="logModal.close"
    />

    <InstanceSettingsModal
      v-if="settingsModal.item"
      :instance="settingsModal.item"
      :open="settingsModal.visible"
      @close="settingsModal.close"
      @saved="onInstanceSaved"
    />
  </PageShell>
</template>

<style scoped>
/* 实例卡片的「封面位」：淡品牌渐变 + 内描边，替代纯白卡片上的裸图标 */
.instance-cover {
  display: flex;
  height: 64px;
  width: 64px;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-lg);
  background: var(--color-surface-muted);
  color: var(--color-ink-soft);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-line) 70%, transparent);
}

.instance-cover.is-running {
  background: var(--color-success-soft);
  color: var(--color-success);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-success) 18%, transparent);
}
</style>
