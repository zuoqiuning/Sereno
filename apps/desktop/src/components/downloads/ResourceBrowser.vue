<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  AlertTriangle,
  ChevronRight,
  Download,
  Loader2,
  Package,
  Search,
  SlidersHorizontal,
} from "@lucide/vue";
import { useRoute } from "vue-router";

import AppButton from "@/components/common/AppButton.vue";
import { toMessage } from "@/utils/error";
import AppCard from "@/components/common/AppCard.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppPagination from "@/components/common/AppPagination.vue";
import AppSelect from "@/components/common/AppSelect.vue";
import ResourceDetailModal from "@/components/downloads/ResourceDetailModal.vue";
import { getRunningInstances, listInstances } from "@/api/instance";
import { isApiError } from "@/api/invoke";
import {
  searchResources,
  downloadResource,
  getCfModVersions,
  getModVersions,
  installResourceDependency,
} from "@/api/resource";
import { useToastStore } from "@/stores/toast";
import { useTaskStore } from "@/stores/task";
import { useModalState } from "@/composables/useModalState";
import { useVersionStore } from "@/stores/version";
import type {
  InstanceMetadata,
  ModProject,
  ResourceKind,
  ResourceSearchQuery,
  ResourceSort,
  ResourceSource,
} from "@/types";

/**
 * 社区资源搜索面板：MOD / 整合包 / 资源包 / 光影共用。
 *
 * 布局约定：工具条与分页条都是 `shrink-0`，只有中间结果区滚动，
 * 所以翻页控件永远贴在面板底部，不会随列表滚走。
 */
const props = defineProps<{ kind: ResourceKind }>();

const PAGE_SIZE_OPTIONS = [20, 40, 50] as const;
/**
 * 预取深度按来源区分：Modrinth 没有共享 Key，可以多取几页；
 * 聚合模式和 CurseForge 受配额限制，只预取 2 / 1 页，避免翻页缓存把 CF 配额放大。
 */
const PREFETCH_AHEAD: Record<ResourceSource, number> = {
  modrinth: 5,
  curseforge: 1,
  aggregate: 2,
};
const PREFETCH_DELAY = 600;

const route = useRoute();
const versionStore = useVersionStore();
const instances = ref<InstanceMetadata[]>([]);
const instanceId = ref("all");
const mcVersion = ref("");
const source = ref<ResourceSource>("aggregate");
const sort = ref<ResourceSort>("downloads");
const loader = ref("");
const keyword = ref("");
const filtersOpen = ref(false);

const items = ref<ModProject[]>([]);
const total = ref(0);
const page = ref(1);
const pageCount = ref(1);
const pageSize = ref<number>(PAGE_SIZE_OPTIONS[0]);
const warnings = ref<string[]>([]);
const loading = ref(false);
const error = ref("");
/** 作品详情弹窗：关闭时先淡出，动画播完再卸载。 */
const detail = useModalState<ModProject>();
const listRef = ref<HTMLElement | null>(null);

const releaseVersions = computed(() =>
  (versionStore.manifest?.versions ?? []).filter((version) => version.type === "release"),
);

const targetInstance = computed(
  () => instances.value.find((instance) => instance.id === instanceId.value) ?? null,
);

const instanceOptions = computed(() => [
  { value: "all", label: "全部实例" },
  ...instances.value.map((instance) => ({
    value: instance.id,
    label: `${instance.displayName}（${instance.gameVersion}）`,
  })),
]);

const mcVersionOptions = computed(() => [
  { value: "", label: "全部正式版" },
  ...releaseVersions.value.map((version) => ({ value: version.id, label: version.id })),
]);

const sourceOptions = [
  { value: "aggregate", label: "双源聚合" },
  { value: "modrinth", label: "Modrinth" },
  { value: "curseforge", label: "CurseForge" },
];

const sortOptions = [
  { value: "downloads", label: "下载量最多" },
  { value: "updated", label: "最近更新" },
];

const loaderOptions = [
  { value: "", label: "全部加载器" },
  { value: "fabric", label: "Fabric" },
  { value: "forge", label: "Forge" },
  { value: "neoforge", label: "NeoForge" },
  { value: "quilt", label: "Quilt" },
];

const pageSizeOptions = computed(() =>
  PAGE_SIZE_OPTIONS.map((size) => ({ value: size, label: `${size} 条 / 页` })),
);

function onFilterChange(): void {
  void reload(1);
}

const totalLabel = computed(() => {
  const amount = total.value.toLocaleString("zh-CN");
  return source.value === "aggregate" ? `约 ${amount} 个结果` : `共 ${amount} 个结果`;
});

