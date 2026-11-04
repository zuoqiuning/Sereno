<script setup lang="ts">
/**
 * 存档管理弹窗：浏览 / 打开目录 / 删除。
 *
 * 备份与恢复在 WorldBackupModal（入口在实例菜单），这里只管「看与删」。
 */
import { computed, ref, watch } from "vue";
import { FolderOpen, Trash2, Globe, AlertTriangle, Loader2 } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppModal from "@/components/common/AppModal.vue";
import {
  deleteInstanceSave,
  listInstanceSaves,
  openSaveFolder,
  type SaveSummary,
} from "@/api/instance";
import { toMessage } from "@/utils/error";
import { formatBytes } from "@/utils/format";

const props = defineProps<{
  instance: { id: string; displayName: string };
  open: boolean;
}>();

const emit = defineEmits<{ close: [] }>();

const saves = ref<SaveSummary[]>([]);
const loading = ref(false);
const errorMessage = ref("");
/** 正在删除的存档名（按钮 loading + 防重复点击）。 */
const deletingName = ref<string | null>(null);
/** 二次确认：待删除的存档（null = 未进入确认态）。 */
const confirmingDelete = ref<SaveSummary | null>(null);

const sortedSaves = computed(() => saves.value);

function formatDate(secs: string): string {
  const num = Number(secs);
  if (!Number.isFinite(num) || num <= 0) return "未知时间";
  return new Date(num * 1000).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

async function load(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try {
    saves.value = await listInstanceSaves(props.instance.id);
  } catch (error) {
    errorMessage.value = toMessage(error);
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.open,
  (value) => {
    if (value) {
      confirmingDelete.value = null;
      void load();
    }
  },
);

async function onDelete(save: SaveSummary): Promise<void> {
  if (deletingName.value) return;
  deletingName.value = save.name;
  try {
    await deleteInstanceSave(props.instance.id, save.name);
    saves.value = saves.value.filter((item: SaveSummary) => item.name !== save.name);
    confirmingDelete.value = null;
  } catch (error) {
    errorMessage.value = toMessage(error);
  } finally {
    deletingName.value = null;
  }
}

async function onOpenFolder(save: SaveSummary): Promise<void> {
  try {
    await openSaveFolder(props.instance.id, save.name);
  } catch (error) {
    errorMessage.value = toMessage(error);
  }
}
</script>

<template>
  <AppModal :open="open" width="lg" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <Globe :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-h2 font-semibold leading-5 text-ink">存档管理</h2>
        <p class="truncate text-micro text-ink-muted">
          {{ instance.displayName }} · {{ saves.length }} 个世界
        </p>
      </div>
    </template>

    <div class="flex flex-col gap-3">
      <p v-if="errorMessage" class="text-caption text-danger">
        {{ errorMessage }}
      </p>

      <div
        v-if="loading"
        class="flex h-40 items-center justify-center gap-2 text-caption text-ink-muted"
      >
        <Loader2 :size="16" class="animate-spin" />
        正在扫描存档目录…
      </div>

      <div
        v-else-if="saves.length === 0"
        class="flex h-40 flex-col items-center justify-center gap-2 text-caption text-ink-muted"
      >
        <Globe :size="28" class="text-ink-muted/50" />
        这个实例还没有存档——进游戏创建世界后会出现在这里
      </div>

      <div v-else class="flex flex-col divide-y divide-line/40">
        <div v-for="save in sortedSaves" :key="save.name" class="flex items-center gap-3 py-3">
          <!-- 图标：游戏内截图，无图标用占位 -->
          <img
            v-if="save.iconDataUrl"
            :src="save.iconDataUrl"
            :alt="save.name"
            class="h-12 w-12 shrink-0 rounded-md border border-line/50 object-cover"
          />
          <div
            v-else
            class="flex h-12 w-12 shrink-0 items-center justify-center rounded-md bg-surface-muted"
          >
            <Globe :size="20" class="text-ink-muted" />
          </div>

          <div class="min-w-0 flex-1">
            <div class="truncate text-body font-medium text-ink">
              {{ save.name }}
            </div>
            <div class="truncate text-micro text-ink-muted tabular">
              最后游玩 {{ formatDate(save.lastPlayedSecs) }} ·
              {{ formatBytes(Number(save.sizeBytes)) }}
            </div>
          </div>

          <div class="flex shrink-0 items-center gap-1.5">
            <template v-if="confirmingDelete?.name === save.name">
              <!-- 二次确认：就地展开，避免误删大世界 -->
              <span class="flex items-center gap-1 text-micro text-warning">
                <AlertTriangle :size="13" />
                确认删除？
              </span>
              <AppButton
                variant="danger"
                size="sm"
                :loading="deletingName === save.name"
                @click="onDelete(save)"
              >
                删除
              </AppButton>
              <AppButton variant="ghost" size="sm" @click="confirmingDelete = null">
                取消
              </AppButton>
            </template>
            <template v-else>
              <AppButton variant="ghost" size="sm" @click="onOpenFolder(save)">
                <FolderOpen :size="14" />
                打开目录
              </AppButton>
              <AppButton
                variant="ghost"
                size="sm"
                class="text-danger hover:bg-danger/10"
                @click="confirmingDelete = save"
              >
                <Trash2 :size="14" />
                删除
              </AppButton>
            </template>
          </div>
        </div>
      </div>

      <p class="text-micro text-ink-muted">
        提示：删除的存档会尽量进入系统回收站；需要保留完整副本请使用实例菜单的「世界备份」。
      </p>
    </div>
  </AppModal>
</template>
