<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Loader2, RefreshCw, Search, Check, X } from "@lucide/vue";
import {
  Bundle,
  Clock00,
  DiamondSword,
  IronIngot,
  Map,
  ShulkerShell,
} from "minecraft-svg-vue";
import { useRoute, useRouter } from "vue-router";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppEmptyState from "@/components/common/AppEmptyState.vue";
import AppIconButton from "@/components/common/AppIconButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppModal from "@/components/common/AppModal.vue";
import AppSelect from "@/components/common/AppSelect.vue";
import AppSkeleton from "@/components/common/AppSkeleton.vue";
import PageShell from "@/components/layout/PageShell.vue";
import SideNav from "@/components/layout/SideNav.vue";
import type { SideNavItem } from "@/components/layout/SideNav.vue";
import ResourceBrowser from "@/components/downloads/ResourceBrowser.vue";
import { createInstance, listInstances } from "@/api/instance";
import { installGame } from "@/api/install";
import { installLoader, getLoaderVersions, type LoaderType } from "@/api/loader";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useVersionStore } from "@/stores/version";
import { useTaskStore } from "@/stores/task";
import type { DownloadProgress, VersionSummary, VersionType } from "@/types";

// 分类图标用 Minecraft 原版物品的像素图标——比通用线性图标更贴主题。
const categories: SideNavItem[] = [
  { id: "vanilla", label: "原版游戏", icon: IronIngot },
  { id: "mod", label: "MOD", icon: DiamondSword },
  { id: "modpack", label: "整合包", icon: ShulkerShell },
  { id: "resourcepack", label: "资源包", icon: Bundle },
  { id: "datapack", label: "数据包", icon: Map },
  { id: "shader", label: "光影", icon: Clock00 },
];

/** 分类 → 工具条里的一句话提示（不单开一行，宽屏才显示）。 */
const CATEGORY_HINT: Record<string, string> = {
  vanilla: "安装原版游戏，可同时选择加载器",
  mod: "搜索并安装 Mod，支持 Modrinth 与 CurseForge",
  modpack: "一键安装整合包，自动装好游戏与全部 Mod",
  resourcepack: "替换材质与音效",
  datapack: "安装数据包，改变游戏机制",
  shader: "安装光影，需要 Iris 或 OptiFine",
};

type TabId = "release" | "snapshot" | "old" | "april";
const tabs = [
  { id: "release", label: "正式版" },
  { id: "snapshot", label: "快照版" },
  { id: "old", label: "远古版" },
  { id: "april", label: "愚人节版" },
] as const;

const APRIL_FOOLS: Record<string, { displayId: string; desc: string }> = {
  "15w14a": { displayId: "15w14a", desc: "作为一款全年龄向的游戏，我们需要和平，需要爱与拥抱。" },
  "1.rv-pre1": { displayId: "1.RV-Pre1", desc: "是时候将现代科技带入 Minecraft 了！" },
  "3d shareware v1.34": {
    displayId: "3D Shareware v1.34",
    desc: "我们从地下室的废墟里找到了这个开发于 1994 年的杰作！",
  },
  "20w14infinite": {
    displayId: "20w14∞",
    desc: "我们加入了 20 亿个新的维度，让无限的想象变成了现实！",
  },
  "22w13oneblockatatime": {
    displayId: "22w13oneblockatatime",
    desc: "一次一个方块更新！迎接全新的挖掘、合成与骑乘玩法吧！",
  },
  "23w13a_or_b": { displayId: "23w13a_or_b", desc: "研究表明：玩家喜欢做出选择——越多越好！" },
  "24w14potato": {
    displayId: "24w14potato",
    desc: "毒马铃薯一直被大家忽视和低估，于是我们超级加强它了！",
  },
  "25w14craftmine": { displayId: "25w14craftmine", desc: "你可以合成任何东西——包括合成你的世界！" },
  "26w14a": { displayId: "26w14a", desc: "为什么要物品栏？让方块跟着你走吧！" },
};

