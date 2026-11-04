<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Archive, Camera, Pin, PinOff, RotateCcw, Trash2 } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import { toMessage } from "@/utils/error";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import {
  createContentSnapshot,
  createWorldBackup,
  deleteContentSnapshot,
  listContentSnapshots,
  listWorldBackups,
  pinContentSnapshot,
  restoreContentSnapshot,
  restoreWorldBackup,
} from "@/api/backup";
import { askConfirm } from "@/composables/useConfirm";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import type { ContentSnapshot, InstanceMetadata, WorldBackup } from "@/types";

/**
 * 备份与回滚。
 *
 * 两类备份语义完全不同，所以放在同一个弹窗的两个页签里：
 * - **内容快照**：mods / config / 资源包 / 光影，用来「装坏了能退回去」；
 * - **世界存档**：`saves/`，用来「存档玩坏了能退回去」。
 */
const props = withDefaults(defineProps<{ instance: InstanceMetadata; open?: boolean }>(), {
  open: true,
});
const emit = defineEmits<{ (e: "close"): void }>();

const toast = useToastStore();

type Tab = "content" | "world";
const tab = ref<Tab>("content");

const loading = ref(true);
const busy = ref<string | null>(null);
const error = ref("");

const snapshots = ref<ContentSnapshot[]>([]);
const backups = ref<WorldBackup[]>([]);

const REASON_LABELS: Record<string, string> = {
  manual: "手动创建",
  "before-mod-change": "安装 Mod 前",
  "before-loader-change": "更换加载器前",
  "before-import": "导入前",
};

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatDate(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  if (Number.isNaN(date.getTime())) return "未知时间";
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function reasonLabel(reason: string): string {
  return REASON_LABELS[reason] ?? reason;
}

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    const [content, world] = await Promise.all([
      listContentSnapshots(props.instance.id),
      listWorldBackups(props.instance.id),
    ]);
    snapshots.value = content;
    backups.value = world;
  } catch (err) {
    error.value = `读取备份失败：${toMessage(err)}`;
    void log.error(error.value, { component: "BackupModal", action: "list" });
  } finally {
    loading.value = false;
  }
}

async function createSnapshot(): Promise<void> {
  busy.value = "create-snapshot";
  error.value = "";
  try {
    const snapshot = await createContentSnapshot(props.instance.id, "manual", false);
    snapshots.value = [snapshot, ...snapshots.value];
    toast.success("快照已创建", `${snapshot.fileCount} 个文件 · ${formatSize(snapshot.sizeBytes)}`);
    void log.info(`创建内容快照: ${snapshot.id}`, {
      component: "BackupModal",
      action: "createSnapshot",
      instanceId: props.instance.id,
    });
  } catch (err) {
    error.value = `创建快照失败：${toMessage(err)}`;
    void log.error(error.value, { component: "BackupModal", action: "createSnapshot" });
  } finally {
    busy.value = null;
  }
}

async function restoreSnapshot(snapshot: ContentSnapshot): Promise<void> {
  const confirmed = await askConfirm({
    title: "回滚到这份快照？",
    message: `当前 mods / config / 资源包会被「替换」为 ${formatDate(snapshot.createdAt)} 的内容。\n世界存档不受影响。`,
    confirmText: "回滚",
    danger: true,
  });
  if (!confirmed) return;

  busy.value = `restore-${snapshot.id}`;
  error.value = "";
  try {
    await restoreContentSnapshot(props.instance.id, snapshot.id);
    toast.success("已回滚到快照", formatDate(snapshot.createdAt));
    void log.info(`回滚内容快照: ${snapshot.id}`, {
      component: "BackupModal",
      action: "restoreSnapshot",
      instanceId: props.instance.id,
    });
  } catch (err) {
    error.value = `回滚失败：${toMessage(err)}`;
    void log.error(error.value, { component: "BackupModal", action: "restoreSnapshot" });
  } finally {
    busy.value = null;
  }
}

