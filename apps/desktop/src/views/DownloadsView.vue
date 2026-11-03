<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import type { Component } from "vue";
import {
  Gamepad2,
  Gift,
  Loader2,
  Database,
  Package,
  Palette,
  Puzzle,
  Search,
  Sparkles,
  ChevronRight,
  Check,
  X,
} from "@lucide/vue";
import { useRoute, useRouter } from "vue-router";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import ResourceBrowser from "@/components/downloads/ResourceBrowser.vue";
import { createInstance } from "@/api/instance";
import { installGame } from "@/api/install";
import { installLoader, getLoaderVersions, type LoaderType } from "@/api/loader";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useVersionStore } from "@/stores/version";
import { useTaskStore } from "@/stores/task";
import type { DownloadProgress, VersionSummary, VersionType } from "@/types";

interface Category {
  id: string;
  label: string;
  icon: Component;
}
const categories: Category[] = [
  { id: "vanilla", label: "原版游戏", icon: Gamepad2 },
  { id: "mod", label: "MOD", icon: Puzzle },
  { id: "modpack", label: "整合包", icon: Package },
  { id: "resourcepack", label: "资源包", icon: Palette },
  { id: "datapack", label: "数据包", icon: Database },
  { id: "shader", label: "光影", icon: Sparkles },
];

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
const manifestVersions = computed<VersionSummary[]>(() => versionStore.manifest?.versions ?? []);

