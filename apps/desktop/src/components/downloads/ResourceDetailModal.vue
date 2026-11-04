<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { AlertTriangle, ChevronDown, ExternalLink, Link2, Loader2, Package } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import AppSelect from "@/components/common/AppSelect.vue";
import ModpackInstallModal from "@/components/downloads/ModpackInstallModal.vue";
import { isApiError } from "@/api/invoke";
import {
  downloadResource,
  getCfModVersions,
  getCfModVersionsBatch,
  getModVersions,
  getResourceDependencies,
  installResourceDependency,
  openResourcePage,
} from "@/api/resource";
import { useVersionStore } from "@/stores/version";
import { useTaskStore } from "@/stores/task";
import { useToastStore } from "@/stores/toast";
import { log } from "@/api/settings";
import { useModalState } from "@/composables/useModalState";
import { gameVersionGroup } from "@/utils/gameVersion";
import { listLocalResources } from "@/api/local";
import { getRunningInstances } from "@/api/instance";
import type {
  CfFile,
  InstanceMetadata,
  LocalResourceKind,
  ModProject,
  ModVersion,
  ResourceDependencyInfo,
  ResourceKind,
} from "@/types";

/** 作品详情页：版本按「加载器 + 具体游戏版本」分组，前缀只用于顶部筛选。 */
const props = withDefaults(
  defineProps<{
    item: ModProject;
    kind: ResourceKind;
    instance: InstanceMetadata | null;
    /** 是否显示；关闭时先淡出，动画播完再由父组件卸载。 */
    open?: boolean;
  }>(),
  { open: true },
);

const emit = defineEmits<{
  (e: "close"): void;
}>();

/** 必需前置的引用，名称在展开分组时批量解析。 */
interface DependencyRef {
  id: string;
  versionId: string | null;
  /** 可选前置：不装也能启动，但作者推荐安装。 */
  optional: boolean;
}

interface DetailVersion {
  key: string;
  title: string;
  fileName: string;
  fileUrl: string | null;
  sha1: string | null;
  downloads: number;
  updatedAt: string;
  channelKey: "release" | "beta" | "alpha" | "";
  loaderKey: string;
  loaderKeys: string[];
  loaderLabels: string[];
  /** 该文件实际支持的游戏版本（例如 1.21.1）。 */
  gameVersion: string;
  /** 该文件支持的全部游戏版本（一个文件常常同时支持 1.21 与 1.21.1）。 */
  gameVersions: string[];
  /** 上游平台里的版本 ID（Modrinth version id），整合包安装需要它。 */
  sourceVersionId: string | null;
  /** 该文件声明的必需前置与不兼容项。 */
  dependencies: DependencyRef[];
  conflicts: DependencyRef[];
}

interface VersionGroup {
  key: string;
  /** 分组标题里的具体游戏版本，例如 1.21.1 / 26.1。 */
  gameVersion: string;
  loaderKey: string;
  title: string;
  versions: DetailVersion[];
}

interface RawVersion {
  /** 去重键：Modrinth 用 version id，CurseForge 用 file id。 */
  key: string;
  baseName: string;
  fileName: string;
  fileUrl: string | null;
  sha1: string | null;
  downloads: number;
  updatedAt: string;
  channelKey: "release" | "beta" | "alpha" | "";
  loaderKeys: string[];
  loaderLabels: string[];
  gameVersions: string[];
  sourceVersionId: string | null;
  dependencies: DependencyRef[];
  conflicts: DependencyRef[];
}

const LOADER_LABELS: Record<string, string> = {
  neoforge: "NeoForge",
  forge: "Forge",
  fabric: "Fabric",
  quilt: "Quilt",
  liteloader: "LiteLoader",
  optifine: "OptiFine",
  rift: "Rift",
  cauldron: "Cauldron",
};
const LOADER_ORDER = [
  "neoforge",
  "forge",
  "fabric",
  "quilt",
  "liteloader",
  "optifine",
  "rift",
  "cauldron",
];

const versionStore = useVersionStore();
const loading = ref(true);
const error = ref("");
const groups = ref<VersionGroup[]>([]);
/** 原始版本列表：可增量合并（CurseForge 按版本补全时会追加）。 */
const rawVersions = ref<RawVersion[]>([]);
const gamePrefixes = ref<string[]>([]);
const activePrefix = ref("");
const activeLoader = ref("");
const expandedKeys = ref<Set<string>>(new Set());
const downloadingKey = ref<string | null>(null);
const notice = ref("");
const noticeTone = ref<"success" | "error">("success");
/** 整合包安装弹窗：关闭时先淡出，动画播完再卸载。 */
const modpackModal = useModalState<{ versionId: string; title: string }>();
/** 依赖 ID → 项目信息（名称、图标、介绍、下载量），展开分组时批量解析并缓存。 */
const dependencyInfos = ref<Record<string, ResourceDependencyInfo>>({});
/** 已经拿到文件的具体游戏版本，避免重复查询。 */
const coveredVersions = ref<Set<string>>(new Set());
/** 正在按具体版本补全 CurseForge 的文件列表。 */
const completing = ref(false);
/** 补全任务进行中；复用同一个 Promise，避免同一批版本被查两遍。 */
let pendingCompletion: Promise<void> | null = null;
/** 目标实例里已经存在的文件名（用于标记「已安装」）。 */
const installedFileNames = ref<Set<string>>(new Set());

const sourceLabel = computed(() => (props.item.source === "modrinth" ? "Modrinth" : "CurseForge"));

const usesLoader = computed(() => props.kind === "mod" || props.kind === "modpack");

const loaderOptions = computed(() => {
  const keys = [...new Set(groups.value.map((group) => group.loaderKey).filter(Boolean))];
  return [
    { value: "", label: "全部加载器" },
    ...keys
      .sort((a, b) => loaderRank(a) - loaderRank(b))
      .map((key) => ({ value: key, label: loaderLabelOf(key) })),
  ];
});

const filteredGroups = computed(() =>
  groups.value.filter(
    (group) =>
      (!activePrefix.value || gameVersionGroup(group.gameVersion) === activePrefix.value) &&
      (!activeLoader.value || group.loaderKey === activeLoader.value),
  ),
);