const placeholder = computed(() => {
  switch (props.kind) {
    case "mod":
      return "搜索 Mod 名称或描述...";
    case "modpack":
      return "搜索整合包名称或描述...";
    case "resourcepack":
      return "搜索资源包名称或描述...";
    case "datapack":
      return "搜索数据包名称或描述...";
    default:
      return "搜索光影名称或描述...";
  }
});

function sourceLabel(value: ResourceSource): string {
  if (value === "modrinth") return "Modrinth";
  if (value === "curseforge") return "CurseForge";
  return "聚合";
}

/** 下载量：中文习惯用「万 / 亿」，比 M / k 更好读。 */
function formatDownloads(value: number): string {
  if (value >= 100_000_000) return `${(value / 100_000_000).toFixed(1)} 亿`;
  if (value >= 10_000) return `${(value / 10_000).toFixed(1)} 万`;
  return String(value);
}

async function loadInstances(): Promise<void> {
  try {
    instances.value = await listInstances();
  } catch {
    instances.value = [];
  }
}

function buildQuery(targetPage: number): ResourceSearchQuery {
  return {
    query: keyword.value.trim(),
    source: source.value,
    kind: props.kind,
    mcVersion: mcVersion.value,
    loader: props.kind === "mod" ? loader.value : "",
    sort: sort.value,
    page: targetPage,
    pageSize: pageSize.value,
  };
}

/**
 * 翻页 / 筛选的请求代次。
 * 快速切换筛选时，旧请求可能后返回；代次不一致就直接丢弃，避免旧结果覆盖新结果。
 * 预取任务也用它判断是否已被新一轮翻页取消。
 */
let generation = 0;

async function prefetchAhead(fromPage: number, current: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, PREFETCH_DELAY));

  const depth = PREFETCH_AHEAD[source.value] ?? 1;
  for (let step = 1; step <= depth; step += 1) {
    const target = fromPage + step;
    if (target > pageCount.value || current !== generation) return;
    try {
      await searchResources(buildQuery(target));
    } catch {
      return;
    }
  }
}

async function reload(targetPage = 1): Promise<void> {
  const current = ++generation;
  page.value = Math.max(1, targetPage);
  loading.value = true;
  error.value = "";

  try {
    const result = await searchResources(buildQuery(page.value));
    if (current !== generation) return;
    items.value = result.items;
    total.value = result.total;
    pageCount.value = result.pageCount;
    warnings.value = result.warnings ?? [];
    listRef.value?.scrollTo({ top: 0 });

    if (page.value < pageCount.value) {
      void prefetchAhead(page.value, current);
    }
  } catch (err) {
    if (current !== generation) return;
    items.value = [];
    total.value = 0;
    pageCount.value = 1;
    warnings.value = [];
    error.value = `搜索失败：${toMessage(err)}`;
  } finally {
    if (current === generation) {
      loading.value = false;
    }
  }
}

const toast = useToastStore();
const taskStore = useTaskStore();
/** 正在一键下载的资源键（`来源-id`），允许列表里同时下多个。 */
const quickDownloadIds = ref<Set<string>>(new Set());

function quickKey(item: ModProject): string {
  return `${item.source}-${item.id}`;
}

/**
 * 列表里的一键下载：自动挑当前实例可用的最新文件，并自动装必需前置。
 * 适合已经知道自己要什么的用户，不必点进详情页。
 */
