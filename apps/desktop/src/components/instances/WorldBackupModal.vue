<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Archive, RotateCcw } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import { createWorldBackup, listWorldBackups, restoreWorldBackup } from "@/api/backup";
import { log } from "@/api/settings";
import type { InstanceMetadata, WorldBackup } from "@/types";

const props = withDefaults(defineProps<{ instance: InstanceMetadata; open?: boolean }>(), {
  open: true,
});
const emit = defineEmits<{ (e: "close"): void }>();

const loading = ref(true);
const creating = ref(false);
const restoring = ref<string | null>(null);
const backups = ref<WorldBackup[]>([]);
const error = ref("");
const notice = ref("");

function formatSize(bytes: number): string {
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

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    backups.value = await listWorldBackups(props.instance.id);
  } catch (err) {
    error.value = `读取备份失败：${String(err)}`;
    void log.error(error.value, { component: "WorldBackupModal", action: "list" });
  } finally {
    loading.value = false;
  }
}

async function create(): Promise<void> {
  creating.value = true;
  notice.value = "";
  try {
    const backup = await createWorldBackup(props.instance.id);
    backups.value = [backup, ...backups.value];
    notice.value = `已创建备份 ${formatDate(backup.createdAt)}`;
    void log.info(`创建存档备份: ${backup.name}`, {
      component: "WorldBackupModal",
      action: "create",
    });
  } catch (err) {
    error.value = `创建备份失败：${String(err)}`;
    void log.error(error.value, { component: "WorldBackupModal", action: "create" });
  } finally {
    creating.value = false;
  }
}

async function restore(backup: WorldBackup): Promise<void> {
  if (
    !confirm(
      `确定用 ${formatDate(backup.createdAt)} 的备份覆盖当前存档吗？\n当前存档会先暂存，解压成功后再删除。`,
    )
  ) {
    return;
  }

  restoring.value = backup.name;
  notice.value = "";
  try {
    await restoreWorldBackup(props.instance.id, backup.name);
    notice.value = `已恢复 ${formatDate(backup.createdAt)} 的备份`;
    void log.info(`恢复存档备份: ${backup.name}`, {
      component: "WorldBackupModal",
      action: "restore",
    });
  } catch (err) {
    error.value = `恢复备份失败：${String(err)}`;
    void log.error(error.value, { component: "WorldBackupModal", action: "restore" });
  } finally {
    restoring.value = null;
  }
}

onMounted(() => {
  void load();
});
</script>

<template>
  <AppModal width="md" height="fixed" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
      >
        <Archive :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-[16px] font-semibold leading-5 text-ink">存档备份</h2>
        <p class="truncate text-[11px] text-ink-muted">
          {{ instance.displayName }} · 备份保存在启动器数据目录
        </p>
      </div>
      <AppButton variant="primary" size="sm" :loading="creating" @click="create">
        <Archive v-if="!creating" :size="14" />
        立即备份
      </AppButton>
    </template>

    <p v-if="notice" class="mb-3 text-[12px] text-success">{{ notice }}</p>
    <p v-if="error" class="mb-3 text-[12px] text-danger">{{ error }}</p>

    <div v-if="loading" class="flex h-48 items-center justify-center">
      <AppLoading :size="56" label="正在读取备份…" />
    </div>
    <div
      v-else-if="backups.length === 0"
      class="flex h-48 flex-col items-center justify-center gap-2 text-ink-muted"
    >
      <Archive :size="32" class="opacity-25" />
      <p class="text-[13px]">还没有备份</p>
      <p class="text-[11px]">先玩一会儿，再点右上角「立即备份」</p>
    </div>
    <div v-else class="flex flex-col gap-2">
      <div
        v-for="backup in backups"
        :key="backup.name"
        class="flex items-center gap-3 rounded-lg border border-line/50 bg-surface/60 p-3"
      >
        <div class="min-w-0 flex-1">
          <div class="text-[13px] font-medium text-ink">{{ formatDate(backup.createdAt) }}</div>
          <div class="truncate text-[11px] text-ink-muted">
            {{ backup.name }} · {{ formatSize(backup.size) }}
          </div>
        </div>
        <AppButton
          variant="secondary"
          size="sm"
          :loading="restoring === backup.name"
          :disabled="!!restoring || creating"
          @click="restore(backup)"
        >
          <RotateCcw v-if="restoring !== backup.name" :size="14" />
          恢复
        </AppButton>
      </div>
    </div>
  </AppModal>
</template>