const visibleFileCount = computed(() =>
  filteredGroups.value.reduce((total, group) => total + group.versions.length, 0),
);

const downloadTarget = computed(() =>
  props.instance ? `下载到「${props.instance.displayName}」` : "下载到启动器的下载目录",
);

/** 顶部固定栏里的支持摘要，例如 `Fabric / NeoForge · 1.16+`。 */
const supportSummary = computed(() => {
  const loaders = [...new Set(groups.value.map((group) => group.loaderKey).filter(Boolean))]
    .map(loaderLabelOf)
    .join(" / ");
  const numericVersions = gamePrefixes.value.filter((version) => /\d/.test(version));
  const oldest = numericVersions[numericVersions.length - 1] ?? "";
  return [loaders, oldest ? `${oldest}+` : ""].filter(Boolean).join(" · ");
});

const downloadHint = computed(() => {
  // 整合包会自己新建实例，不需要目标实例
  if (props.kind === "modpack") return "";
  if (!props.instance) return "未选择实例时，文件会下载到启动器的下载目录";
  if (props.kind === "mod" && props.instance.loader.type === "vanilla") {
    return "目标实例是原版，请先安装加载器";
  }
  return "";
});

/** 只把官方正式版作为游戏版本分组，快照/预览版不混进正式版标签。 */
const officialVersions = computed(() => {
  const versions = versionStore.manifest?.versions ?? [];
  return new Set(
    versions.filter((version) => version.type === "release").map((version) => version.id),
  );
});

function toErrorMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

function normalizeLoader(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function loaderLabelOf(key: string): string {
  if (!key) return "";
  return LOADER_LABELS[key] ?? key.charAt(0).toUpperCase() + key.slice(1);
}

function loaderRank(key: string): number {
  const preferred = props.instance?.loader.type;
  const order =
    preferred && LOADER_ORDER.includes(preferred)
      ? [preferred, ...LOADER_ORDER.filter((loader) => loader !== preferred)]
      : LOADER_ORDER;
  const index = order.indexOf(key);
  return index === -1 ? 99 : index;
}

function versionTokens(value: string): number[] {
  const match = value.match(/\d+/g);
  return match ? match.map(Number) : [];
}

/** 1.21.1 > 1.21 > 1.20.4；非数字版本排在后面。 */
function compareGameVersion(a: string, b: string): number {
  const left = versionTokens(a);
  const right = versionTokens(b);
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const l = left[index] ?? 0;
    const r = right[index] ?? 0;
    if (l !== r) return l - r;
  }
  return a.localeCompare(b, "zh-CN");
}

function sortGameVersions(values: string[]): string[] {
  return [...new Set(values)].sort((a, b) => {
    const aNumeric = /\d/.test(a);
    const bNumeric = /\d/.test(b);
    if (aNumeric && bNumeric) return compareGameVersion(b, a);
    if (aNumeric) return -1;
    if (bNumeric) return 1;
    return a.localeCompare(b, "zh-CN");
  });
}

type ChannelKey = "release" | "beta" | "alpha" | "";

function channelKeyFromModrinth(type: string | null | undefined): ChannelKey {
  switch (type) {
    case "release":
      return "release";
    case "beta":
      return "beta";
    case "alpha":
      return "alpha";
    default:
      return "";
  }
}

function channelKeyFromRelease(type: number | undefined): ChannelKey {
  switch (type) {
    case 1:
      return "release";
    case 2:
      return "beta";
    case 3:
      return "alpha";
    default:
      return "";
  }
}

function channelLetter(key: ChannelKey): string {
  switch (key) {
    case "release":
      return "R";
    case "beta":
      return "B";
    case "alpha":
      return "A";
    default:
      return "";
  }
}

function channelTone(key: ChannelKey): string {
  switch (key) {
    case "release":
      return "bg-brand-500/12 text-brand-500";
    case "beta":
      return "bg-warning-soft text-warning";
    case "alpha":
      return "bg-danger-soft text-danger";
    default:
      return "bg-ink/5 text-ink-muted";
  }
}

function formatDate(value: string): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value.slice(0, 10);
  return date.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
}

/** 与 PCL 一致的相对时间：刚刚 / 分钟 / 小时 / 天 / 月 / 年。 */
function formatRelative(value: string): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value.slice(0, 10);

  const diffMs = Date.now() - date.getTime();
  if (diffMs < 0) return formatDate(value);

  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 60) return minutes < 1 ? "刚刚" : `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} 个月前`;
  return `${Math.floor(days / 365)} 年前`;
}

function formatDownloads(value: number): string {
  if (value >= 100_000_000) return `${(value / 100_000_000).toFixed(1)} 亿`;
  if (value >= 10_000) return `${(value / 10_000).toFixed(1)} 万`;
  return String(value);
}

/** 行内第二行：下载量与更新时间，版本号已经写在分组标题里。 */
function rowMeta(version: DetailVersion): string {
  return [
    version.downloads > 0 ? `${formatDownloads(version.downloads)} 次下载` : "",
    version.updatedAt ? `更新于 ${formatRelative(version.updatedAt)}` : "",
  ]
    .filter(Boolean)
    .join(" · ");
}

function splitCfTags(tags: string[]): { gameVersions: string[]; loaders: string[] } {
  const gameVersions: string[] = [];
  const loaders: string[] = [];

  for (const tag of tags) {
    const key = normalizeLoader(tag);
    if (usesLoader.value && LOADER_LABELS[key]) {
      loaders.push(key);
      continue;
    }
    if (/\d/.test(tag)) {
      gameVersions.push(tag);
    }
  }

  return { gameVersions, loaders };
}