async function quickDownload(item: ModProject): Promise<void> {
  const instance = targetInstance.value;
  const key = quickKey(item);
  if (!instance || props.kind !== "mod" || quickDownloadIds.value.has(key)) return;

  quickDownloadIds.value.add(key);
  // 任务 ID 必须带实例 ID：同一个 Mod 同时下载到两个实例时，
  // 后端是两次独立下载（各自 Channel），ID 不带实例会把进度串台
  const taskId = `quick-${instance.id}-${key}`;
  taskStore.start(taskId, `下载 ${item.title}`, true);

  try {
    const loader = instance.loader.type === "vanilla" ? "" : instance.loader.type;
    const running = await getRunningInstances().catch(() => [] as string[]);
    let fileUrl = "";
    let filename = "";
    let sha1: string | null = null;
    const requiredDeps: { id: string; versionId: string | null }[] = [];

    if (item.source === "curseforge") {
      const files = await getCfModVersions(item.id, instance.gameVersion, loader);
      const file = files.find((entry) => entry.downloadUrl);
      if (!file?.downloadUrl) throw new Error("该 Mod 没有适配当前实例的文件");
      fileUrl = file.downloadUrl;
      filename = file.fileName;
      for (const dep of file.dependencies ?? []) {
        if ((dep.relationType ?? 0) === 3) {
          requiredDeps.push({ id: String(dep.modId), versionId: null });
        }
      }
    } else {
      const versions = await getModVersions(item.id, [instance.gameVersion], loader);
      const version = versions.find((entry) => entry.files.length > 0);
      const file = version?.files.find((entry) => entry.primary) ?? version?.files[0];
      if (!version || !file) throw new Error("该 Mod 没有适配当前实例的文件");
      fileUrl = file.url;
      filename = file.filename;
      sha1 = file.hashes.sha1 ?? null;
      for (const dep of version.dependencies ?? []) {
        if (dep.dependencyType === "required") {
          requiredDeps.push({ id: dep.projectId, versionId: dep.versionId ?? null });
        }
      }
    }

    for (const dep of requiredDeps) {
      try {
        await installResourceDependency(instance.id, item.source, dep.id, dep.versionId);
      } catch {
        // 前置失败不阻断本体，详情页里仍可单独安装
      }
    }

    await downloadResource(instance.id, props.kind, fileUrl, filename, sha1, taskId, (progress) => {
      if (progress.kind === "session") {
        taskStore.updateProgress(taskId, progress.summary, null);
      } else {
        taskStore.updateProgress(taskId, null, progress.snapshot);
      }
    });

    taskStore.finish(taskId);
    if (running.includes(instance.id)) {
      toast.warning("已下载到实例", `${item.title} · 游戏正在运行，重启后生效`);
    } else {
      toast.success("下载完成", `${item.title} 已安装到 ${instance.displayName}`);
    }
  } catch (err) {
    if (isApiError(err) && err.type === "cancelled") {
      taskStore.markCancelled(taskId);
      return;
    }
    const message = toMessage(err);
    taskStore.setError(taskId, message);
    toast.error("下载失败", message);
  } finally {
    quickDownloadIds.value.delete(key);
  }
}

function onInstanceChange(): void {
  const instance = targetInstance.value;
  if (instance) {
    mcVersion.value = instance.gameVersion;
    loader.value =
      props.kind === "mod" && instance.loader.type !== "vanilla" ? instance.loader.type : "";
  } else {
    mcVersion.value = "";
    loader.value = "";
  }
  void reload(1);
}

function resetFilters(): void {
  sort.value = "downloads";
  pageSize.value = PAGE_SIZE_OPTIONS[0];
  const instance = targetInstance.value;
  loader.value =
    props.kind === "mod" && instance && instance.loader.type !== "vanilla"
      ? instance.loader.type
      : "";
  void reload(1);
}

onBeforeUnmount(() => {
  // 切换分类后旧组件已经卸载，继续预取只会浪费上游配额。
  generation += 1;
});