async function removeSnapshot(snapshot: ContentSnapshot): Promise<void> {
  const confirmed = await askConfirm({
    title: "删除这份快照？",
    message: "删除后无法恢复（不会进入回收站）。",
    confirmText: "删除",
    danger: true,
  });
  if (!confirmed) return;

  busy.value = `delete-${snapshot.id}`;
  try {
    await deleteContentSnapshot(props.instance.id, snapshot.id);
    snapshots.value = snapshots.value.filter((item) => item.id !== snapshot.id);
  } catch (err) {
    error.value = `删除快照失败：${toMessage(err)}`;
    void log.error(error.value, { component: "BackupModal", action: "deleteSnapshot" });
  } finally {
    busy.value = null;
  }
}

async function togglePin(snapshot: ContentSnapshot): Promise<void> {
  const next = !snapshot.pinned;
  busy.value = `pin-${snapshot.id}`;
  try {
    await pinContentSnapshot(props.instance.id, snapshot.id, next);
    snapshot.pinned = next;
    toast.info(next ? "已标记为保留" : "已取消保留", "保留的快照不会被自动淘汰");
  } catch (err) {
    error.value = `设置保留标记失败：${toMessage(err)}`;
  } finally {
    busy.value = null;
  }
}

async function createBackup(): Promise<void> {
  busy.value = "create-world";
  try {
    const backup = await createWorldBackup(props.instance.id);
    backups.value = [backup, ...backups.value];
    toast.success("存档已备份", formatDate(backup.createdAt));
  } catch (err) {
    error.value = `创建存档备份失败：${toMessage(err)}`;
    void log.error(error.value, { component: "BackupModal", action: "createWorld" });
  } finally {
    busy.value = null;
  }
}

async function restoreBackup(backup: WorldBackup): Promise<void> {
  const confirmed = await askConfirm({
    title: "用这份备份覆盖当前存档？",
    message: `将恢复到 ${formatDate(backup.createdAt)} 的状态。\n当前存档会先暂存，解压成功后才删除。`,
    confirmText: "恢复",
    danger: true,
  });
  if (!confirmed) return;

  busy.value = `restore-${backup.name}`;
  try {
    await restoreWorldBackup(props.instance.id, backup.name);
    toast.success("存档已恢复", formatDate(backup.createdAt));
  } catch (err) {
    error.value = `恢复存档失败：${toMessage(err)}`;
    void log.error(error.value, { component: "BackupModal", action: "restoreWorld" });
  } finally {
    busy.value = null;
  }
}

const snapshotCount = computed(() => snapshots.value.length);

onMounted(() => {
  void load();
});
</script>