function rawFromModrinth(version: ModVersion): RawVersion {
  const file = version.files.find((entry) => entry.primary) ?? version.files[0];
  const loaders = usesLoader.value
    ? [...new Set((version.loaders ?? []).map(normalizeLoader))].filter(
        (key) => LOADER_LABELS[key] || key.length > 0,
      )
    : [];
  const rawName = version.name?.trim() ?? "";
  const baseName =
    rawName && rawName.toLowerCase().includes(props.item.title.toLowerCase())
      ? rawName
      : `${props.item.title} ${version.versionNumber}`;

  return {
    key: version.id,
    baseName,
    fileName: file?.filename ?? "",
    fileUrl: file?.url ?? null,
    sha1: file?.hashes.sha1 ?? null,
    downloads: version.downloads ?? 0,
    updatedAt: version.datePublished ?? "",
    channelKey: channelKeyFromModrinth(version.versionType),
    loaderKeys: loaders,
    loaderLabels: loaders.map(loaderLabelOf),
    gameVersions: version.gameVersions ?? [],
    sourceVersionId: version.id,
    dependencies: (version.dependencies ?? [])
      .filter(
        (dependency) =>
          dependency.dependencyType === "required" || dependency.dependencyType === "optional",
      )
      .map((dependency) => ({
        id: dependency.projectId,
        versionId: dependency.versionId ?? null,
        optional: dependency.dependencyType === "optional",
      })),
    conflicts: (version.dependencies ?? [])
      .filter((dependency) => dependency.dependencyType === "incompatible")
      .map((dependency) => ({
        id: dependency.projectId,
        versionId: dependency.versionId ?? null,
        optional: false,
      })),
  };
}

function rawFromCf(file: CfFile): RawVersion {
  const { gameVersions, loaders } = splitCfTags(file.gameVersions ?? []);
  const loaderKeys = [...new Set(loaders)];
  const displayName = file.displayName?.trim() || file.fileName?.trim() || props.item.title;
  const baseName = displayName.toLowerCase().includes(props.item.title.toLowerCase())
    ? displayName
    : `${props.item.title} ${displayName}`;

  return {
    key: `cf-${file.id}`,
    baseName,
    fileName: file.fileName,
    fileUrl: file.downloadUrl ?? null,
    sha1: null,
    downloads: file.downloadCount ?? 0,
    updatedAt: file.fileDate ?? "",
    channelKey: channelKeyFromRelease(file.releaseType),
    loaderKeys,
    loaderLabels: loaderKeys.map(loaderLabelOf),
    gameVersions,
    // CurseForge 的整合包以「文件」形式分发：每个文件就是一份整合包 zip，
    // 文件 ID 就是后端解析 `manifest.json` 需要的 `fileId`。
    // Modrinth 用的是版本 ID，两者语义不同，因此这里带上来源前缀由上层区分。
    sourceVersionId:
      props.kind === "modpack" ? `cf:${file.id}` : null,
    dependencies: (file.dependencies ?? [])
      .filter((dependency) => (dependency.relationType ?? 0) === 3 || (dependency.relationType ?? 0) === 2)
      .map((dependency) => ({
        id: String(dependency.modId),
        versionId: null,
        optional: (dependency.relationType ?? 0) === 2,
      })),
    conflicts: (file.dependencies ?? [])
      .filter((dependency) => (dependency.relationType ?? 0) === 5)
      .map((dependency) => ({ id: String(dependency.modId), versionId: null, optional: false })),
  };
}

/** 版本名已经包含游戏版本时就不再重复追加 "for ..."。 */
function buildRowTitle(baseName: string, groupTitle: string, gameVersion: string): string {
  if (!groupTitle) return baseName;
  const lower = baseName.toLowerCase();
  const hasVersion = gameVersion ? lower.includes(gameVersion.toLowerCase()) : true;
  if (hasVersion) return baseName;
  return `${baseName} for ${groupTitle}`;
}

function buildGroups(rawVersions: RawVersion[], releaseVersions: Set<string>): VersionGroup[] {
  const map = new Map<string, VersionGroup>();

  for (const raw of rawVersions) {
    const allVersions = raw.gameVersions.length > 0 ? raw.gameVersions : ["未知版本"];
    const releaseOnly =
      releaseVersions.size > 0
        ? allVersions.filter((version) => releaseVersions.has(version))
        : allVersions;
    const versions = sortGameVersions(releaseOnly.length > 0 ? releaseOnly : allVersions);

    // 一个文件可以同时支持多个加载器（例如 Forge + NeoForge）。
    // 每个加载器都要有自己的分组，否则按加载器筛选时会漏掉文件。
    const groupLoaders = raw.loaderKeys.length > 0 ? raw.loaderKeys : [""];
    for (const loaderKey of groupLoaders) {
      for (const gameVersion of versions) {
        const groupKey = `${loaderKey}|${gameVersion}`;
        let group = map.get(groupKey);
        if (!group) {
          const title = [loaderLabelOf(loaderKey), gameVersion].filter(Boolean).join(" ");
          group = { key: groupKey, gameVersion, loaderKey, title, versions: [] };
          map.set(groupKey, group);
        }

        if (group.versions.some((entry) => entry.fileName === raw.fileName)) {
          continue;
        }

        group.versions.push({
          key: `${raw.fileName}|${raw.baseName}|${loaderKey}|${gameVersion}`,
          title: buildRowTitle(raw.baseName, group.title, gameVersion),
          fileName: raw.fileName,
          fileUrl: raw.fileUrl,
          sha1: raw.sha1,
          downloads: raw.downloads,
          updatedAt: raw.updatedAt,
          channelKey: raw.channelKey,
          loaderKey,
          loaderKeys: raw.loaderKeys,
          loaderLabels: raw.loaderLabels,
          gameVersion,
          gameVersions: raw.gameVersions,
          sourceVersionId: raw.sourceVersionId,
          dependencies: raw.dependencies,
          conflicts: raw.conflicts,
        });
      }
    }
  }

  const result = [...map.values()];
  for (const group of result) {
    group.versions.sort((a, b) => {
      const byGameVersion = compareGameVersion(b.gameVersion, a.gameVersion);
      if (byGameVersion !== 0) return byGameVersion;
      const byDate = b.updatedAt.localeCompare(a.updatedAt);
      return byDate !== 0 ? byDate : b.downloads - a.downloads;
    });
  }
  result.sort((a, b) => {
    const byVersion = compareGameVersion(b.gameVersion, a.gameVersion);
    return byVersion !== 0 ? byVersion : loaderRank(a.loaderKey) - loaderRank(b.loaderKey);
  });
  return result;
}

function toggleGroup(group: VersionGroup): void {
  const next = new Set(expandedKeys.value);
  if (next.has(group.key)) {
    next.delete(group.key);
  } else {
    next.add(group.key);
    void loadDependencies(group);
  }
  expandedKeys.value = next;
}

