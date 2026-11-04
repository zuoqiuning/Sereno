<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { FileArchive, Package, Puzzle } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppModal from "@/components/common/AppModal.vue";
import AppSelect from "@/components/common/AppSelect.vue";
import { listInstances } from "@/api/instance";
import { importLocalResourcesFromPaths } from "@/api/local";
import { useToastStore } from "@/stores/toast";
import type { InstanceMetadata, LocalResourceKind } from "@/types";

/**
 * 拖拽导入。
 *
 * 把文件从桌面拖进窗口后，这里问两件事：**装到哪个实例**、**zip 算哪一类资源**。
 * 问清楚了才动手——直接装到「最近使用的实例」是猜用户的意图，装错了很难察觉。
 */
const props = defineProps<{ open: boolean; paths: string[] }>();
const emit = defineEmits<{ close: [] }>();

const toast = useToastStore();
const instances = ref<InstanceMetadata[]>([]);
const targetId = ref("");
const zipKind = ref<LocalResourceKind>("resourcepack");
const importing = ref(false);

const modFiles = computed(() => props.paths.filter((p) => /\.(jar|litemod)$/i.test(p)));
const zipFiles = computed(() => props.paths.filter((p) => /\.zip$/i.test(p)));
const modpackFiles = computed(() => props.paths.filter((p) => /\.mrpack$/i.test(p)));
const otherFiles = computed(() =>
  props.paths.filter((p) => !/\.(jar|litemod|zip|mrpack)$/i.test(p)),
);

const instanceOptions = computed(() =>
  instances.value
    .filter((item) => item.status === "ready")
    .map((item) => ({ value: item.id, label: item.displayName })),
);

const zipKindOptions = [
  { value: "resourcepack", label: "资源包" },
  { value: "shader", label: "光影" },
  { value: "datapack", label: "数据包" },
];

function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

async function loadInstances(): Promise<void> {
  try {
    instances.value = await listInstances();
    if (!targetId.value) {
      targetId.value = instanceOptions.value[0]?.value ?? "";
    }
  } catch {
    instances.value = [];
  }
}

async function doImport(): Promise<void> {
  if (!targetId.value || importing.value) return;
  importing.value = true;
  try {
    let count = 0;
    if (modFiles.value.length > 0) {
      count += await importLocalResourcesFromPaths(targetId.value, "mod", modFiles.value);
    }
    if (zipFiles.value.length > 0) {
      count += await importLocalResourcesFromPaths(targetId.value, zipKind.value, zipFiles.value);
    }
    const target = instances.value.find((item) => item.id === targetId.value);
    toast.success(`已导入 ${count} 个文件`, target?.displayName ?? "");
    emit("close");
  } catch (err) {
    toast.error("导入失败", err instanceof Error ? err.message : String(err));
  } finally {
    importing.value = false;
  }
}

watch(
  () => props.open,
  (open) => {
    if (open) void loadInstances();
  },
);

onMounted(() => {
  if (props.open) void loadInstances();
});
</script>

<template>
  <AppModal width="sm" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <FileArchive :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-h2 font-semibold leading-5 text-ink">导入拖入的文件</h2>
        <p class="truncate text-micro text-ink-muted">共 {{ paths.length }} 个文件</p>
      </div>
    </template>

    <div class="flex flex-col gap-4">
      <!-- 文件清单 -->
      <div class="flex flex-col gap-2">
        <div v-if="modFiles.length" class="flex items-start gap-2.5">
          <Puzzle :size="15" class="mt-0.5 shrink-0 text-ink-muted" />
          <div class="min-w-0 flex-1">
            <p class="text-caption font-medium text-ink">
              Mod（{ modFiles.length } 个，将安装到 mods 目录）
            </p>
            <p class="mt-0.5 truncate text-micro text-ink-muted">
              {{ modFiles.map(baseName).join("、") }}
            </p>
          </div>
        </div>

        <div v-if="zipFiles.length" class="flex items-start gap-2.5">
          <FileArchive :size="15" class="mt-0.5 shrink-0 text-ink-muted" />
          <div class="min-w-0 flex-1">
            <p class="text-caption font-medium text-ink">
              压缩包（{ zipFiles.length }} 个，按下方分类安装）
            </p>
            <p class="mt-0.5 truncate text-micro text-ink-muted">
              {{ zipFiles.map(baseName).join("、") }}
            </p>
          </div>
        </div>

        <div v-if="modpackFiles.length" class="flex items-start gap-2.5">
          <Package :size="15" class="mt-0.5 shrink-0 text-ink-muted" />
          <div class="min-w-0 flex-1">
            <p class="text-caption font-medium text-ink">整合包（{ modpackFiles.length }} 个）</p>
            <p class="mt-0.5 text-micro text-ink-muted">
              整合包需要连同游戏版本一起安装，请到「下载」页面选择对应整合包安装。
            </p>
          </div>
        </div>

        <div v-if="otherFiles.length" class="flex items-start gap-2.5">
          <Package :size="15" class="mt-0.5 shrink-0 text-ink-muted" />
          <div class="min-w-0 flex-1">
            <p class="text-caption font-medium text-ink">
              不支持的文件（{ otherFiles.length }} 个）
            </p>
            <p class="mt-0.5 truncate text-micro text-ink-muted">
              {{ otherFiles.map(baseName).join("、") }}
            </p>
          </div>
        </div>
      </div>

      <!-- 目标实例 -->
      <div class="flex flex-col gap-1.5">
        <span class="text-caption font-medium text-ink">安装到哪个实例</span>
        <AppSelect
          v-model="targetId"
          :options="instanceOptions"
          placeholder="选择一个已安装完成的实例"
        />
        <p v-if="instanceOptions.length === 0" class="text-micro text-danger">
          没有可用的实例，请先到「下载」页面安装一个游戏版本。
        </p>
      </div>

      <!-- zip 归类 -->
      <div v-if="zipFiles.length > 0" class="flex flex-col gap-1.5">
        <span class="text-caption font-medium text-ink">压缩包归类为</span>
        <AppSelect v-model="zipKind" :options="zipKindOptions" />
      </div>
    </div>

    <template #footer>
      <div class="flex justify-end gap-2">
        <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
        <AppButton
          variant="primary"
          :loading="importing"
          :disabled="
            !targetId ||
            importing ||
            instanceOptions.length === 0 ||
            modFiles.length + zipFiles.length === 0
          "
          @click="doImport"
        >
          导入
        </AppButton>
      </div>
    </template>
  </AppModal>
</template>