const router = useRouter();
const route = useRoute();
const versionStore = useVersionStore();
const taskStore = useTaskStore();
const activeCategory = ref("vanilla");
const activeTab = ref<TabId>("release");
const searchKeyword = ref("");
const loading = ref(false);
const errorMessage = ref("");

/** 已安装的 MC 版本集合：用来在版本列表里标出「你已经装过了」。 */
const installedVersions = ref<Set<string>>(new Set());

/**
 * 搜索防抖。
 *
 * 版本清单有 800+ 条，每次按键都全量过滤会让输入明显发涩；
 * 200ms 是「感觉不到延迟」与「不打字也计算」之间的平衡点。
 */
const debouncedKeyword = ref("");
let keywordTimer: number | undefined;
watch(searchKeyword, (value) => {
  window.clearTimeout(keywordTimer);
  keywordTimer = window.setTimeout(() => {
    debouncedKeyword.value = value;
  }, 200);
});
onBeforeUnmount(() => window.clearTimeout(keywordTimer));

/** 当前分类的一句话提示：贴在工具条里，不额外占一行高度。 */
const pageHint = computed(() => CATEGORY_HINT[activeCategory.value] ?? "");

// 分类写回 query：刷新页面 / 从命令面板跳回来时还在同一个分类
watch(activeCategory, (value) => {
  void router.replace({ query: { ...route.query, category: value } });
});

// --- 弹窗相关状态 ---
const showModal = ref(false);
const modalVersion = ref<VersionSummary | null>(null);

// 使用数组保证顺序，方便渲染
interface LoaderOption {
  id: string;
  name: string;
  selected: boolean;
  version: string;
  versions: string[];
  loading: boolean;
  /** 拉取版本列表失败（网络 / 镜像不可达），与“确实不支持此版本”区分开。 */
  failed: boolean;
}

const loadersInfo = ref<LoaderOption[]>([
  {
    id: "forge",
    name: "Forge",
    selected: false,
    version: "",
    versions: [],
    loading: false,
    failed: false,
  },
  {
    id: "neoforge",
    name: "NeoForge",
    selected: false,
    version: "",
    versions: [],
    loading: false,
    failed: false,
  },
  {
    id: "fabric",
    name: "Fabric",
    selected: false,
    version: "",
    versions: [],
    loading: false,
    failed: false,
  },
  {
    id: "quilt",
    name: "Quilt",
    selected: false,
    version: "",
    versions: [],
    loading: false,
    failed: false,
  },
]);

const selectedIds = computed(() => loadersInfo.value.filter((l) => l.selected).map((l) => l.id));
const loaderLoading = computed(() => loadersInfo.value.some((item) => item.loading));
let loaderLoadGeneration = 0;

// 核心互斥逻辑
function getLoaderState(id: string) {
  const isSelected = selectedIds.value.includes(id);
  if (isSelected) return { disabled: false, reason: "" };

  if (selectedIds.value.includes("fabric") && id !== "fabric") {
    return { disabled: true, reason: "与 Fabric 不兼容" };
  }
  if (selectedIds.value.includes("quilt") && id !== "quilt") {
    return { disabled: true, reason: "与 Quilt 不兼容" };
  }
  if (selectedIds.value.includes("neoforge") && id !== "neoforge") {
    return { disabled: true, reason: "与 NeoForge 不兼容" };
  }
  if (
    selectedIds.value.includes("forge") &&
    (id === "neoforge" || id === "fabric" || id === "quilt")
  ) {
    return { disabled: true, reason: "与 Forge 不兼容" };
  }

  return { disabled: false, reason: "" };
}

function getAprilFoolsInfo(id: string) {
  return APRIL_FOOLS[id.toLowerCase()];
}