/** 重新按当前原始版本列表分组。 */
function rebuildGroups(): void {
  const built = buildGroups(rawVersions.value, officialVersions.value);
  groups.value = built;
  gamePrefixes.value = sortGameVersions(built.map((group) => gameVersionGroup(group.gameVersion)));
}

/** 该前缀下、官方正式版清单里存在的具体版本。 */
function concreteVersionsOf(prefix: string): string[] {
  const releases = versionStore.manifest?.versions ?? [];
  return releases
    .filter((version) => version.type === "release" && gameVersionGroup(version.id) === prefix)
    .map((version) => version.id);
}

/**
 * 首屏要查询的游戏版本集合。
 *
 * Modrinth 的版本接口一次只能可靠地过滤一个游戏版本（多值会退化成全量），
 * 所以这里给出「实例那一版 + 最近几个正式版」，由后端并发查询后合并。
 */
function initialScope(): string[] {
  const releases = (versionStore.manifest?.versions ?? [])
    .filter((version) => version.type === "release")
    .map((version) => version.id);

  const versions = new Set<string>();
  const instanceVersion = props.instance?.gameVersion;
  if (instanceVersion) versions.add(instanceVersion);

  // 没有实例时多取两个最近正式版，首屏不至于只有一个分支。
  for (const release of releases.slice(0, instanceVersion ? 1 : 3)) {
    versions.add(release);
  }
  return [...versions];
}

/**
 * 目标实例的加载器。
 *
 * Modrinth 按加载器过滤能把响应缩小一个量级（JEI 的 1.21.1：
 * 959 KB → 318 KB），所以有实例时一定带上。
 */
const instanceLoader = computed(() => {
  const loader = props.instance?.loader.type ?? "";
  return loader && loader !== "vanilla" ? loader : "";
});

/**
 * 补全某个版本前缀下的其它分支。
 *
 * 两个站点的补法不一样：
 * - Modrinth 的接口支持多值 `game_versions`，**一次请求**就能覆盖整个前缀；
 * - CurseForge 只认单个版本，只能逐个查询，但这些查询可以并发发起
 *   （后端统一限速），单个版本的响应只有几 KB，很划算。
 */
function completePrefix(prefix: string): Promise<void> {
  if (!prefix) return Promise.resolve();
  // 已有补全任务在跑时复用它，避免同一批版本被查两遍。
  if (pendingCompletion) return pendingCompletion;

  const isCurseForge = props.item.source === "curseforge";
  const missing = concreteVersionsOf(prefix).filter(
    (version) => !coveredVersions.value.has(version),
  );
  if (missing.length === 0) return Promise.resolve();

  completing.value = true;
  pendingCompletion = (async () => {
    try {
      if (isCurseForge) {
        await completeCurseForgePrefix(missing);
      } else {
        await completeModrinthPrefix(missing);
      }
      rebuildGroups();
      await expandInstanceGroup();
    } catch (err) {
      // 补全失败不影响已经拿到的版本列表
      void log.error(`补全 ${prefix} 版本列表失败: ${toErrorMessage(err)}`, {
        component: "ResourceDetailModal",
        action: "completePrefix",
      });
    } finally {
      completing.value = false;
      pendingCompletion = null;
    }
  })();

  return pendingCompletion;
}

/** Modrinth：逐个版本并发查询后合并。 */
async function completeModrinthPrefix(versions: string[]): Promise<void> {
  for (const version of versions) {
    coveredVersions.value.add(version);
  }
  const loaded = await getModVersions(props.item.id, versions, instanceLoader.value);
  mergeRaw(loaded.map(rawFromModrinth));
}

/** CurseForge：先单独取实例那一版，其余分支一次性并发补全。 */
async function completeCurseForgePrefix(versions: string[]): Promise<void> {
  const instanceVersion = props.instance?.gameVersion;
  const rest = versions.filter((version) => version !== instanceVersion);

  // 目标实例的那一版先单独查，命中后立刻展开对应分组
  if (instanceVersion && versions.includes(instanceVersion)) {
    coveredVersions.value.add(instanceVersion);
    const files = await getCfModVersions(props.item.id, instanceVersion, "");
    mergeRaw(files.map(rawFromCf));
    rebuildGroups();
    await expandInstanceGroup();
  }

  if (rest.length > 0) {
    for (const version of rest) {
      coveredVersions.value.add(version);
    }
    const files = await getCfModVersionsBatch(props.item.id, rest, "");
    mergeRaw(files.map(rawFromCf));
  }
}

/** 把新拿到的版本并进原始列表（按去重键去重）。 */
function mergeRaw(extra: RawVersion[]): void {
  if (extra.length === 0) return;
  const known = new Set(rawVersions.value.map((raw) => raw.key));
  const fresh = extra.filter((raw) => !known.has(raw.key));
  if (fresh.length === 0) return;
  rawVersions.value = [...rawVersions.value, ...fresh];
}

/**
 * 定位并展开目标实例对应的版本组。
 *
 * 实例是 1.21.1 + NeoForge，就直接展开「NeoForge 1.21.1」这一组，
 * 不用让用户自己在几十个分组里翻。
 */
