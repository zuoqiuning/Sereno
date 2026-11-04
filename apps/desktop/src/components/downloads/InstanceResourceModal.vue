<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import type { Component } from "vue";
import {
  Database,
  Download,
  FolderOpen,
  Palette,
  Puzzle,
  RefreshCw,
  Sparkles,
  Trash2,
  Upload,
} from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import { isApiError } from "@/api/invoke";
import {
  checkLocalResourceUpdates,
  importLocalResources,
  listLocalResources,
  openLocalResourceFolder,
  removeLocalResources,
  setLocalResourceEnabled,
  setLocalResourcesEnabled,
  updateLocalResource,
} from "@/api/local";
import { askConfirm } from "@/composables/useConfirm";
import type {
  InstanceMetadata,
  LocalResource,
  LocalResourceKind,
  LocalResourceMeta,
  LocalResourceUpdate,
} from "@/types";

/**
 * 实例资源管理：查看已安装的 MOD / 资源包 / 光影，启用禁用、检查更新与导入。
 */
const props = withDefaults(defineProps<{ instance: InstanceMetadata; open?: boolean }>(), {
  open: true,
});

const emit = defineEmits<{
  (e: "close"): void;
  (e: "browse", kind: LocalResourceKind): void;
}>();

interface KindOption {
  id: LocalResourceKind;
  label: string;
  icon: Component;
}

const kinds: KindOption[] = [
  { id: "mod", label: "MOD", icon: Puzzle },
  { id: "resourcepack", label: "资源包", icon: Palette },
  { id: "datapack", label: "数据包", icon: Database },
  { id: "shader", label: "光影", icon: Sparkles },
];

const filterOptions = [
  { id: "all", label: "全部" },
  { id: "enabled", label: "启用" },
  { id: "disabled", label: "禁用" },
] as const;

type FilterId = (typeof filterOptions)[number]["id"];

const activeKind = ref<LocalResourceKind>("mod");
const keyword = ref("");
const filter = ref<FilterId>("all");
const items = ref<LocalResource[]>([]);
const loading = ref(true);
const error = ref("");
const notice = ref("");
const noticeTone = ref<"success" | "error" | "muted">("muted");
const checking = ref(false);
const importing = ref(false);
const toggling = ref<string | null>(null);
const updating = ref<string | null>(null);
const updatingAll = ref(false);
const batching = ref(false);
/** 勾选中的文件：批量启用 / 禁用 / 删除的目标。 */
const selected = ref<string[]>([]);
const metadata = ref<Record<string, LocalResourceMeta>>({});
const updates = ref<Record<string, LocalResourceUpdate>>({});

const activeKindOption = computed(
  () => kinds.find((kind) => kind.id === activeKind.value) ?? kinds[0],
);

const enabledCount = computed(() => items.value.filter((item) => item.enabled).length);
const disabledCount = computed(() => items.value.filter((item) => !item.enabled).length);
const updateCount = computed(() => Object.keys(updates.value).length);

const filteredItems = computed(() => {
  const text = keyword.value.trim().toLowerCase();
  return items.value.filter((item) => {
    if (filter.value === "enabled" && !item.enabled) return false;
    if (filter.value === "disabled" && item.enabled) return false;
    if (!text) return true;
    return `${item.displayName} ${item.fileName}`.toLowerCase().includes(text);
  });
});