/** 这个版本是否已经在某个实例里装过。 */
function isInstalled(versionId: string): boolean {
  return installedVersions.value.has(versionId);
}
const manifestVersions = computed<VersionSummary[]>(() => versionStore.manifest?.versions ?? []);

const filteredVersions = computed<VersionSummary[]>(() => {
  const all = manifestVersions.value;
  const keyword = debouncedKeyword.value.trim().toLowerCase();
  let list = all.filter((v) => {
    const isApril = !!getAprilFoolsInfo(v.id);
    if (activeTab.value === "april") return isApril;
    if (isApril) return false;
    if (activeTab.value === "release") return v.type === "release";
    if (activeTab.value === "snapshot") return v.type === "snapshot";
    if (activeTab.value === "old") return v.type === "old_beta" || v.type === "old_alpha";
    return false;
  });
  if (keyword) {
    list = list.filter((v) => {
      const info = getAprilFoolsInfo(v.id);
      const text = info ? info.displayId.toLowerCase() : v.id.toLowerCase();
      return text.includes(keyword);
    });
  }
  return list;
});

function typeLabel(type: VersionType): string {  switch (type) {
    case "release":
      return "正式";
    case "snapshot":
      return "预览";
    case "old_beta":
      return "Beta";
    case "old_alpha":
      return "Alpha";
    default:
      return "未知";
  }
}
function typeVariant(type: VersionType): "success" | "warning" | "default" | "danger" {
  switch (type) {
    case "release":
      return "success";
    case "snapshot":
      return "warning";
    case "old_beta":
    case "old_alpha":
      return "default";
    default:
      return "danger";
  }
}
function formatDate(version: VersionSummary): string {
  const raw = version.releaseTime || version.time || "";
  if (!raw) return "未知日期";
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return raw;
  return date.toLocaleDateString("zh-CN", { year: "numeric", month: "short", day: "numeric" });
}
function toErrorMessage(error: unknown): string {
  if (isApiError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

async function loadVersions(force = false): Promise<void> {
  if (versionStore.isFresh && !force) return;
  loading.value = true;
  errorMessage.value = "";
  try {
    await versionStore.ensureLoaded(force);
  } catch (error) {
    errorMessage.value = `版本清单获取失败：${toErrorMessage(error)}`;
    void log.error(errorMessage.value, {
      component: "DownloadsView",
      action: "loadVersions",
    });
  } finally {
    loading.value = false;
  }
}

/** 打开安装选项时并行拉取三个加载器的版本列表，避免逐个点开才加载。 */
async function loadAllLoaderVersions(versionId: string): Promise<void> {
  const generation = ++loaderLoadGeneration;

  await Promise.all(
    loadersInfo.value.map(async (item) => {
      item.loading = true;
      item.failed = false;
      try {
        const versions = await getLoaderVersions(versionId, item.id as LoaderType);
        if (generation !== loaderLoadGeneration) return;
        item.versions = versions;
        if (versions.length > 0 && !item.version) {
          item.version = versions[0];
        }
      } catch {
        if (generation === loaderLoadGeneration) {
          item.versions = [];
          item.failed = true;
        }
      } finally {
        if (generation === loaderLoadGeneration) {
          item.loading = false;
        }
      }
    }),
  );
}

function openDownloadModal(version: VersionSummary) {
  modalVersion.value = version;
  showModal.value = true;
  for (const item of loadersInfo.value) {
    item.selected = false;
    item.version = "";
    item.versions = [];
    item.loading = false;
    item.failed = false;
  }
  void loadAllLoaderVersions(version.id);
}

function toggleLoader(index: number) {
  const item = loadersInfo.value[index];
  const state = getLoaderState(item.id);

  if (state.disabled) return;

  item.selected = !item.selected;
  if (item.selected && !item.version && item.versions.length > 0) {
    item.version = item.versions[0];
  }
}

async function executeDownload() {
  const version = modalVersion.value;
  if (!version) return;
  showModal.value = false;

  const loadersToInstall = loadersInfo.value.filter((l) => l.selected && l.version);
  const lname = loadersToInstall.map((l) => ` ${l.name}`).join("");

  const info = getAprilFoolsInfo(version.id);
  const displayId = info ? info.displayId : version.id;
  const instanceName = `${displayId}${lname || " 原版"}`;

  let instanceId = "";
  try {
    const meta = await createInstance(version.id, instanceName);
    instanceId = meta.id;
    taskStore.start(instanceId, instanceName);
    router.push("/tasks");
  } catch (error) {
    errorMessage.value = `创建实例失败：${toErrorMessage(error)}`;
    void log.error(errorMessage.value, {
      component: "DownloadsView",
      action: "createInstance",
    });
    return;
  }

  try {
    await installGame(instanceId, (progress: DownloadProgress) => {
      if (progress.kind === "session") {
        taskStore.updateProgress(instanceId, progress.summary, null);
      } else {
        taskStore.updateProgress(instanceId, null, progress.snapshot);
      }
    });

    for (const loader of loadersToInstall) {
      taskStore.setStage3Running(instanceId);
      try {
        await installLoader(instanceId, loader.id as LoaderType, loader.version);
      } catch (error) {
        const message = `${loader.name} 安装失败：${toErrorMessage(error)}`;
        taskStore.setError(instanceId, message);
        void log.error(message, {
          component: "DownloadsView",
          instanceId,
          action: "installLoader",
        });
        return;
      }
    }

    taskStore.finish(instanceId);
  } catch (error) {
    const msg = `下载失败：${toErrorMessage(error)}`;
    taskStore.setError(instanceId, msg);
    void log.error(msg, {
      component: "DownloadsView",
      instanceId,
      action: "download",
    });
  }
}

onMounted(() => {
  const category = route.query.category;
  if (typeof category === "string" && categories.some((entry) => entry.id === category)) {
    activeCategory.value = category;
  }
  void loadVersions();

  // 「已安装」标记：进页面读一次实例列表即可，不需要每次渲染都算
  void listInstances()
    .then((instances) => {
      installedVersions.value = new Set(instances.map((item) => item.gameVersion));
    })
    .catch(() => undefined);
});
</script>

<template>
  <PageShell :chrome="false" width="xl">
    <div class="flex min-h-0 flex-1 gap-6">
      <SideNav v-model="activeCategory" :items="categories" title="下载" />

      <div class="flex min-w-0 flex-1 flex-col gap-4">
        <!-- 社区资源：搜索 + 详情由 ResourceBrowser 自己管理 -->
        <ResourceBrowser v-if="activeCategory === 'mod'" key="mod" kind="mod" />
        <ResourceBrowser v-else-if="activeCategory === 'modpack'" key="modpack" kind="modpack" />
        <ResourceBrowser
          v-else-if="activeCategory === 'resourcepack'"
          key="resourcepack"
          kind="resourcepack"
        />
        <ResourceBrowser
          v-else-if="activeCategory === 'datapack'"
          key="datapack"
          kind="datapack"
        />
        <ResourceBrowser v-else-if="activeCategory === 'shader'" key="shader" kind="shader" />

        <template v-else>
          <!-- 工具条：类型 tab + 分类提示 + 刷新 + 搜索，一行放完，不再单开页头 -->
          <div class="flex shrink-0 items-center justify-between gap-3">
            <div class="flex min-w-0 items-center gap-3">
              <div class="flex shrink-0 gap-1 rounded-md bg-surface-muted p-1">
                <button
                  v-for="tab in tabs"
                  :key="tab.id"
                  type="button"
                  class="rounded-sm px-3 py-1.5 text-caption transition-fast"
                  :class="
                    activeTab === tab.id
                      ? 'bg-surface font-semibold text-ink shadow-sm'
                      : 'font-medium text-ink-muted hover:text-ink-soft'
                  "
                  @click="activeTab = tab.id"
                >
                  {{ tab.label }}
                </button>
              </div>
              <p class="hidden truncate text-micro text-ink-muted lg:block">{{ pageHint }}</p>
            </div>

            <div class="flex shrink-0 items-center gap-2">
              <AppIconButton
                size="sm"
                label="刷新版本清单"
                :disabled="loading"
                @click="loadVersions(true)"
              >
                <RefreshCw :size="14" :class="{ 'animate-spin': loading }" />
              </AppIconButton>
              <div class="w-56">
                <AppInput v-model="searchKeyword" clearable placeholder="搜索版本号…">
                  <template #prefix>
                    <Search :size="14" />
                  </template>
                </AppInput>
              </div>
            </div>
          </div>

          <!-- 版本列表：用「行 + 分隔线」而不是一版一卡，800 条才不会变成一堵墙 -->
          <Transition name="tab" mode="out-in">
            <div
              :key="activeTab"
              class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface"
            >
              <div v-if="loading" class="flex flex-col divide-y divide-line/50">
                <div v-for="i in 8" :key="i" class="flex items-center gap-3 px-4 py-3">
                  <AppSkeleton width="h-9 w-9" rounded="rounded-md" />
                  <div class="flex flex-1 flex-col gap-2">
                    <AppSkeleton width="w-24" height="h-4" />
                    <AppSkeleton width="w-40" height="h-3" />
                  </div>
                  <AppSkeleton width="w-16" height="h-7" rounded="rounded-md" />
                </div>
              </div>

              <div v-else-if="errorMessage" class="flex flex-1 items-center justify-center">
                <AppEmptyState
                  :icon="Search"
                  title="版本清单获取失败"
                  :description="errorMessage"
                  action-label="重试"
                  @action="loadVersions(true)"
                />
              </div>

              <div v-else-if="filteredVersions.length === 0" class="flex flex-1 items-center justify-center">
                <AppEmptyState
                  :icon="Search"
                  title="没有找到匹配的版本"
                  description="换个关键词，或切到其它类型看看。"
                  action-label="清空搜索"
                  @action="searchKeyword = ''"
                />
              </div>

              <div v-else class="min-h-0 flex-1 overflow-y-auto">
                <div class="flex flex-col divide-y divide-line/50">
                  <div
                    v-for="version in filteredVersions"
                    :key="version.id"
                    class="group flex items-center gap-3 px-4 py-3 transition-fast hover:bg-surface-muted"
                  >
                    <!-- 行首不再放图标块：版本号本身就是这一行的主体 -->
                    <span class="w-35 shrink-0 truncate font-mono text-h2 font-semibold text-ink tabular">
                      {{ getAprilFoolsInfo(version.id)?.displayId ?? version.id }}
                    </span>

                    <div class="flex min-w-0 flex-1 items-center gap-2">
                      <AppBadge
                        v-if="activeTab !== 'april'"
                        :variant="typeVariant(version.type)"
                        size="sm"
                        outline
                      >
                        {{ typeLabel(version.type) }}
                      </AppBadge>
                      <AppBadge v-if="isInstalled(version.id)" variant="brand" size="sm">
                        已安装
                      </AppBadge>
                      <span
                        v-if="activeTab === 'april'"
                        class="truncate text-micro text-ink-muted"
                        :title="getAprilFoolsInfo(version.id)?.desc"
                      >
                        {{ getAprilFoolsInfo(version.id)?.desc }}
                      </span>
                    </div>

                    <span class="w-20 shrink-0 text-right text-micro text-ink-muted tabular">
                      {{ formatDate(version) }}
                    </span>

                    <AppButton
                      :variant="isInstalled(version.id) ? 'ghost' : 'link'"
                      size="sm"
                      class="w-19 shrink-0"
                      @click="openDownloadModal(version)"
                    >
                      {{ isInstalled(version.id) ? "再装一个" : "下载" }}
                    </AppButton>
                  </div>
                </div>
              </div>
            </div>
          </Transition>
        </template>
      </div>
    </div>

    <!-- 下载选项 Modal -->
    <AppModal :open="showModal" width="sm" aria-label="安装选项" @close="showModal = false">
      <template #header>
        <div class="min-w-0 flex-1">
          <h2 class="text-h2 font-semibold text-ink">安装选项</h2>
          <p class="text-micro text-ink-muted">
            {{ modalVersion ? getAprilFoolsInfo(modalVersion.id)?.displayId ?? modalVersion.id : "" }}
            · 选择需要一并安装的加载器
          </p>
        </div>
      </template>

      <div class="flex flex-col gap-2">
        <div
          v-for="(item, index) in loadersInfo"
          :key="item.id"
          class="overflow-hidden rounded-md border transition-fast"
          :class="[
            item.selected ? 'border-brand-500/40 bg-brand-500/5' : 'border-line/70 bg-surface',
            getLoaderState(item.id).disabled ? 'opacity-60' : '',
          ]"
        >
          <div
            class="flex select-none items-center justify-between px-3 py-2.5"
            :class="getLoaderState(item.id).disabled ? 'cursor-not-allowed' : 'cursor-pointer'"
            @click="toggleLoader(index)"
          >
            <div class="flex w-full items-center gap-3">
              <div
                class="flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-xs border transition-fast"
                :class="
                  item.selected
                    ? 'border-brand-500 bg-brand-500 text-white'
                    : 'border-line bg-surface'
                "
                style="height: 18px; width: 18px"
              >
                <Check v-if="item.selected" :size="12" stroke-width="3" />
              </div>

              <span class="text-body font-medium" :class="item.selected ? 'text-ink' : 'text-ink-soft'">
                {{ item.name }}
              </span>

              <span v-if="getLoaderState(item.id).disabled" class="text-micro text-ink-muted">
                {{ getLoaderState(item.id).reason }}
              </span>

              <span
                v-if="item.selected && item.version && !item.loading"
                class="max-w-[150px] flex-1 truncate rounded-xs bg-brand-500/10 px-2 py-0.5 font-mono text-micro text-brand-500"
              >
                {{ item.version }}
              </span>
              <Loader2 v-else-if="item.loading" :size="14" class="shrink-0 animate-spin text-ink-muted" />
              <span
                v-else-if="
                  !item.selected && item.versions.length === 0 && !getLoaderState(item.id).disabled
                "
                class="text-micro"
                :class="item.failed ? 'text-danger' : 'text-ink-muted'"
              >
                {{ item.failed ? "获取版本失败" : "不支持此版本" }}
              </span>
            </div>

            <AppIconButton
              v-if="item.selected"
              size="sm"
              variant="danger"
              label="取消选择"
              @click.stop="toggleLoader(index)"
            >
              <X :size="14" />
            </AppIconButton>
          </div>

          <div v-if="item.selected && !getLoaderState(item.id).disabled" class="px-3 pb-3">
            <div v-if="item.loading" class="flex items-center gap-2 text-caption text-ink-muted">
              <Loader2 :size="14" class="animate-spin" /> 正在获取版本…
            </div>
            <div
              v-else-if="item.versions.length === 0"
              class="rounded-sm bg-warning-soft px-3 py-2 text-caption text-warning"
            >
              该游戏版本暂无此加载器的支持。
            </div>
            <AppSelect
              v-else
              v-model="item.version"
              :options="item.versions.map((version) => ({ value: version, label: version }))"
            />
          </div>
        </div>
      </div>

      <template #footer>
        <div class="flex justify-end gap-2">
          <AppButton variant="secondary" @click="showModal = false">取消</AppButton>
          <AppButton variant="primary" :disabled="loaderLoading" @click="executeDownload">
            确认下载
          </AppButton>
        </div>
      </template>
    </AppModal>
  </PageShell>
</template>
