<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
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
  Settings2,
  Trash2,
  Wrench,
} from "@lucide/vue";
import { useRouter } from "vue-router";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import InstanceResourceModal from "@/components/downloads/InstanceResourceModal.vue";
import WorldBackupModal from "@/components/instances/WorldBackupModal.vue";
import LogViewerModal from "@/components/instances/LogViewerModal.vue";
import HealthModal from "@/components/instances/HealthModal.vue";
import InstanceSettingsModal from "@/components/instances/InstanceSettingsModal.vue";
import { exportDiagnostic, getRunningInstances, listInstances, onGameExited } from "@/api/instance";
import { launchGame } from "@/api/launch";
import { installGame, repairInstance } from "@/api/install";
import { getLoaderVersions, installLoader, type LoaderType } from "@/api/loader";
import { commands } from "@/bindings";
import { callCommand, isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useTaskStore } from "@/stores/task";
import { useModalState } from "@/composables/useModalState";
import type { DownloadProgress, InstanceMetadata, LocalResourceKind } from "@/types";

const router = useRouter();
const taskStore = useTaskStore();
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

async function handleLaunch(inst: InstanceMetadata): Promise<void> {
  if (launchingId.value) return;
  launchingId.value = inst.id;
  try {
    await launchGame(inst.id);
    if (!runningIds.value.includes(inst.id)) {
      runningIds.value = [...runningIds.value, inst.id];
    }
    void log.info(`游戏已启动: ${inst.id}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "launch",
    });
  } catch (e) {
    let msg = "启动失败，请查看日志。";
    if (isApiError(e)) {
      if (e.type === "javaNotFound") {
        msg = `找不到适合该版本的 Java ${e.requiredMajor}。\n请前往「设置」页面扫描 Java。`;
      } else if (e.type === "notAuthenticated" || e.type === "tokenRefreshFailed") {
        msg = "微软登录已失效，请点击右上角的账户重新登录。";
      } else {
        msg = e.message;
      }
    } else if (e instanceof Error) {
      msg = e.message;
    }
    alert(msg);
    void log.error(`启动失败: ${JSON.stringify(e)}`, {
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
  taskStore.start(inst.id, inst.displayName);
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
    const message = `继续安装失败：${error instanceof Error ? error.message : String(error)}`;
    taskStore.setError(inst.id, message);
    void log.error(message, { component: "InstancesView", instanceId: inst.id, action: "install" });
  } finally {
    installingId.value = null;
  }
}

async function handleRepair(inst: InstanceMetadata): Promise<void> {
  if (repairingId.value) return;
  if (!confirm(`将校验并补齐「${inst.displayName}」缺失或损坏的游戏文件，继续吗？`)) return;

  repairingId.value = inst.id;
  const taskId = `repair-${inst.id}`;
  taskStore.start(taskId, `修复 ${inst.displayName}`);
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
    const message = `修复失败：${error instanceof Error ? error.message : String(error)}`;
    taskStore.setError(taskId, message);
    void log.error(message, { component: "InstancesView", instanceId: inst.id, action: "repair" });
  } finally {
    repairingId.value = null;
  }
}

/** 健康检查里的「一键修复」：复用实例页已有的流程，不重复实现。 */
function handleHealthAction(kind: string): void {
  const inst = healthModal.item;
  if (!inst) return;
  healthModal.close();

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
  } catch (e) {
    void log.error(`打开文件夹失败: ${String(e)}`, { component: "InstancesView" });
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

  dialog.installing = true;
  try {
    await installLoader(dialog.instance.id, dialog.type as LoaderType, dialog.version);
    const name = loaderChoices.find((choice) => choice.id === dialog.type)?.name ?? dialog.type;
    closeLoaderDialog();
    await loadInstances();
    alert(`${name} 安装完成，可以启动游戏或安装 Mod 了。`);
  } catch (error) {
    alert(isApiError(error) ? error.message : "加载器安装失败，请查看日志。");
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
    alert(`诊断包已导出：\n${path}`);
    void log.info(`诊断包已导出: ${path}`, {
      component: "InstancesView",
      instanceId: inst.id,
      action: "diagnostics",
    });
  } catch (error) {
    alert(isApiError(error) ? error.message : "导出诊断包失败，请查看日志。");
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
  if (!confirm(`确定要删除实例「${displayName}」吗？\n文件将被移入回收站。`)) {
    return;
  }
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
    alert(isApiError(e) ? e.message : "删除失败，请查看日志。");
    void log.error(`删除实例失败: ${String(e)}`, {
      component: "InstancesView",
      instanceId,
      action: "delete",
    });
  } finally {
    deletingId.value = null;
  }
}

onMounted(() => {
  void loadInstances();
  void onGameExited(() => {
    void loadInstances();
  }).then((unlisten) => {
    unlistenExit = unlisten;
  });
});

onBeforeUnmount(() => {
  unlistenExit?.();
});

function formatDate(timestampStr: string): string {
  if (!timestampStr) return "刚刚";
  const timestamp = Number.parseInt(timestampStr, 10);
  if (Number.isNaN(timestamp)) {
    const date = new Date(timestampStr);
    return Number.isNaN(date.getTime()) ? "未知" : date.toLocaleDateString("zh-CN");
  }
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

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
  <div class="mx-auto flex h-full w-full max-w-4xl flex-col px-10 pt-8 pb-10">
    <header class="mb-6 flex shrink-0 items-center justify-between">
      <div class="flex items-center gap-3">
        <Boxes :size="24" class="text-brand-500" />
        <h1 class="font-display text-2xl font-semibold text-ink">游戏实例</h1>
      </div>
      <AppButton variant="secondary" :loading="loading" @click="loadInstances">
        <RefreshCw :size="14" :class="{ 'animate-spin': loading }" />
        刷新
      </AppButton>
    </header>

    <div class="flex-1 overflow-y-auto pr-1">
      <div v-if="loading && instances.length === 0" class="flex h-full items-center justify-center">
        <AppLoading :size="64" label="正在读取实例…" />
      </div>
      <div
        v-else-if="instances.length === 0"
        class="flex h-full flex-col items-center justify-center rounded-xl border border-dashed border-line/70 bg-glass/30 text-ink-muted"
      >
        <Boxes :size="40" class="mb-3 opacity-20" />
        <p class="text-[14px]">尚未安装任何游戏版本</p>
        <p class="mt-1 text-[12px]">请前往「下载」页面下载一个版本</p>
      </div>

      <div v-else class="grid grid-cols-2 gap-4 pb-4">
        <AppCard v-for="inst in instances" :key="inst.id" hoverable padding="md">
          <div class="flex flex-col h-full justify-between">
            <div>
              <div class="mb-3 flex items-center justify-between gap-2">
                <div class="flex flex-wrap items-center gap-2">
                  <AppBadge variant="brand" size="sm">{{ inst.gameVersion }}</AppBadge>
                  <AppBadge variant="default" size="sm" outline>{{
                    loaderLabel(inst.loader.type)
                  }}</AppBadge>
                  <AppBadge v-if="isRunning(inst.id)" variant="success" size="sm" pulse>
                    运行中
                  </AppBadge>
                </div>
                <span class="text-[11px] text-ink-muted">{{ formatDate(inst.createdAt) }}</span>
              </div>
              <h3 class="mb-1 truncate text-lg font-semibold text-ink">{{ inst.displayName }}</h3>
              <p class="text-[12px] text-ink-muted">
                游玩时长: {{ formatPlaytime(inst.playtimeSecs ?? 0) }}
              </p>
            </div>

            <div class="mt-4 flex flex-wrap items-center justify-end gap-2">
              <AppButton
                variant="secondary"
                size="sm"
                :disabled="!!deletingId || launchingId !== null || repairingId !== null"
                @click="handleOpenFolder(inst.id)"
              >
                <Folder :size="14" />
              </AppButton>
              <AppButton
                variant="secondary"
                size="sm"
                title="实例设置（Java / 内存 / JVM 参数）"
                :disabled="
                  !!deletingId || launchingId !== null || repairingId !== null || isRunning(inst.id)
                "
                @click="settingsModal.open(inst)"
              >
                <Settings2 :size="14" />
              </AppButton>
              <AppButton
                variant="secondary"
                size="sm"
                title="检查 Java / 内存 / Mod 依赖与崩溃"
                @click="healthModal.open(inst)"
              >
                <HeartPulse :size="14" />
              </AppButton>
              <AppButton
                variant="secondary"
                size="sm"
                title="管理已安装的 MOD / 资源包 / 光影"
                :disabled="!!deletingId || launchingId !== null || repairingId !== null"
                @click="resourceModal.open(inst)"
              >
                <Puzzle :size="14" />
              </AppButton>
              <AppButton
                variant="secondary"
                size="sm"
                title="存档备份与恢复"
                :disabled="
                  !!deletingId || launchingId !== null || repairingId !== null || isRunning(inst.id)
                "
                @click="backupModal.open(inst)"
              >
                <Archive :size="14" />
              </AppButton>
              <AppButton
                variant="secondary"
                size="sm"
                title="查看最新日志与崩溃报告"
                @click="logModal.open(inst)"
              >
                <ScrollText :size="14" />
              </AppButton>
              <AppButton
                variant="secondary"
                size="sm"
                title="导出诊断包（日志 + 崩溃报告）"
                :loading="exportingId === inst.id"
                :disabled="exportingId !== null && exportingId !== inst.id"
                @click="handleExportDiagnostic(inst)"
              >
                <FileText v-if="exportingId !== inst.id" :size="14" />
              </AppButton>
              <AppButton
                v-if="inst.status === 'ready'"
                variant="secondary"
                size="sm"
                title="校验并修复缺失或损坏的游戏文件"
                :loading="repairingId === inst.id"
                :disabled="
                  !!deletingId ||
                  !!launchingId ||
                  !!installingId ||
                  isRunning(inst.id) ||
                  (repairingId !== null && repairingId !== inst.id)
                "
                @click="handleRepair(inst)"
              >
                <Wrench v-if="repairingId !== inst.id" :size="14" />
              </AppButton>
              <AppButton
                v-if="inst.status === 'ready'"
                variant="secondary"
                size="sm"
                title="为这个实例安装或更换加载器"
                :disabled="
                  !!launchingId || !!installingId || repairingId !== null || isRunning(inst.id)
                "
                @click="openLoaderDialog(inst)"
              >
                <Package :size="14" />
                加载器
              </AppButton>
              <AppButton
                variant="danger"
                size="sm"
                :loading="deletingId === inst.id"
                :disabled="
                  !!deletingId ||
                  launchingId !== null ||
                  installingId !== null ||
                  repairingId !== null ||
                  isRunning(inst.id)
                "
                @click="handleDelete(inst.id, inst.displayName)"
              >
                <Trash2 v-if="deletingId !== inst.id" :size="14" />
              </AppButton>
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
                  !!installingId ||
                  deletingId !== null ||
                  repairingId !== null ||
                  isRunning(inst.id)
                "
                @click="handleInstall(inst)"
              >
                <RefreshCw v-if="installingId !== inst.id" :size="14" />
                {{ inst.status === "installing" ? "继续安装" : "安装" }}
              </AppButton>
            </div>
          </div>
        </AppCard>
      </div>
    </div>

    <!-- 加载器安装弹窗 -->
    <AppModal :open="loaderVisible" width="sm" @close="closeLoaderDialog">
      <template #header>
        <div v-if="loaderDialog" class="min-w-0 flex-1">
          <h3 class="text-[15px] font-semibold text-ink">安装加载器</h3>
          <p class="truncate text-[11px] text-ink-muted">
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
            class="rounded-md border px-3 py-2 text-sm font-medium transition-colors"
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
          <div v-if="loaderDialog.loading" class="text-sm text-ink-muted">正在获取版本列表…</div>
          <div v-else-if="loaderDialog.versions.length === 0" class="text-sm text-danger">
            没有找到可用版本，请检查网络后重试。
          </div>
          <select
            v-else
            v-model="loaderDialog.version"
            class="w-full rounded-md border border-line bg-surface px-3 py-2 text-sm text-ink"
          >
            <option v-for="version in loaderDialog.versions" :key="version" :value="version">
              {{ version }}
            </option>
          </select>
        </div>

        <p class="mt-4 text-[12px] text-ink-muted">
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
  </div>
</template>
