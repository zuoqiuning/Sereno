<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { AlertTriangle, ChevronRight, Package, Search, SlidersHorizontal } from "@lucide/vue";
import { useRoute } from "vue-router";

import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppPagination from "@/components/common/AppPagination.vue";
import ResourceDetailModal from "@/components/downloads/ResourceDetailModal.vue";
import { listInstances } from "@/api/instance";
import { isApiError } from "@/api/invoke";
import { searchResources } from "@/api/resource";
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
const selectClass =
  "h-9 rounded-md border border-line/80 bg-surface px-2.5 text-[13px] text-ink outline-none transition-colors focus:border-brand-500";

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

function toErrorMessage(error: unknown): string {
  if (isApiError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

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
    error.value = `搜索失败：${toErrorMessage(err)}`;
  } finally {
    if (current === generation) {
      loading.value = false;
    }
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
    <div class="sereno-glass shrink-0 rounded-xl p-3">
      <div class="flex flex-wrap items-center gap-2">
        <select
          v-if="kind !== 'modpack'"
          v-model="instanceId"
          :class="selectClass"
          class="min-w-[150px]"
          @change="onInstanceChange"
        >
          <option value="all">全部实例</option>
          <option v-for="instance in instances" :key="instance.id" :value="instance.id">
            {{ instance.displayName }}（{{ instance.gameVersion }}）
          </option>
        </select>
        <select v-model="mcVersion" :class="selectClass" class="min-w-[130px]" @change="reload(1)">
          <option value="">全部正式版</option>
          <option v-for="version in releaseVersions" :key="version.id" :value="version.id">
            {{ version.id }}
          </option>
        </select>
        <select v-model="source" :class="selectClass" @change="reload(1)">
          <option value="aggregate">双源聚合</option>
          <option value="modrinth">Modrinth</option>
          <option value="curseforge">CurseForge</option>
        </select>
        <div class="min-w-[180px] flex-1">
          <AppInput v-model="keyword" :placeholder="placeholder" @submit="reload(1)" />
        </div>
        <AppButton variant="primary" :loading="loading" @click="reload(1)">搜索</AppButton>
      </div>

      <div class="mt-2 flex flex-wrap items-center gap-2">
        <AppButton variant="ghost" size="sm" @click="filtersOpen = !filtersOpen">
          <SlidersHorizontal :size="14" />
          筛选
        </AppButton>
        <span class="text-[12px] text-ink-muted"
          >按{{ sort === "downloads" ? "下载量" : "更新时间" }}排序</span
        >
      </div>

      <div
        v-if="filtersOpen"
        class="mt-2 flex flex-wrap items-center gap-2 border-t border-line/30 pt-2"
      >
        <select v-model="sort" :class="selectClass" @change="reload(1)">
          <option value="downloads">下载量最多</option>
          <option value="updated">最近更新</option>
        </select>
        <select v-if="kind === 'mod'" v-model="loader" :class="selectClass" @change="reload(1)">
          <option value="">全部加载器</option>
          <option value="fabric">Fabric</option>
          <option value="forge">Forge</option>
          <option value="neoforge">NeoForge</option>
          <option value="quilt">Quilt</option>
        </select>
        <select v-model.number="pageSize" :class="selectClass" @change="reload(1)">
          <option v-for="size in PAGE_SIZE_OPTIONS" :key="size" :value="size">
            {{ size }} 条 / 页
          </option>
        </select>
        <AppButton variant="ghost" size="sm" @click="resetFilters">重置筛选</AppButton>
      </div>
    </div>

    <p v-if="error" class="shrink-0 text-[13px] text-danger">{{ error }}</p>
    <p
      v-else-if="warnings.length"
      class="flex shrink-0 items-start gap-1.5 text-[12px] text-warning"
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
        <p class="text-[14px]">没有找到匹配的资源，换个关键词或筛选条件试试</p>
      </div>
      <div v-else class="flex flex-col gap-1.5 pb-2">
        <AppCard v-for="item in items" :key="`${item.source}-${item.id}`" padding="none" hoverable>
          <button
            type="button"
            class="flex w-full items-center gap-3 px-3 py-2 text-left"
            @click="detail.open(item)"
          >
            <img
              v-if="item.iconUrl"
              :src="item.iconUrl"
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
              <span class="truncate text-[14px] font-semibold text-ink">{{ item.title }}</span>
              <span class="truncate text-[11px] text-ink-muted">{{ item.description }}</span>
            </div>
            <div class="flex shrink-0 items-center gap-2 text-[10px]">
              <span class="hidden text-ink-muted sm:block">
                {{ formatDownloads(item.downloads) }} 次下载
              </span>
              <span
                class="rounded px-1.5 py-0.5"
                :class="
                  item.source === 'modrinth'
                    ? 'bg-success-soft text-success'
                    : 'bg-warning-soft text-warning'
                "
              >
                {{ sourceLabel(item.source) }}
              </span>
              <ChevronRight :size="16" class="text-ink-muted" />
            </div>
          </button>
        </AppCard>
      </div>
    </div>

    <div
      class="sereno-glass flex shrink-0 flex-wrap items-center justify-between gap-2 rounded-xl px-3 py-2"
    >
      <span class="text-[12px] text-ink-muted">{{ totalLabel }}</span>
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