const filteredVersions = computed<VersionSummary[]>(() => {
  const all = manifestVersions.value;
  const keyword = searchKeyword.value.trim().toLowerCase();
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

function typeLabel(type: VersionType): string {
  switch (type) {
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
function badgeClass(version: VersionSummary): string {
  if (getAprilFoolsInfo(version.id)) return "bg-warning-soft text-warning";
  switch (version.type) {
    case "release":
      return "bg-success-soft text-success";
    case "snapshot":
      return "bg-warning-soft text-warning";
    default:
      return "bg-ink/5 text-ink-soft";
  }
}
function formatYear(version: VersionSummary): string {
  const raw = version.releaseTime || version.time || "";
  if (!raw) return "未知";
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return raw.slice(0, 4) || "未知";
  return date.getFullYear().toString();
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
});
</script>

<template>
  <div class="mx-auto flex h-full w-full max-w-7xl gap-6 px-10 pt-8 pb-10">
    <!-- 左侧分类导航 -->
    <aside class="sereno-glass flex w-40 shrink-0 flex-col gap-1 self-start rounded-xl p-2">
      <h2 class="mb-1 px-2 text-[12px] font-semibold tracking-widest text-ink-muted">资源分类</h2>
      <button
        v-for="cat in categories"
        :key="cat.id"
        type="button"
        class="flex items-center gap-2 rounded-md px-2.5 py-2 text-left text-[13px] transition-colors"
        :class="
          activeCategory === cat.id
            ? 'bg-brand-500 text-white shadow-sm'
            : 'text-ink-muted hover:bg-ink/4 hover:text-ink-soft'
        "
        @click="activeCategory = cat.id"
      >
        <component :is="cat.icon" :size="15" :stroke-width="2.2" />
        <span :class="activeCategory === cat.id ? 'font-semibold' : 'font-medium'">{{
          cat.label
        }}</span>
      </button>
    </aside>

    <main class="flex min-w-0 flex-1 flex-col gap-4">
      <!-- 社区资源搜索：MOD / 整合包 / 资源包 / 光影 -->
      <ResourceBrowser v-if="activeCategory === 'mod'" kind="mod" />
      <ResourceBrowser v-else-if="activeCategory === 'modpack'" kind="modpack" />
      <ResourceBrowser v-else-if="activeCategory === 'resourcepack'" kind="resourcepack" />
      <ResourceBrowser v-else-if="activeCategory === 'datapack'" kind="datapack" />
      <ResourceBrowser v-else-if="activeCategory === 'shader'" kind="shader" />

      <template v-if="activeCategory === 'vanilla'">
        <div
          class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-line/30 pb-3"
        >
          <div class="flex gap-1 rounded-md bg-ink/5 p-1">
            <button
              v-for="tab in tabs"
              :key="tab.id"
              type="button"
              class="rounded-sm px-3 py-1.5 text-[13px] font-medium transition-colors"
              :class="
                activeTab === tab.id
                  ? 'bg-surface text-ink shadow-sm'
                  : 'text-ink-muted hover:text-ink-soft'
              "
              @click="activeTab = tab.id"
            >
              {{ tab.label }}
            </button>
          </div>
          <div class="w-64">
            <AppInput v-model="searchKeyword" placeholder="搜索版本..." />
          </div>
        </div>

        <div class="min-h-0 flex-1 overflow-y-auto pr-1">
          <div v-if="loading" class="flex h-full items-center justify-center">
            <AppLoading :size="64" label="正在获取版本清单…" />
          </div>
          <div
            v-else-if="errorMessage"
            class="flex h-full flex-col items-center justify-center gap-3 text-ink-muted"
          >
            <Search :size="32" :stroke-width="1.6" class="opacity-30" />
            <p class="max-w-md text-center text-[14px] text-danger">{{ errorMessage }}</p>
            <AppButton variant="secondary" @click="loadVersions(true)">重试</AppButton>
          </div>
          <div
            v-else-if="filteredVersions.length === 0"
            class="flex h-full flex-col items-center justify-center text-ink-muted"
          >
            <Search :size="32" :stroke-width="1.6" class="mb-3 opacity-30" />
            <p class="text-[14px]">没有找到匹配的版本</p>
          </div>
          <div v-else class="flex flex-col gap-2 pb-2">
            <AppCard v-for="version in filteredVersions" :key="version.id" padding="sm" hoverable>
              <div class="flex items-center justify-between gap-4">
                <div class="flex min-w-0 items-center gap-3">
                  <div
                    class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg"
                    :class="badgeClass(version)"
                  >
                    <Gift
                      v-if="getAprilFoolsInfo(version.id)"
                      :size="20"
                      :stroke-width="2.2"
                    /><Package v-else :size="20" :stroke-width="2.2" />
                  </div>
                  <div class="flex min-w-0 flex-col gap-0.5">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="font-mono text-[16px] font-bold text-ink">{{
                        getAprilFoolsInfo(version.id)?.displayId ?? version.id
                      }}</span>
                      <AppBadge
                        v-if="activeTab !== 'april'"
                        :variant="typeVariant(version.type)"
                        size="sm"
                        outline
                        >{{ typeLabel(version.type) }}</AppBadge
                      >
                    </div>
                    <p v-if="activeTab === 'april'" class="text-[12px] text-ink-muted">
                      <span class="font-medium opacity-80">{{ formatYear(version) }}</span
                      ><span class="mx-1.5 opacity-30">|</span
                      >{{ getAprilFoolsInfo(version.id)?.desc }}
                    </p>
                    <p v-else class="text-[12px] text-ink-muted">{{ formatDate(version) }}</p>
                  </div>
                </div>
                <AppButton variant="primary" size="sm" @click="openDownloadModal(version)">
                  下载
                </AppButton>
              </div>
            </AppCard>
          </div>
        </div>
      </template>
    </main>

    <!-- 下载选项 Modal -->
    <AppModal :open="showModal" width="sm" @close="showModal = false">
      <template #header>
        <div class="min-w-0 flex-1">
          <h2 class="text-[15px] font-semibold text-ink">安装选项</h2>
          <p class="text-[11px] text-ink-muted">选择需要的加载器</p>
        </div>
      </template>

      <div class="flex flex-col gap-3">
        <div
          v-for="(item, index) in loadersInfo"
          :key="item.id"
          class="flex flex-col rounded-lg border overflow-hidden transition-colors"
          :class="[
            item.selected ? 'bg-brand-500/5 border-brand-500/30' : 'bg-surface border-line/60',
            getLoaderState(item.id).disabled ? 'opacity-60 bg-surface-muted grayscale' : '',
          ]"
        >
          <div
            class="flex items-center justify-between p-3 px-4 select-none"
            :class="getLoaderState(item.id).disabled ? 'cursor-not-allowed' : 'cursor-pointer'"
            @click="toggleLoader(index)"
          >
            <div class="flex items-center gap-3 w-full">
              <!-- 复选框图标 -->
              <div
                class="flex h-5 w-5 shrink-0 items-center justify-center rounded border transition-colors"
                :class="
                  item.selected
                    ? 'bg-brand-500 border-brand-500 text-white'
                    : 'border-line/80 bg-surface'
                "
              >
                <Check v-if="item.selected" :size="14" stroke-width="3" />
              </div>

              <span
                class="text-[14px] font-medium"
                :class="item.selected ? 'text-ink' : 'text-ink-soft'"
              >
                {{ item.name }}
              </span>

              <!-- 禁用提示 -->
              <span v-if="getLoaderState(item.id).disabled" class="text-[12px] text-ink-muted">
                {{ getLoaderState(item.id).reason }}
              </span>

              <!-- 已选版本的展示 -->
              <span
                v-if="item.selected && item.version && !item.loading"
                class="text-[12px] text-brand-500 font-mono bg-brand-500/10 px-2 py-0.5 rounded flex-1 truncate max-w-[150px]"
              >
                {{ item.version }}
              </span>
              <Loader2
                v-else-if="item.loading"
                :size="14"
                class="shrink-0 animate-spin text-ink-muted"
              />
              <span
                v-else-if="
                  !item.selected && item.versions.length === 0 && !getLoaderState(item.id).disabled
                "
                class="text-[11px]"
                :class="item.failed ? 'text-danger' : 'text-ink-muted'"
              >
                {{ item.failed ? "获取版本失败" : "不支持此版本" }}
              </span>
            </div>

            <div class="flex items-center gap-2">
              <button
                v-if="item.selected"
                @click.stop="toggleLoader(index)"
                class="text-ink-muted hover:text-danger transition-colors p-1"
              >
                <X :size="16" />
              </button>
              <ChevronRight
                v-if="!getLoaderState(item.id).disabled"
                :size="16"
                class="text-ink-muted transition-transform duration-200"
                :class="item.selected ? 'rotate-90' : ''"
              />
            </div>
          </div>

          <!-- 展开的版本选择区 -->
          <div v-if="item.selected && !getLoaderState(item.id).disabled" class="px-4 pb-4 pt-1">
            <div v-if="item.loading" class="flex items-center gap-2 text-[13px] text-ink-muted">
              <Loader2 :size="14" class="animate-spin" /> 正在获取版本...
            </div>
            <div
              v-else-if="item.versions.length === 0"
              class="text-[13px] text-warning bg-warning/10 px-3 py-2 rounded-md"
            >
              该游戏版本暂无此加载器的支持。
            </div>
            <select
              v-else
              v-model="item.version"
              class="w-full h-9 rounded-md border border-line/80 bg-surface px-3 text-[13px] text-ink outline-none focus:border-brand-500"
            >
              <option v-for="v in item.versions" :key="v" :value="v">{{ v }}</option>
            </select>
          </div>
        </div>
      </div>

      <template #footer>
        <div class="flex justify-end gap-3">
          <AppButton variant="secondary" @click="showModal = false">取消</AppButton>
          <AppButton variant="primary" :disabled="loaderLoading" @click="executeDownload">
            确认下载
          </AppButton>
        </div>
      </template>
    </AppModal>
  </div>
</template>
