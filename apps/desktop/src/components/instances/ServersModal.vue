<script setup lang="ts">
/**
 * 服务器列表弹窗：浏览 / 添加 / 删除 `servers.dat`。
 *
 * 数据与顺序和游戏内多人游戏列表完全一致（读的是同一个文件）。
 */
import { computed, ref, watch } from "vue";
import { Server, Loader2, Plus, Trash2, Globe } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppModal from "@/components/common/AppModal.vue";
import {
  addInstanceServer,
  deleteInstanceServer,
  listInstanceServers,
  type ServerSummary,
} from "@/api/instance";
import { toMessage } from "@/utils/error";

const props = defineProps<{
  instance: { id: string; displayName: string };
  open: boolean;
}>();

const emit = defineEmits<{ close: [] }>();

const servers = ref<ServerSummary[]>([]);
const loading = ref(false);
const errorMessage = ref("");
/** 正在添加（按钮 loading）。 */
const adding = ref(false);
/** 待删除的下标（null = 未进入确认态）。 */
const confirmingIndex = ref<number | null>(null);
/** 删除中。 */
const deletingIndex = ref<number | null>(null);

const newName = ref("");
const newAddress = ref("");
const canAdd = computed(
  () => newAddress.value.trim().length > 0 && newName.value.trim().length > 0,
);

async function load(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try {
    servers.value = await listInstanceServers(props.instance.id);
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
      confirmingIndex.value = null;
      newName.value = "";
      newAddress.value = "";
      void load();
    }
  },
);

async function onAdd(): Promise<void> {
  if (!canAdd.value || adding.value) return;
  adding.value = true;
  errorMessage.value = "";
  try {
    await addInstanceServer(props.instance.id, newName.value.trim(), newAddress.value.trim());
    newName.value = "";
    newAddress.value = "";
    await load();
  } catch (error) {
    errorMessage.value = toMessage(error);
  } finally {
    adding.value = false;
  }
}

async function onDelete(server: ServerSummary): Promise<void> {
  if (deletingIndex.value !== null) return;
  deletingIndex.value = server.index;
  try {
    await deleteInstanceServer(props.instance.id, server.index);
    servers.value = servers.value.filter((item) => item.index !== server.index);
    confirmingIndex.value = null;
  } catch (error) {
    errorMessage.value = toMessage(error);
  } finally {
    deletingIndex.value = null;
  }
}
</script>

<template>
  <AppModal :open="open" width="lg" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <Server :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-h2 font-semibold leading-5 text-ink">服务器列表</h2>
        <p class="truncate text-micro text-ink-muted">
          {{ instance.displayName }} · {{ servers.length }} 个服务器 · 与游戏内多人列表同步
        </p>
      </div>
    </template>

    <div class="flex flex-col gap-3">
      <p v-if="errorMessage" class="text-caption text-danger">
        {{ errorMessage }}
      </p>

      <!-- 添加表单 -->
      <div class="flex items-center gap-2">
        <div class="w-40">
          <AppInput v-model="newName" placeholder="名称（如 生存服）" />
        </div>
        <div class="min-w-0 flex-1">
          <AppInput
            v-model="newAddress"
            clearable
            placeholder="服务器地址（如 mc.example.com 或 1.2.3.4:25565）"
            @keydown.enter="onAdd"
          />
        </div>
        <AppButton variant="primary" size="sm" :disabled="!canAdd" :loading="adding" @click="onAdd">
          <Plus :size="14" />
          添加
        </AppButton>
      </div>

      <div
        v-if="loading"
        class="flex h-32 items-center justify-center gap-2 text-caption text-ink-muted"
      >
        <Loader2 :size="16" class="animate-spin" />
        正在读取服务器列表…
      </div>

      <div
        v-else-if="servers.length === 0"
        class="flex h-32 flex-col items-center justify-center gap-2 text-caption text-ink-muted"
      >
        <Globe :size="28" class="text-ink-muted/50" />
        还没有添加服务器——用上面的表单添加，游戏内即可直接连接
      </div>

      <div v-else class="flex flex-col divide-y divide-line/40">
        <div v-for="server in servers" :key="server.index" class="flex items-center gap-3 py-3">
          <img
            v-if="server.iconDataUrl"
            :src="server.iconDataUrl"
            :alt="server.name"
            class="h-10 w-10 shrink-0 rounded-md border border-line/50 object-cover"
          />
          <div
            v-else
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-surface-muted"
          >
            <Server :size="18" class="text-ink-muted" />
          </div>

          <div class="min-w-0 flex-1">
            <div class="truncate text-body font-medium text-ink">
              {{ server.name || "未命名服务器" }}
            </div>
            <div class="truncate font-mono text-micro text-ink-muted tabular">
              {{ server.address }}
            </div>
          </div>

          <template v-if="confirmingIndex === server.index">
            <span class="shrink-0 text-micro text-warning">确认删除？</span>
            <AppButton
              variant="danger"
              size="sm"
              :loading="deletingIndex === server.index"
              @click="onDelete(server)"
            >
              删除
            </AppButton>
            <AppButton variant="ghost" size="sm" @click="confirmingIndex = null"> 取消 </AppButton>
          </template>
          <AppButton
            v-else
            variant="ghost"
            size="sm"
            class="text-danger hover:bg-danger/10"
            @click="confirmingIndex = server.index"
          >
            <Trash2 :size="14" />
            删除
          </AppButton>
        </div>
      </div>

      <p class="text-micro text-ink-muted">
        提示：这里的修改直接写进游戏的 servers.dat，游戏内多人游戏列表会同步显示。
      </p>
    </div>
  </AppModal>
</template>