<template>
  <AppModal width="md" height="fixed" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <Archive :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-h2 font-semibold leading-5 text-ink">备份与回滚</h2>
        <p class="truncate text-micro text-ink-muted">
          {{ instance.displayName }} · 装坏了、玩崩了，都能退回来
        </p>
      </div>
      <AppButton
        v-if="tab === 'content'"
        variant="primary"
        size="sm"
        :loading="busy === 'create-snapshot'"
        :disabled="!!busy"
        @click="createSnapshot"
      >
        <Camera v-if="busy !== 'create-snapshot'" :size="14" />
        创建快照
      </AppButton>
      <AppButton
        v-else
        variant="primary"
        size="sm"
        :loading="busy === 'create-world'"
        :disabled="!!busy"
        @click="createBackup"
      >
        <Archive v-if="busy !== 'create-world'" :size="14" />
        备份存档
      </AppButton>
    </template>

    <template #toolbar>
      <div class="flex items-center gap-1">
        <button
          type="button"
          class="rounded-md px-3 py-1.5 text-caption font-medium transition-colors"
          :class="
            tab === 'content'
              ? 'bg-brand-500 text-white'
              : 'text-ink-muted hover:bg-ink/5 hover:text-ink'
          "
          @click="tab = 'content'"
        >
          内容快照（{{ snapshotCount }}）
        </button>
        <button
          type="button"
          class="rounded-md px-3 py-1.5 text-caption font-medium transition-colors"
          :class="
            tab === 'world'
              ? 'bg-brand-500 text-white'
              : 'text-ink-muted hover:bg-ink/5 hover:text-ink'
          "
          @click="tab = 'world'"
        >
          世界存档（{{ backups.length }}）
        </button>
      </div>
    </template>

    <p v-if="error" class="mb-3 rounded-md bg-danger/8 px-3 py-2 text-caption text-danger">
      {{ error }}
    </p>

    <div v-if="loading" class="flex h-48 items-center justify-center">
      <AppLoading :size="56" label="正在读取备份…" />
    </div>

    <!-- 内容快照 -->
    <template v-else-if="tab === 'content'">
      <div
        v-if="snapshots.length === 0"
        class="flex h-48 flex-col items-center justify-center gap-2 text-ink-muted"
      >
        <Camera :size="32" class="opacity-25" />
        <p class="text-body">还没有内容快照</p>
        <p class="text-micro">装 Mod 之前点一下「创建快照」，出问题可以一键退回</p>
      </div>
      <div v-else class="flex flex-col gap-2">
        <div
          v-for="snapshot in snapshots"
          :key="snapshot.id"
          class="flex items-center gap-3 rounded-lg border border-line/50 bg-surface/60 p-3"
        >
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="text-body font-medium text-ink">{{
                formatDate(snapshot.createdAt)
              }}</span>
              <span
                v-if="snapshot.pinned"
                class="rounded-xs bg-brand-500/12 px-1.5 py-0.5 text-micro font-medium text-brand-500"
                >保留</span
              >
            </div>
            <div class="truncate text-micro text-ink-muted">
              {{ reasonLabel(snapshot.reason) }} · {{ snapshot.fileCount }} 个文件 ·
              {{ formatSize(snapshot.sizeBytes) }}
            </div>
          </div>

          <AppButton
            variant="ghost"
            size="sm"
            :disabled="!!busy"
            :title="snapshot.pinned ? '取消保留' : '标记为保留（不自动淘汰）'"
            @click="togglePin(snapshot)"
          >
            <PinOff v-if="snapshot.pinned" :size="14" />
            <Pin v-else :size="14" />
          </AppButton>
          <AppButton
            variant="ghost"
            size="sm"
            :disabled="!!busy"
            title="删除这份快照"
            @click="removeSnapshot(snapshot)"
          >
            <Trash2 :size="14" />
          </AppButton>
          <AppButton
            variant="secondary"
            size="sm"
            :loading="busy === `restore-${snapshot.id}`"
            :disabled="!!busy"
            @click="restoreSnapshot(snapshot)"
          >
            <RotateCcw v-if="busy !== `restore-${snapshot.id}`" :size="14" />
            回滚
          </AppButton>
        </div>
      </div>
    </template>

    <!-- 世界存档 -->
    <template v-else>
      <div
        v-if="backups.length === 0"
        class="flex h-48 flex-col items-center justify-center gap-2 text-ink-muted"
      >
        <Archive :size="32" class="opacity-25" />
        <p class="text-body">还没有存档备份</p>
        <p class="text-micro">先玩一会儿，再点右上角「备份存档」</p>
      </div>
      <div v-else class="flex flex-col gap-2">
        <div
          v-for="backup in backups"
          :key="backup.name"
          class="flex items-center gap-3 rounded-lg border border-line/50 bg-surface/60 p-3"
        >
          <div class="min-w-0 flex-1">
            <div class="text-body font-medium text-ink">{{ formatDate(backup.createdAt) }}</div>
            <div class="truncate text-micro text-ink-muted">
              {{ backup.name }} · {{ formatSize(backup.size) }}
            </div>
          </div>
          <AppButton
            variant="secondary"
            size="sm"
            :loading="busy === `restore-${backup.name}`"
            :disabled="!!busy"
            @click="restoreBackup(backup)"
          >
            <RotateCcw v-if="busy !== `restore-${backup.name}`" :size="14" />
            恢复
          </AppButton>
        </div>
      </div>
    </template>
  </AppModal>
</template>