async function expandInstanceGroup(): Promise<void> {
  const instance = props.instance;
  if (!instance) return;

  const loaderKey = normalizeLoader(instance.loader.type);
  const gameVersion = instance.gameVersion;
  const exact = groups.value.find(
    (group) => group.loaderKey === loaderKey && group.gameVersion === gameVersion,
  );
  if (!exact) return;

  const next = new Set(expandedKeys.value);
  next.add(exact.key);
  expandedKeys.value = next;
  await loadDependencies(exact);

  // 展开后把这一组滚到视野内
  requestAnimationFrame(() => {
    document
      .querySelector(`[data-group-key="${CSS.escape(exact.key)}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" });
  });
}

/** 展开分组时一次性解析该组所有版本的前置与冲突信息。 */
async function loadDependencies(group: VersionGroup): Promise<void> {
  const ids = [
    ...new Set(
      group.versions.flatMap((version) =>
        [...version.dependencies, ...version.conflicts].map((dep) => dep.id),
      ),
    ),
  ].filter((id) => id !== props.item.id && !dependencyInfos.value[id]);
  if (ids.length === 0) return;

  try {
    const resolved = await getResourceDependencies(
      props.item.source === "curseforge" ? "curseforge" : "modrinth",
      ids,
    );
    const next = { ...dependencyInfos.value };
    for (const info of resolved) {
      next[info.id] = info;
    }
    dependencyInfos.value = next;
  } catch (error) {
    // 解析失败时保留占位，不阻断版本列表与下载，但要把原因记下来。
    void log.error(`解析前置信息失败: ${toErrorMessage(error)}`, {
      component: "ResourceDetailModal",
      action: "loadDependencies",
    });
  }
}

/** 把一组依赖引用转成可展示、可下载的条目；解析失败时用 mod id 兜底。 */
function dependencyViews(refs: DependencyRef[]): DependencyView[] {
  const ids = [...new Set(refs.map((dep) => dep.id))].filter((id) => id !== props.item.id);

  return ids.map((id) => {
    const info = dependencyInfos.value[id];
    const own = refs.filter((entry) => entry.id === id);
    const ref = own.find((entry) => !entry.optional) ?? own[0];
    return {
      id,
      name: info?.name ?? id,
      iconUrl: info?.iconUrl ?? null,
      description: info?.description ?? null,
      downloads: info?.downloads ?? 0,
      updatedAt: info?.updatedAt ?? null,
      source: info?.source ?? (props.item.source === "curseforge" ? "curseforge" : "modrinth"),
      versionId: ref?.versionId ?? null,
      optional: own.every((entry) => entry.optional),
    };
  });
}

/** 该分组涉及的前置项目；解析失败时也保留条目，用 mod id 兜底。 */
function groupDependencies(group: VersionGroup): DependencyView[] {
  return dependencyViews(group.versions.flatMap((version) => version.dependencies));
}

/** 前置条目：项目信息可能没解析出来，但下载入口始终保留。 */
interface DependencyView {
  id: string;
  name: string;
  iconUrl: string | null;
  description: string | null;
  downloads: number;
  updatedAt: string | null;
  source: "modrinth" | "curseforge" | "aggregate";
  versionId: string | null;
  optional: boolean;
}

const downloadingDep = ref<string | null>(null);

/** 单独安装一个前置；后端会按实例的游戏版本与加载器挑文件。 */
async function downloadDependency(dep: DependencyView): Promise<void> {
  const instance = props.instance;
  if (!instance || downloadingDep.value) return;

  downloadingDep.value = dep.id;
  notice.value = "";
  try {
    const source = dep.source === "curseforge" ? "curseforge" : "modrinth";
    const name = await installResourceDependency(instance.id, source, dep.id, dep.versionId);
    noticeTone.value = "success";
    notice.value = `已安装前置「${name || dep.name}」`;
  } catch (err) {
    noticeTone.value = "error";
    notice.value = `前置「${dep.name}」安装失败：${toErrorMessage(err)}`;
  } finally {
    downloadingDep.value = null;
  }
}

/** 未解析出名称时退化为数量，避免出现空白。 */
function refsText(refs: DependencyRef[]): string {
  const ids = [...new Set(refs.map((dep) => dep.id))].filter((id) => id !== props.item.id);
  if (ids.length === 0) return "";

  const names = ids.map((id) => dependencyInfos.value[id]?.name);
  if (names.every((name) => name && name !== "未知前置")) {
    return names.join("、");
  }
  return `${ids.length} 项`;
}

/** 行内「前置：Fabric API」；名称没解析出来时退化为数量。 */
function dependencyText(version: DetailVersion): string {
  return refsText(version.dependencies);
}

/** 行内「不兼容：OptiFine」；同样只在解析成功后显示名称。 */
function conflictText(version: DetailVersion): string {
  return refsText(version.conflicts);
}

/** 行内前置数量，例如「1 项前置」。 */
function dependencyCount(version: DetailVersion): number {
  return new Set(version.dependencies.map((dep) => dep.id).filter((id) => id !== props.item.id))
    .size;
}

function downloadBlockReason(version: DetailVersion): string | null {
  if (!version.fileUrl) return "该文件被作者设置为禁止第三方下载";
  // 没有目标实例时文件会落到启动器的下载目录，因此不做版本匹配校验。
  if (!props.instance) return null;

  if (props.kind === "mod") {
    if (props.instance.loader.type === "vanilla") {
      return "目标实例是原版，请先安装加载器";
    }
    const instanceLoader = normalizeLoader(props.instance.loader.type);
    if (version.loaderKeys.length > 0 && !version.loaderKeys.includes(instanceLoader)) {
      return `该版本适用于 ${version.loaderLabels.join(" / ")}，与实例加载器不匹配`;
    }
    if (
      version.gameVersions.length > 0 &&
      !version.gameVersions.includes(props.instance.gameVersion)
    ) {
      return `该版本适用于 ${version.gameVersions.join(" / ")}，与实例 ${props.instance.gameVersion} 不匹配`;
    }
  }

  return null;
}

/** 前置选择弹窗：一键包含前置，或手动勾选。 */
interface DependencyDialogState {
  version: DetailVersion;
  deps: DependencyView[];
  manual: boolean;
  selected: string[];
}

const dependencyDialog = ref<DependencyDialogState | null>(null);
const dependencyDialogOpen = ref(false);
const taskStore = useTaskStore();
const toast = useToastStore();

/** 点「下载」：没有前置/没有实例时直接下；有前置时先让用户选择。 */
function requestDownload(version: DetailVersion): void {
  if (!version.fileUrl) return;

  const deps = dependencyViews(version.dependencies);
  if (!props.instance || deps.length === 0 || props.kind !== "mod") {
    void startDownload(version, []);
    return;
  }

  dependencyDialog.value = {
    version,
    deps,
    manual: false,
    selected: deps.filter((dep) => !dep.optional).map((dep) => dep.id),
  };
  dependencyDialogOpen.value = true;
}

function closeDependencyDialog(): void {
  dependencyDialogOpen.value = false;
}

/** 一键下载：所有前置（含可选）都装上。 */
function downloadWithAllDependencies(): void {
  const dialog = dependencyDialog.value;
  if (!dialog) return;
  closeDependencyDialog();
  void startDownload(
    dialog.version,
    dialog.deps.map((dep) => dep.id),
  );
}

/** 手动选择：只装勾选的前置。 */
function downloadWithSelectedDependencies(): void {
  const dialog = dependencyDialog.value;
  if (!dialog) return;
  closeDependencyDialog();
  void startDownload(dialog.version, dialog.selected);
}

function downloadWithoutDependencies(): void {
  const dialog = dependencyDialog.value;
  if (!dialog) return;
  closeDependencyDialog();
  void startDownload(dialog.version, []);
}

function toggleDependency(id: string): void {
  const dialog = dependencyDialog.value;
  if (!dialog) return;
  dialog.selected = dialog.selected.includes(id)
    ? dialog.selected.filter((entry) => entry !== id)
    : [...dialog.selected, id];
}

async function startDownload(version: DetailVersion, dependencyIds: string[]): Promise<void> {
  if (!version.fileUrl || downloadingKey.value) return;

  const reason = downloadBlockReason(version);
  // 版本 / 加载器不匹配时不往实例里塞错文件，但允许下载到下载目录，
  // 由用户自己决定什么时候装。
  const instance = reason ? null : props.instance;
  downloadingKey.value = version.key;
  notice.value = "";

  const taskId = `resource-${props.item.source}-${props.item.id}-${version.key}`;
  taskStore.start(taskId, `下载 ${version.title}`, true);

  const instanceRunning = instance
    ? (await getRunningInstances().catch(() => [] as string[])).includes(instance.id)
    : false;

  const deps = instance
    ? dependencyIds
        .map((id) => dependencyViews(version.dependencies).find((dep) => dep.id === id))
        .filter((dep): dep is DependencyView => Boolean(dep))
    : [];
  const installed: string[] = [];
  const failed: string[] = [];

  try {
    // 先装选中的前置，再装本体；前置失败不阻断本体。
    for (const dep of deps) {
      try {
        await installResourceDependency(instance!.id, props.item.source, dep.id, dep.versionId);
        installed.push(dep.name);
      } catch {
        failed.push(dep.name);
      }
    }

    await downloadResource(
      instance?.id ?? null,
      props.kind,
      version.fileUrl,
      version.fileName,
      version.sha1,
      taskId,
      (progress) => {
        if (progress.kind === "session") {
          taskStore.updateProgress(taskId, progress.summary, null);
        } else {
          taskStore.updateProgress(taskId, null, progress.snapshot);
        }
      },
    );

    taskStore.finish(taskId);
    const target = instance ? `实例 ${instance.displayName}` : "启动器下载目录";
    const base = `已下载「${version.title}」到${target}`;

    if (!instance && reason) {
      noticeTone.value = "success";
      notice.value = `${base}（${reason}）`;
      toast.success("下载完成", `${version.title} 已保存到下载目录`);
    } else if (failed.length > 0) {
      noticeTone.value = "error";
      notice.value = `${base}，但前置 ${failed.join("、")} 安装失败`;
      toast.warning("前置未全部安装", failed.join("、"));
    } else {
      noticeTone.value = "success";
      notice.value = installed.length ? `${base}，并安装前置 ${installed.join("、")}` : base;
      if (instanceRunning) {
        toast.warning("已下载到实例", `${version.title} · 游戏正在运行，重启后生效`);
      } else {
        toast.success(
          "下载完成",
          installed.length ? `已安装前置：${installed.join("、")}` : version.title,
        );
      }
      if (instanceRunning) notice.value += "；游戏正在运行，重启后生效";
    }
  } catch (err) {
    if (isApiError(err) && err.type === "cancelled") {
      taskStore.markCancelled(taskId);
      return;
    }
    const message = toErrorMessage(err);
    taskStore.setError(taskId, message);
    noticeTone.value = "error";
    notice.value = `下载失败：${message}`;
    toast.error("下载失败", message);
  } finally {
    downloadingKey.value = null;
  }
}

/** 整合包：打开安装确认弹窗。 */
function openModpack(version: DetailVersion): void {
  if (!version.sourceVersionId) return;
  modpackModal.open({ versionId: version.sourceVersionId, title: version.title });
}

async function handleOpenPage(): Promise<void> {
  try {
    await openResourcePage(props.item.source, props.item.slug, props.kind);
  } catch (err) {
    noticeTone.value = "error";
    notice.value = `打开项目页失败：${toErrorMessage(err)}`;
  }
}

async function loadVersions(): Promise<void> {
  loading.value = true;
  error.value = "";
  notice.value = "";

  try {
    await versionStore.ensureLoaded().catch(() => undefined);

    rawVersions.value =
      props.item.source === "modrinth"
        ? (
            await getModVersions(props.item.id, initialScope(), instanceLoader.value)
          ).map(rawFromModrinth)
        : (await getCfModVersions(props.item.id, "", "")).map(rawFromCf);

    // 记录已经拿到的具体版本，避免补全时重复查询。
    coveredVersions.value = new Set(rawVersions.value.flatMap((raw) => raw.gameVersions));

    rebuildGroups();
    expandedKeys.value = new Set();
    activeLoader.value = "";

    // 从实例进入时聚焦到该实例的主次版本（1.21.1 → 1.21）。
    const preferred = props.instance ? gameVersionGroup(props.instance.gameVersion) : "";
    activePrefix.value = groups.value.some(
      (group) => gameVersionGroup(group.gameVersion) === preferred,
    )
      ? preferred
      : "";

    // CurseForge 需要按具体版本补全，随后自动展开实例对应的那一组。
    await completePrefix(activePrefix.value);
    await expandInstanceGroup();
  } catch (err) {
    error.value = `版本列表获取失败：${toErrorMessage(err)}`;
  } finally {
    loading.value = false;
  }
}

async function loadInstalledFiles(): Promise<void> {
  if (!props.instance || props.kind === "modpack") return;
  try {
    const list = await listLocalResources(props.instance.id, props.kind as LocalResourceKind);
    installedFileNames.value = new Set(
      list.map((item) => item.fileName.replace(/\.disabled$/i, "")),
    );
  } catch {
    // 读取失败不影响版本列表，只是不显示「已安装」。
  }
}

onMounted(() => {
  void loadVersions();
  void loadInstalledFiles();
});

// 切换版本前缀时补全该前缀下的其它具体分支（CurseForge 需要逐版本查询）。
watch(activePrefix, (prefix) => {
  void completePrefix(prefix);
});
</script>

<template>
  <AppModal width="lg" height="fixed" :padded="false" :open="open" @close="emit('close')">
    <template #header>
      <img
        v-if="item.iconUrl"
        :src="item.iconUrl"
        referrerpolicy="no-referrer"
        class="h-10 w-10 shrink-0 rounded-lg bg-ink/5 object-cover"
      />
      <div v-else class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-ink/5">
        <Package :size="20" class="text-ink-muted" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-h2 font-semibold leading-5 text-ink">{{ item.title }}</h2>
        <p class="truncate text-micro text-ink-muted">
          <span v-if="supportSummary">{{ supportSummary }} · </span>
          {{ sourceLabel }} · {{ formatDownloads(item.downloads) }} 次下载
          <span v-if="item.updatedAt"> · 更新于 {{ formatRelative(item.updatedAt) }}</span>
        </p>
      </div>
      <AppButton variant="secondary" size="sm" @click="handleOpenPage">
        <ExternalLink :size="13" />
        项目页
      </AppButton>
    </template>

    <template #toolbar>
      <div class="flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          class="h-7 rounded-md px-2.5 text-micro font-medium transition-colors"
          :class="
            activePrefix === ''
              ? 'bg-brand-500 text-white'
              : 'bg-ink/5 text-ink-soft hover:bg-ink/10'
          "
          @click="activePrefix = ''"
        >
          全部
        </button>
        <button
          v-for="prefix in gamePrefixes"
          :key="prefix"
          type="button"
          class="h-7 rounded-md px-2.5 font-mono text-micro font-medium transition-colors"
          :class="
            activePrefix === prefix
              ? 'bg-brand-500 text-white'
              : 'bg-ink/5 text-ink-soft hover:bg-ink/10'
          "
          @click="activePrefix = prefix"
        >
          {{ prefix }}
        </button>

        <div class="ml-auto flex items-center gap-2">
          <div v-if="loaderOptions.length > 2" class="w-31">
            <AppSelect v-model="activeLoader" :options="loaderOptions" size="sm" />
          </div>
          <span v-if="completing" class="flex items-center gap-1 text-micro text-ink-muted">
            <Loader2 :size="12" class="animate-spin" />
            正在补全其它分支
          </span>
          <span class="text-micro text-ink-muted">
            {{ filteredGroups.length }} 组 · {{ visibleFileCount }} 条
          </span>
          <span
            v-if="kind !== 'modpack'"
            class="rounded-md px-2 py-1 text-micro"
            :class="instance ? 'bg-ink/5 text-ink-muted' : 'bg-warning-soft text-warning'"
          >
            目标：{{ instance?.displayName ?? "未选择实例" }}
          </span>
        </div>
      </div>
    </template>

    <div class="px-5 py-3">
      <p class="line-clamp-2 text-caption leading-5 text-ink-soft">{{ item.description }}</p>

      <p
        v-if="notice"
        class="mt-2 text-caption"
        :class="noticeTone === 'error' ? 'text-danger' : 'text-success'"
      >
        {{ notice }}
      </p>
      <p v-else-if="downloadHint" class="mt-2 text-caption text-ink-muted">{{ downloadHint }}</p>

      <div v-if="loading" class="flex h-64 items-center justify-center">
        <AppLoading :size="64" label="正在获取版本列表…" />
      </div>
      <div v-else-if="error" class="flex h-64 flex-col items-center justify-center gap-3">
        <p class="text-body text-danger">{{ error }}</p>
        <AppButton variant="secondary" size="sm" @click="loadVersions">重试</AppButton>
      </div>
      <div
        v-else-if="filteredGroups.length === 0"
        class="flex h-48 items-center justify-center text-body text-ink-muted"
      >
        没有可用版本
      </div>
      <div v-else class="mt-3 flex flex-col gap-2 pb-1">
        <section
          v-for="group in filteredGroups"
          :key="group.key"
          :data-group-key="group.key"
          class="overflow-hidden rounded-lg border border-line/50 bg-surface-muted/25"
        >
          <button
            type="button"
            class="flex w-full items-center justify-between gap-3 px-4 py-2.5 text-left transition-colors hover:bg-ink/3"
            @click="toggleGroup(group)"
          >
            <span class="text-body font-semibold text-ink">{{ group.title }}</span>
            <span class="flex items-center gap-2 text-micro text-ink-muted">
              {{ group.versions.length }} 个版本
              <ChevronDown
                :size="15"
                class="transition-transform"
                :class="expandedKeys.has(group.key) ? 'rotate-180' : ''"
              />
            </span>
          </button>

          <div v-if="expandedKeys.has(group.key)" class="border-t border-line/40">
            <!-- 前置资源：与 PCL 一致，展开后先看到需要一起装的前置 MOD -->
            <div
              v-if="groupDependencies(group).length"
              class="border-b border-line/30 bg-ink/2 px-4 py-2.5"
            >
              <div class="mb-1.5 text-micro font-medium text-ink-muted">前置资源</div>
              <div class="flex flex-col gap-2">
                <div
                  v-for="dep in groupDependencies(group)"
                  :key="dep.id"
                  class="flex items-center gap-2.5"
                >
                  <img
                    v-if="dep.iconUrl"
                    referrerpolicy="no-referrer"
                    :src="dep.iconUrl"
                    class="h-7 w-7 shrink-0 rounded-md bg-ink/5 object-cover"
                    loading="lazy"
                  />
                  <div
                    v-else
                    class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-ink/5"
                  >
                    <Package :size="14" class="text-ink-muted" />
                  </div>
                  <div class="min-w-0 flex-1">
                    <div class="truncate text-caption font-medium text-ink">
                      {{ dep.name }}
                      <span
                        v-if="dep.optional"
                        class="ml-1 rounded bg-ink/5 px-1 py-0.5 text-micro font-normal text-ink-muted"
                      >
                        可选
                      </span>
                    </div>
                    <div v-if="dep.description" class="truncate text-micro text-ink-muted">
                      {{ dep.description }}
                    </div>
                  </div>
                  <div class="shrink-0 text-right text-micro text-ink-muted">
                    <span v-if="dep.downloads">{{ formatDownloads(dep.downloads) }} 次下载 · </span>
                    <span v-if="dep.updatedAt">{{ formatRelative(dep.updatedAt) }} · </span>
                    {{ dep.source === "modrinth" ? "Modrinth" : "CurseForge" }}
                  </div>
                  <AppButton
                    v-if="instance && kind === 'mod'"
                    variant="secondary"
                    size="sm"
                    :loading="downloadingDep === dep.id"
                    :disabled="downloadingDep !== null && downloadingDep !== dep.id"
                    @click="downloadDependency(dep)"
                  >
                    安装
                  </AppButton>
                  <span v-else-if="kind === 'mod'" class="shrink-0 text-micro text-ink-muted">
                    选实例后安装
                  </span>
                </div>
              </div>
            </div>

            <div class="divide-y divide-line/30">
              <div
                v-for="version in group.versions"
                :key="version.key"
                class="flex items-center gap-3 px-4 py-2"
              >
                <span
                  class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-micro font-semibold"
                  :class="channelTone(version.channelKey)"
                >
                  {{
                    channelLetter(version.channelKey) ||
                    loaderLabelOf(version.loaderKey).charAt(0) ||
                    item.title.slice(0, 1)
                  }}
                </span>
                <div class="min-w-0 flex-1">
                  <div class="flex min-w-0 items-center gap-1.5">
                    <span class="truncate text-body font-medium leading-5 text-ink">
                      {{ version.title }}
                    </span>
                    <span
                      v-if="installedFileNames.has(version.fileName)"
                      class="shrink-0 rounded bg-success-soft px-1.5 py-0.5 text-micro text-success"
                    >
                      已安装
                    </span>
                  </div>
                  <div v-if="rowMeta(version)" class="truncate text-micro text-ink-muted">
                    {{ rowMeta(version) }}
                  </div>
                  <div
                    v-if="dependencyCount(version)"
                    class="flex items-center gap-1 truncate text-micro text-ink-muted"
                    :title="dependencyText(version)"
                  >
                    <Link2 :size="11" class="shrink-0" />
                    {{ dependencyCount(version) }} 项前置{{
                      dependencyText(version) ? `：${dependencyText(version)}` : ""
                    }}
                  </div>
                  <div
                    v-if="conflictText(version)"
                    class="flex items-center gap-1 truncate text-micro text-danger"
                    :title="conflictText(version)"
                  >
                    <AlertTriangle :size="11" class="shrink-0" />
                    不兼容：{{ conflictText(version) }}
                  </div>
                </div>
                <AppButton
                  v-if="kind === 'modpack'"
                  variant="primary"
                  size="sm"
                  :disabled="!version.sourceVersionId"
                  title="创建新实例并安装整合包"
                  @click="openModpack(version)"
                >
                  安装
                </AppButton>
                <AppButton
                  v-else-if="!version.fileUrl"
                  variant="secondary"
                  size="sm"
                  title="作者禁止第三方下载，打开项目页手动下载"
                  @click="handleOpenPage"
                >
                  打开页面
                </AppButton>
                <AppButton
                  v-else
                  variant="secondary"
                  size="sm"
                  :loading="downloadingKey === version.key"
                  :title="downloadBlockReason(version) ?? downloadTarget"
                  @click="requestDownload(version)"
                >
                  下载
                </AppButton>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  </AppModal>

  <!-- 前置选择：一键包含前置，或手动勾选 -->
  <AppModal :open="dependencyDialogOpen" width="sm" @close="closeDependencyDialog">
    <template #header>
      <div class="min-w-0 flex-1">
        <h3 class="text-h2 font-semibold text-ink">前置资源</h3>
        <p class="truncate text-micro text-ink-muted">
          {{ dependencyDialog?.version.title }}
        </p>
      </div>
    </template>

    <div v-if="dependencyDialog" class="flex flex-col gap-4">
      <template v-if="!dependencyDialog.manual">
        <p class="text-caption leading-5 text-ink-muted">
          检测到 {{ dependencyDialog.deps.length }} 个前置，其中
          {{ dependencyDialog.deps.filter((dep) => dep.optional).length }} 个为可选。
        </p>
        <div class="flex flex-col gap-2">
          <AppButton variant="primary" @click="downloadWithAllDependencies">
            一键下载（含前置）
          </AppButton>
          <AppButton variant="secondary" @click="dependencyDialog.manual = true">
            手动选择前置
          </AppButton>
          <AppButton variant="ghost" @click="downloadWithoutDependencies">
            仅下载本体
          </AppButton>
        </div>
      </template>

      <template v-else>
        <div class="flex max-h-64 flex-col gap-1 overflow-y-auto pr-1">
          <label
            v-for="dep in dependencyDialog.deps"
            :key="dep.id"
            class="flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 transition-colors hover:bg-ink/4"
          >
            <input
              type="checkbox"
              class="accent-brand-500"
              :checked="dependencyDialog.selected.includes(dep.id)"
              @change="toggleDependency(dep.id)"
            />
            <span class="min-w-0 flex-1 truncate text-caption text-ink">{{ dep.name }}</span>
            <span
              v-if="dep.optional"
              class="shrink-0 rounded bg-ink/5 px-1 py-0.5 text-micro text-ink-muted"
            >
              可选
            </span>
          </label>
        </div>

        <div class="flex items-center justify-between gap-2">
          <AppButton variant="secondary" @click="dependencyDialog.manual = false">返回</AppButton>
          <AppButton variant="primary" @click="downloadWithSelectedDependencies">
            下载选中（{{ dependencyDialog.selected.length }}）
          </AppButton>
        </div>
      </template>
    </div>
  </AppModal>

  <ModpackInstallModal
    v-if="modpackModal.item"
    :item="item"
    :version-id="modpackModal.item.versionId"
    :version-title="modpackModal.item.title"
    :open="modpackModal.visible"
    @close="modpackModal.close"
  />
</template>