const loaderLabel = computed(() => {
  switch (props.instance.loader.type) {
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
});

function toErrorMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

function baseName(fileName: string): string {
  return fileName.replace(/\.disabled$/i, "");
}

function updateFor(item: LocalResource): LocalResourceUpdate | undefined {
  return updates.value[baseName(item.fileName)];
}

function metaFor(item: LocalResource): LocalResourceMeta | undefined {
  return metadata.value[baseName(item.fileName)];
}

function countFor(filterId: FilterId): number {
  if (filterId === "enabled") return enabledCount.value;
  if (filterId === "disabled") return disabledCount.value;
  return items.value.length;
}

/** 当前筛选结果是否已全选。 */
const allSelected = computed(
  () =>
    filteredItems.value.length > 0 &&
    filteredItems.value.every((item) => selected.value.includes(item.fileName)),
);

function isSelected(fileName: string): boolean {
  return selected.value.includes(fileName);
}

function toggleSelect(fileName: string): void {
  selected.value = isSelected(fileName)
    ? selected.value.filter((name) => name !== fileName)
    : [...selected.value, fileName];
}

function toggleSelectAll(): void {
  selected.value = allSelected.value ? [] : filteredItems.value.map((item) => item.fileName);
}

function clearSelection(): void {
  selected.value = [];
}

/** 批量启用 / 禁用。 */
async function batchSetEnabled(enabled: boolean): Promise<void> {
  if (selected.value.length === 0) return;
  const names = [...selected.value];
  batching.value = true;
  try {
    const count = await setLocalResourcesEnabled(
      props.instance.id,
      activeKind.value,
      names,
      enabled,
    );
    setNotice(`已${enabled ? "启用" : "禁用"} ${count} 个资源`, "success");
    selected.value = [];
    await loadResources();
  } catch (err) {
    setNotice(`批量操作失败：${toErrorMessage(err)}`, "error");
  } finally {
    batching.value = false;
  }
}

/** 删除一个或多个本地资源。 */
async function removeFiles(fileNames: string[]): Promise<void> {
  if (fileNames.length === 0) return;
  const ok = await askConfirm({
    title:
      fileNames.length === 1
        ? `删除「${fileNames[0]}」？`
        : `删除选中的 ${fileNames.length} 个资源？`,
    message: "文件会从实例目录中永久删除，需要重新下载才能恢复。",
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;

  batching.value = true;
  try {
    const count = await removeLocalResources(props.instance.id, activeKind.value, fileNames);
    setNotice(`已删除 ${count} 个资源`, "success");
    selected.value = [];
    await loadResources();
  } catch (err) {
    setNotice(`删除失败：${toErrorMessage(err)}`, "error");
  } finally {
    batching.value = false;
  }
}

function setNotice(message: string, tone: "success" | "error" | "muted" = "muted"): void {
  notice.value = message;
  noticeTone.value = tone;
}

async function loadResources(): Promise<void> {
  loading.value = true;
  error.value = "";
  selected.value = [];
  try {
    items.value = await listLocalResources(props.instance.id, activeKind.value);
  } catch (err) {
    error.value = `读取本地资源失败：${toErrorMessage(err)}`;
    items.value = [];
  } finally {
    loading.value = false;
  }
}

async function toggleResource(item: LocalResource): Promise<void> {
  toggling.value = item.fileName;
  try {
    await setLocalResourceEnabled(
      props.instance.id,
      activeKind.value,
      item.fileName,
      !item.enabled,
    );
    await loadResources();
  } catch (err) {
    setNotice(`切换启用状态失败：${toErrorMessage(err)}`, "error");
  } finally {
    toggling.value = null;
  }
}

/** 扫描本地文件：同时补齐图标 / 名称 / 介绍，并找出可更新版本。 */
async function scanResources(showNotice = true): Promise<void> {
  if (items.value.length === 0) {
    if (showNotice) setNotice("当前分类还没有已安装的资源");
    return;
  }

  checking.value = true;
  if (showNotice) setNotice("");
  try {
    const scan = await checkLocalResourceUpdates(props.instance.id, activeKind.value);
    metadata.value = Object.fromEntries(
      scan.metadata.map((entry) => [baseName(entry.fileName), entry]),
    );
    updates.value = Object.fromEntries(
      scan.updates.map((entry) => [baseName(entry.fileName), entry]),
    );
    if (showNotice) {
      setNotice(
        scan.updates.length > 0
          ? `发现 ${scan.updates.length} 个可更新资源`
          : "没有发现可更新的资源",
        scan.updates.length > 0 ? "success" : "muted",
      );
    }
  } catch (err) {
    setNotice(`检查更新失败：${toErrorMessage(err)}`, "error");
  } finally {
    checking.value = false;
  }
}

async function applyUpdate(item: LocalResource): Promise<void> {
  const update = updateFor(item);
  if (!update) return;

  updating.value = item.fileName;
  try {
    await updateLocalResource(
      props.instance.id,
      activeKind.value,
      item.fileName,
      update.newFileName,
      update.downloadUrl,
      update.sha1 ?? null,
    );
    const next = { ...updates.value };
    delete next[baseName(item.fileName)];
    updates.value = next;
    await loadResources();
    setNotice(`已更新「${item.displayName}」`, "success");
  } catch (err) {
    setNotice(`更新失败：${toErrorMessage(err)}`, "error");
  } finally {
    updating.value = null;
  }
}

async function applyAllUpdates(): Promise<void> {
  const targets = items.value.filter((item) => item.enabled && updateFor(item));
  if (targets.length === 0) return;

  updatingAll.value = true;
  let done = 0;
  const failed: string[] = [];

  for (const item of targets) {
    const update = updateFor(item);
    if (!update) continue;
    try {
      await updateLocalResource(
        props.instance.id,
        activeKind.value,
        item.fileName,
        update.newFileName,
        update.downloadUrl,
        update.sha1 ?? null,
      );
      const next = { ...updates.value };
      delete next[baseName(item.fileName)];
      updates.value = next;
      done += 1;
    } catch {
      failed.push(item.displayName);
    }
  }

  await loadResources();
  if (failed.length > 0) {
    setNotice(`已更新 ${done} 个，失败 ${failed.length} 个：${failed.join("、")}`, "error");
  } else {
    setNotice(`已更新 ${done} 个资源`, "success");
  }
  updatingAll.value = false;
}

async function importResources(): Promise<void> {
  importing.value = true;
  try {
    const imported = await importLocalResources(props.instance.id, activeKind.value);
    if (imported > 0) {
      await loadResources();
      setNotice(`已导入 ${imported} 个资源`, "success");
      if (items.value.length > 0) void scanResources(false);
    }
  } catch (err) {
    setNotice(`导入失败：${toErrorMessage(err)}`, "error");
  } finally {
    importing.value = false;
  }
}

async function openFolder(): Promise<void> {
  try {
    await openLocalResourceFolder(props.instance.id, activeKind.value);
  } catch (err) {
    setNotice(`打开文件夹失败：${toErrorMessage(err)}`, "error");
  }
}

async function loadAndScan(): Promise<void> {
  await loadResources();
  if (items.value.length > 0) {
    void scanResources(false);
  }
}

watch(activeKind, () => {
  keyword.value = "";
  filter.value = "all";
  metadata.value = {};
  updates.value = {};
  notice.value = "";
  void loadAndScan();
});

onMounted(() => {
  void loadAndScan();
});
</script>

<template>
  <AppModal width="lg" height="fixed" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <component :is="activeKindOption.icon" :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-h2 font-semibold leading-5 text-ink">
          {{ instance.displayName }}
        </h2>
        <p class="truncate text-micro text-ink-muted">
          {{ instance.gameVersion }} · {{ loaderLabel }} · 资源管理
        </p>
      </div>
      <span class="text-micro text-ink-muted"
        >启用 {{ enabledCount }} · 禁用 {{ disabledCount }}</span
      >
    </template>

    <div class="flex flex-wrap items-center gap-2">
      <div class="min-w-55 flex-1">
        <AppInput v-model="keyword" placeholder="搜索已安装资源..." />
      </div>
      <AppButton variant="secondary" size="sm" :loading="checking" @click="scanResources(true)">
        <RefreshCw :size="14" />
        检查更新
      </AppButton>
      <AppButton
        v-if="updateCount > 0"
        variant="secondary"
        size="sm"
        :loading="updatingAll"
        :disabled="checking || importing"
        @click="applyAllUpdates"
      >
        <RefreshCw :size="14" />
        全部更新 ({{ updateCount }})
      </AppButton>
      <AppButton variant="secondary" size="sm" :loading="importing" @click="importResources">
        <Upload :size="14" />
        从文件安装
      </AppButton>
      <AppButton variant="secondary" size="sm" @click="openFolder">
        <FolderOpen :size="14" />
        打开文件夹
      </AppButton>
      <AppButton variant="primary" size="sm" @click="emit('browse', activeKind)">
        <Download :size="14" />
        下载新资源
      </AppButton>
    </div>

    <div class="mt-3 flex flex-wrap items-center justify-between gap-2">
      <div class="flex gap-1 rounded-md bg-ink/5 p-1">
        <button
          v-for="kind in kinds"
          :key="kind.id"
          type="button"
          class="flex items-center gap-1.5 rounded-sm px-3 py-1.5 text-caption font-medium transition-colors"
          :class="
            activeKind === kind.id
              ? 'bg-surface text-ink shadow-sm'
              : 'text-ink-muted hover:text-ink-soft'
          "
          @click="activeKind = kind.id"
        >
          <component :is="kind.icon" :size="14" />
          {{ kind.label }}
        </button>
      </div>

      <div class="flex items-center gap-2">
        <div class="flex items-center gap-1">
          <button
            v-for="option in filterOptions"
            :key="option.id"
            type="button"
            class="rounded-md px-2.5 py-1 text-caption font-medium transition-colors"
            :class="filter === option.id ? 'bg-brand-500 text-white' : 'text-ink-soft hover:bg-ink/5'"
            @click="filter = option.id"
          >
            {{ option.label }} {{ countFor(option.id) }}
          </button>
        </div>
        <button
          v-if="filteredItems.length > 0"
          type="button"
          class="rounded-md px-2.5 py-1 text-caption font-medium text-ink-muted transition-colors hover:bg-ink/5 hover:text-ink"
          @click="toggleSelectAll"
        >
          {{ allSelected ? "取消全选" : `全选 ${filteredItems.length}` }}
        </button>
      </div>
    </div>

    <p
      v-if="notice"
      class="mt-3 text-caption"
      :class="
        noticeTone === 'error'
          ? 'text-danger'
          : noticeTone === 'success'
            ? 'text-success'
            : 'text-ink-muted'
      "
    >
      {{ notice }}
    </p>

    <!-- 批量操作条：勾选后出现，避免每一行都堆按钮 -->
    <div
      v-if="selected.length > 0"
      class="mt-3 flex items-center gap-2 rounded-md bg-surface-muted px-3 py-2"
    >
      <span class="text-caption font-medium text-ink-soft">已选 {{ selected.length }} 项</span>
      <AppButton
        variant="secondary"
        size="xs"
        :disabled="batching"
        @click="batchSetEnabled(true)"
      >
        全部启用
      </AppButton>
      <AppButton
        variant="secondary"
        size="xs"
        :disabled="batching"
        @click="batchSetEnabled(false)"
      >
        全部禁用
      </AppButton>
      <AppButton
        variant="danger"
        size="xs"
        :loading="batching"
        @click="removeFiles([...selected])"
      >
        删除
      </AppButton>
      <button
        type="button"
        class="ml-auto text-caption text-ink-muted transition-colors hover:text-ink"
        @click="clearSelection"
      >
        取消选择
      </button>
    </div>

    <div v-if="loading" class="flex h-64 items-center justify-center">
      <AppLoading :size="64" label="正在读取本地资源…" />
    </div>
    <div v-else-if="error" class="flex h-64 flex-col items-center justify-center gap-3">
      <p class="text-body text-danger">{{ error }}</p>
      <AppButton variant="secondary" size="sm" @click="loadResources">重试</AppButton>
    </div>
    <div
      v-else-if="filteredItems.length === 0"
      class="flex h-56 flex-col items-center justify-center gap-2 text-ink-muted"
    >
      <component :is="activeKindOption.icon" :size="32" class="opacity-25" />
      <p class="text-body">
        {{ items.length === 0 ? "该分类还没有安装资源" : "没有匹配的资源" }}
      </p>
      <p v-if="items.length === 0" class="text-micro">可以用「从文件安装」或「下载新资源」添加</p>
    </div>
    <div v-else class="mt-3 flex flex-col gap-2">
      <div
        v-for="item in filteredItems"
        :key="item.fileName"
        class="flex items-center gap-3 rounded-lg border bg-surface px-3 py-2.5 transition-fast"
        :class="[
          item.enabled ? 'border-line/50' : 'border-line/30 bg-surface-muted/30',
          isSelected(item.fileName) ? 'border-brand-500/45 bg-[var(--alpha-brand-4)]' : '',
        ]"
      >
        <input
          type="checkbox"
          class="h-3.5 w-3.5 shrink-0 accent-brand-500"
          :checked="isSelected(item.fileName)"
          :aria-label="`选择 ${item.displayName}`"
          @change="toggleSelect(item.fileName)"
        />
        <img
          v-if="metaFor(item)?.iconUrl || updateFor(item)?.iconUrl"
          :src="metaFor(item)?.iconUrl ?? updateFor(item)?.iconUrl ?? ''"
          referrerpolicy="no-referrer"
          class="h-10 w-10 shrink-0 rounded-lg bg-ink/5 object-cover"
          loading="lazy"
        />
        <div
          v-else
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg"
          :class="item.enabled ? 'bg-brand-500/10 text-brand-500' : 'bg-ink/5 text-ink-muted'"
        >
          <component :is="activeKindOption.icon" :size="18" />
        </div>

        <div class="min-w-0 flex-1" :class="item.enabled ? '' : 'opacity-60'">
          <div class="flex flex-wrap items-center gap-2">
            <span class="truncate text-body font-medium text-ink">
              {{ metaFor(item)?.title ?? updateFor(item)?.title ?? item.displayName }}
            </span>
            <span
              v-if="!item.enabled"
              class="rounded bg-ink/5 px-1.5 py-0.5 text-micro text-ink-muted"
            >
              已禁用
            </span>
            <span
              v-if="updateFor(item)"
              class="rounded bg-success-soft px-1.5 py-0.5 text-micro text-success"
            >
              可更新 {{ updateFor(item)?.versionName }}
            </span>
          </div>
          <p class="mt-0.5 truncate text-micro text-ink-muted" :title="item.fileName">
            {{ metaFor(item)?.description || item.fileName }}
          </p>
        </div>

        <AppButton
          v-if="updateFor(item)"
          variant="primary"
          size="sm"
          :loading="updating === item.fileName"
          :disabled="!!toggling || updatingAll"
          @click="applyUpdate(item)"
        >
          更新
        </AppButton>

        <button
          type="button"
          role="switch"
          :aria-checked="item.enabled"
          :disabled="toggling === item.fileName || updatingAll || batching"
          :title="item.enabled ? '禁用' : '启用'"
          class="relative h-5 w-9 shrink-0 rounded-full transition-colors disabled:opacity-40"
          :class="item.enabled ? 'bg-brand-500' : 'bg-ink/15'"
          @click="toggleResource(item)"
        >
          <span
            class="absolute left-0.5 top-0.5 h-4 w-4 rounded-full bg-white shadow-sm transition-transform"
            :class="item.enabled ? 'translate-x-4' : ''"
          />
        </button>

        <button
          type="button"
          class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-danger-soft hover:text-danger disabled:opacity-40"
          :title="`删除 ${item.displayName}`"
          :disabled="batching"
          @click="removeFiles([item.fileName])"
        >
          <Trash2 :size="14" />
        </button>
      </div>
    </div>
  </AppModal>
</template>