onMounted(() => {
  // 从实例健康检查跳转过来时带上了缺失前置的 mod id，直接填进搜索框
  const requestedQuery = route.query.q;
  if (typeof requestedQuery === "string" && requestedQuery.trim()) {
    keyword.value = requestedQuery.trim();
  }

  void versionStore.ensureLoaded().catch(() => undefined);
  void (async () => {
    await loadInstances();
    const requested = route.query.instance;
    if (
      typeof requested === "string" &&
      instances.value.some((instance) => instance.id === requested)
    ) {
      // 从实例资源管理跳转过来时，自动选中该实例。
      instanceId.value = requested;
      onInstanceChange();
    } else {
      void reload(1);
    }
  })();
});
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col gap-3">
    <div class="shrink-0 rounded-lg border border-line/70 bg-surface p-3 shadow-sm">
      <div class="flex flex-wrap items-center gap-2">
        <div v-if="kind !== 'modpack'" class="w-42">
          <AppSelect v-model="instanceId" :options="instanceOptions" @change="onInstanceChange" />
        </div>
        <div class="w-33">
          <AppSelect
            :model-value="mcVersion"
            :options="mcVersionOptions"
            @update:model-value="
              (value) => {
                mcVersion = String(value);
                onFilterChange();
              }
            "
          />
        </div>
        <div class="w-32">
          <AppSelect
            :model-value="source"
            :options="sourceOptions"
            @update:model-value="
              (value) => {
                source = value as ResourceSource;
                onFilterChange();
              }
            "
          />
        </div>
        <div class="min-w-45 flex-1">
          <AppInput v-model="keyword" clearable :placeholder="placeholder" @submit="reload(1)" />
        </div>
        <AppButton variant="primary" :loading="loading" @click="reload(1)">搜索</AppButton>
      </div>

      <div class="mt-2 flex flex-wrap items-center gap-2">
        <AppButton variant="ghost" size="sm" @click="filtersOpen = !filtersOpen">
          <SlidersHorizontal :size="14" />
          筛选
        </AppButton>
        <span class="text-caption text-ink-muted"
          >按{{ sort === "downloads" ? "下载量" : "更新时间" }}排序</span
        >
      </div>

      <div
        v-if="filtersOpen"
        class="mt-2 flex flex-wrap items-center gap-2 border-t border-line/30 pt-2"
      >
        <div class="w-32">
          <AppSelect
            :model-value="sort"
            :options="sortOptions"
            size="sm"
            @update:model-value="
              (value) => {
                sort = value as ResourceSort;
                onFilterChange();
              }
            "
          />
        </div>
        <div v-if="kind === 'mod'" class="w-32">
          <AppSelect
            :model-value="loader"
            :options="loaderOptions"
            size="sm"
            @update:model-value="
              (value) => {
                loader = String(value);
                onFilterChange();
              }
            "
          />
        </div>
        <div class="w-28">
          <AppSelect
            :model-value="pageSize"
            :options="pageSizeOptions"
            size="sm"
            @update:model-value="
              (value) => {
                pageSize = Number(value);
                onFilterChange();
              }
            "
          />
        </div>
        <AppButton variant="ghost" size="sm" @click="resetFilters">重置筛选</AppButton>
      </div>
    </div>

    <p v-if="error" class="shrink-0 text-body text-danger">{{ error }}</p>
    <p
      v-else-if="warnings.length"
      class="flex shrink-0 items-start gap-1.5 text-caption text-warning"
    >
      <AlertTriangle :size="14" class="mt-0.5 shrink-0" />
      <span>{{ warnings.join("；") }}</span>
    </p>
    <div ref="listRef" class="min-h-0 flex-1 overflow-y-auto pr-1">
      <div v-if="loading && items.length === 0" class="flex h-full items-center justify-center">
        <AppLoading :size="64" label="正在加载资源…" />
      </div>
      <div
        v-else-if="items.length === 0 && !error"
        class="flex h-full flex-col items-center justify-center text-ink-muted"
      >
        <Search :size="32" :stroke-width="1.6" class="mb-3 opacity-30" />
        <p class="text-body">没有找到匹配的资源，换个关键词或筛选条件试试</p>
      </div>
      <div v-else class="flex flex-col gap-1.5 pb-2">
        <AppCard v-for="item in items" :key="`${item.source}-${item.id}`" padding="none" hoverable>
          <div class="flex items-center gap-2 px-3 py-2">
            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-3 text-left"
              @click="detail.open(item)"
            >
              <img
                v-if="item.iconUrl"
                :src="item.iconUrl"
                referrerpolicy="no-referrer"
                class="h-10 w-10 shrink-0 rounded-lg bg-ink/5 object-cover"
                loading="lazy"
              />
              <div
                v-else
                class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-ink/5"
              >
                <Package :size="18" class="text-ink-muted" />
              </div>
              <div class="flex min-w-0 flex-1 flex-col">
                <span class="truncate text-body font-semibold text-ink">{{ item.title }}</span>
                <span class="truncate text-micro text-ink-muted">{{ item.description }}</span>
              </div>
            </button>

            <div class="flex shrink-0 items-center gap-2 text-micro">
              <span class="hidden text-ink-muted sm:block">
                {{ formatDownloads(item.downloads) }} 次下载
              </span>
              <!-- 来源只做信息标注：中性描边 + 弱化文字，不与标题抢注意力 -->
              <span class="rounded-sm border border-line px-1.5 py-0.5 text-micro text-ink-muted">
                {{ sourceLabel(item.source) }}
              </span>
              <button
                v-if="kind === 'mod' && targetInstance"
                type="button"
                class="flex h-7 w-7 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-brand-500/10 hover:text-brand-500"
                :title="`一键下载到「${targetInstance.displayName}」，自动安装必需前置`"
                :disabled="quickDownloadIds.has(quickKey(item))"
                @click="quickDownload(item)"
              >
                <Loader2
                  v-if="quickDownloadIds.has(quickKey(item))"
                  :size="14"
                  class="animate-spin text-brand-500"
                />
                <Download v-else :size="14" />
              </button>
              <ChevronRight :size="16" class="text-ink-muted" />
            </div>
          </div>
        </AppCard>
      </div>
    </div>

    <div
      class="flex shrink-0 flex-wrap items-center justify-between gap-2 rounded-lg border border-line/70 bg-surface px-3 py-2 shadow-sm"
    >
      <span class="text-caption text-ink-muted">{{ totalLabel }}</span>
      <AppPagination :page="page" :page-count="pageCount" :disabled="loading" @change="reload" />
    </div>

    <ResourceDetailModal
      v-if="detail.item"
      :item="detail.item"
      :kind="kind"
      :instance="targetInstance"
      :open="detail.visible"
      @close="detail.close"
    />
  </div>
</template>
