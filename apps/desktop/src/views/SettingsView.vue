<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import type { Component } from "vue";
import {
  Check,
  Coffee,
  Download,
  FolderOpen,
  Gamepad2,
  HardDrive,
  Heart,
  HeartHandshake,
  Info,
  RefreshCw,
  SlidersHorizontal,
} from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppBadge from "@/components/common/AppBadge.vue";
import AppInput from "@/components/common/AppInput.vue";
import { commands } from "@/bindings";
import { APP_AUTHOR, APP_NAME, APP_VERSION } from "@/constants/app";
import { callCommand } from "@/api/invoke";
import { installJavaRuntime } from "@/api/java";
import { clearStorageCache, getStorageUsage } from "@/api/storage";
import { formatBytes } from "@/utils/format";
import { log } from "@/api/settings";
import { openProjectRepo, openSupporterPage, type SupporterTarget } from "@/api/supporters";
import { applyTheme } from "@/utils/theme";
import type { AppSettings_Serialize, JavaCandidate, StorageUsage } from "@/types";

type SectionId = "general" | "launch" | "credits" | "about";

const sections: { id: SectionId; label: string; icon: Component }[] = [
  { id: "general", label: "通用", icon: SlidersHorizontal },
  { id: "launch", label: "启动", icon: Gamepad2 },
  { id: "credits", label: "鸣谢", icon: HeartHandshake },
  { id: "about", label: "关于", icon: Info },
];

const activeSection = ref<SectionId>("general");
const loading = ref(false);
const javas = ref<JavaCandidate[]>([]);
const settings = ref<AppSettings_Serialize | null>(null);
const settingsError = ref("");
const downloadingJava = ref<number | null>(null);
const javaError = ref("");
const storage = ref<StorageUsage | null>(null);
const clearingCache = ref(false);

/** 磁盘占用：实例、缓存、日志。 */
async function loadStorage(): Promise<void> {
  try {
    storage.value = await getStorageUsage();
  } catch (e) {
    void log.error(`统计磁盘占用失败: ${e}`, { component: "SettingsView" });
  }
}

async function clearCache(): Promise<void> {
  if (clearingCache.value) return;
  clearingCache.value = true;
  try {
    await clearStorageCache();
    await loadStorage();
    void log.info("缓存已清理", { component: "SettingsView" });
  } catch (e) {
    void log.error(`清理缓存失败: ${e}`, { component: "SettingsView" });
  } finally {
    clearingCache.value = false;
  }
}

/** 缺失的常见 Java 主版本，界面据此给出自动下载按钮。 */
const missingJavaMajors = computed(() =>
  [8, 17, 21].filter((major) => !javas.value.some((java) => java.majorVersion === major)),
);

/** 鸣谢名单：按重要性排序，不再分栏。 */
interface Acknowledgement {
  name: string;
  desc: string;
  supporter: SupporterTarget | null;
}

const acknowledgements: Acknowledgement[] = [
  {
    name: "BMCLAPI",
    desc: "bangbang93 维护的 Minecraft 资源镜像，为游戏本体、加载器与 Maven 依赖提供国内加速。",
    supporter: "bmclapi",
  },
  {
    name: "PCL2（龙腾猫跃）",
    desc: "加载器离线安装、镜像优先与实例资源管理的思路参考了其设计。",
    supporter: "pcl",
  },
];

/** 打开白名单赞助页；失败只记日志，不打断界面。 */
function sponsorFor(target: SupporterTarget | null): void {
  if (!target) return;
  void openSupporterPage(target).catch((error) => {
    void log.error(`打开赞助页失败: ${String(error)}`, { component: "SettingsView" });
  });
}

/** 关于页：直达项目仓库。 */
function openRepo(): void {
  void openProjectRepo().catch((error) => {
    void log.error(`打开仓库失败: ${String(error)}`, { component: "SettingsView" });
  });
}

/** 限速滑杆：界面用 MB/s，配置存 KB/s；0 表示不限。 */
const limitMbps = computed({
  get: () => (settings.value?.download.speedLimitKbps ?? 0) / 1024,
  set: (value: number) => {
    if (settings.value) {
      settings.value.download.speedLimitKbps = value > 0 ? Math.round(value * 1024) : null;
    }
  },
});

/** 滑杆绑定：`defaultMemoryMb` 可为 null，界面上统一显示 2048。 */
const memoryMb = computed({
  get: () => settings.value?.java.defaultMemoryMb ?? 2048,
  set: (value: number) => {
    if (settings.value) settings.value.java.defaultMemoryMb = value;
  },
});

async function loadSettings(): Promise<void> {
  try {
    settings.value = await callCommand(() => commands.getSettings());
    applyTheme(settings.value.theme);
    settingsError.value = "";
  } catch (e) {
    settingsError.value = "设置读取失败，请稍后重试。";
    void log.error(`读取设置失败: ${e}`, { component: "SettingsView" });
  }
}

async function saveSettings(): Promise<void> {
  if (!settings.value) return;
  try {
    await callCommand(() => commands.saveSettings(settings.value!));
    void log.info("设置已保存", { component: "SettingsView" });
  } catch (e) {
    void log.error(`保存设置失败: ${e}`, { component: "SettingsView" });
  }
}

async function selectDir(): Promise<void> {
  if (!settings.value) return;
  try {
    const path = await callCommand(() => commands.selectDirectory());
    if (path) {
      settings.value.instancesRoot = path;
      await saveSettings();
    }
  } catch (e) {
    void log.error(`选择目录失败: ${e}`, { component: "SettingsView" });
  }
}

async function scanJava(): Promise<void> {
  loading.value = true;
  try {
    javas.value = await callCommand(() => commands.getJavaCandidates());
    void log.info(`扫描到 ${javas.value.length} 个 Java 环境`, { component: "SettingsView" });
  } catch (e) {
    void log.error(`Java 扫描失败: ${e}`, { component: "SettingsView" });
  } finally {
    loading.value = false;
  }
}

async function installJava(major: number): Promise<void> {
  downloadingJava.value = major;
  javaError.value = "";
  try {
    await installJavaRuntime(major);
    await scanJava();
    void log.info(`已安装 Java ${major}`, { component: "SettingsView" });
  } catch (e) {
    javaError.value = `下载 Java ${major} 失败：${String(e)}`;
    void log.error(javaError.value, { component: "SettingsView" });
  } finally {
    downloadingJava.value = null;
  }
}

/** 切换深浅主题：立即生效并持久化。 */
function setTheme(event: Event): void {
  if (!settings.value) return;
  const dark = (event.target as HTMLInputElement).checked;
  settings.value.theme = dark ? "dark" : "light";
  applyTheme(settings.value.theme);
  void saveSettings();
}

/** 选中某个 Java 作为默认启动运行时。 */
async function useJava(candidate: JavaCandidate): Promise<void> {
  if (!settings.value) return;
  settings.value.java.customPath = candidate.path;
  await saveSettings();
}

onMounted(() => {
  void loadSettings();
  void scanJava();
  void loadStorage();
});
</script>

<template>
  <div class="mx-auto flex h-full w-full max-w-7xl gap-6 px-10 pt-8 pb-10">
    <!-- 左侧小导航 -->
    <aside class="sereno-glass flex w-40 shrink-0 flex-col gap-1 self-start rounded-xl p-2">
      <h1 class="mb-1 px-2 text-[12px] font-semibold tracking-widest text-ink-muted">设置</h1>
      <button
        v-for="section in sections"
        :key="section.id"
        type="button"
        class="flex items-center gap-2 rounded-md px-2.5 py-2 text-left text-[13px] transition-colors"
        :class="
          activeSection === section.id
            ? 'bg-brand-500 text-white shadow-sm'
            : 'text-ink-muted hover:bg-ink/4 hover:text-ink-soft'
        "
        @click="activeSection = section.id"
      >
        <component :is="section.icon" :size="15" />
        <span :class="activeSection === section.id ? 'font-semibold' : 'font-medium'">
          {{ section.label }}
        </span>
      </button>
    </aside>

    <!-- 右侧内容 -->
    <div class="min-h-0 flex-1 overflow-y-auto pb-2">
      <div
        v-if="!settings && (activeSection === 'general' || activeSection === 'launch')"
        class="flex h-full flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-line/70 text-ink-muted"
      >
        <p class="text-[13px]">{{ settingsError || "正在读取设置…" }}</p>
        <AppButton v-if="settingsError" variant="secondary" size="sm" @click="loadSettings">
          重试
        </AppButton>
      </div>

      <!-- 通用 -->
      <div v-if="settings && activeSection === 'general'" class="flex flex-col gap-5">
        <AppCard title="下载与网络" padding="md">
          <template #icon>
            <Download :size="18" class="text-brand-500" />
          </template>

          <div class="flex flex-col gap-5">
            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between text-sm">
                <span class="font-medium text-ink">最大下载线程数</span>
                <span class="font-mono text-brand-500">{{ settings.download.maxConcurrent }}</span>
              </div>
              <input
                v-model.number="settings.download.maxConcurrent"
                type="range"
                min="1"
                max="32"
                class="w-full accent-brand-500"
                @change="saveSettings"
              />
              <p class="text-[12px] text-ink-muted">
                范围 1–32，默认 8。线程数过高反而会触发限流。
              </p>
            </div>

            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between text-sm">
                <span class="font-medium text-ink">下载限速</span>
                <span class="font-mono text-brand-500">
                  {{ limitMbps > 0 ? `${limitMbps} MB/s` : "不限速" }}
                </span>
              </div>
              <input
                v-model.number="limitMbps"
                type="range"
                min="0"
                max="20"
                step="0.5"
                class="w-full accent-brand-500"
                @change="saveSettings"
              />
              <p class="text-[12px] text-ink-muted">
                整个启动器的总下载带宽上限，0 表示不限。下载整合包等大流量场景可适当限制。
              </p>
            </div>

            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between text-sm">
                <span class="font-medium text-ink">最大内存</span>
                <span class="font-mono text-brand-500">{{ memoryMb }} MB</span>
              </div>
              <input
                v-model.number="memoryMb"
                type="range"
                min="512"
                max="8192"
                step="512"
                class="w-full accent-brand-500"
                @change="saveSettings"
              />
              <p class="text-[12px] text-ink-muted">
                启动时传给 Java 的 -Xmx；大型整合包建议 4096 以上。
              </p>
            </div>

            <div class="flex items-center justify-between">
              <div class="flex flex-col gap-1">
                <span class="font-medium text-ink">优先使用 BMCLAPI 镜像</span>
                <p class="text-[12px] text-ink-muted">
                  游戏、加载器与支持库优先走国内镜像，失败自动回退官方源。
                </p>
              </div>
              <label class="relative inline-flex cursor-pointer items-center">
                <input
                  v-model="settings.download.preferMirror"
                  type="checkbox"
                  class="peer sr-only"
                  @change="saveSettings"
                />
                <div
                  class="peer h-6 w-11 rounded-full bg-ink/20 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:bg-brand-500 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none"
                ></div>
              </label>
            </div>

            <div class="flex items-center justify-between">
              <div class="flex flex-col gap-1">
                <span class="font-medium text-ink">深色主题</span>
                <p class="text-[12px] text-ink-muted">切换深色界面，立即生效。</p>
              </div>
              <label class="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  :checked="settings.theme === 'dark'"
                  class="peer sr-only"
                  @change="setTheme"
                />
                <div
                  class="peer h-6 w-11 rounded-full bg-ink/20 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:bg-brand-500 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none"
                ></div>
              </label>
            </div>
          </div>
        </AppCard>

        <AppCard title="存储路径" padding="md">
          <template #icon>
            <FolderOpen :size="18" class="text-brand-500" />
          </template>

          <div class="flex flex-col gap-3">
            <p class="text-sm text-ink-muted">实例存放位置（游戏文件、Mod 等都会下载到这里）：</p>
            <div class="flex items-center gap-2">
              <div
                class="flex-1 truncate rounded-md border border-line/60 bg-surface-muted/50 px-3 py-2 font-mono text-[13px] text-ink"
              >
                {{ settings.instancesRoot || "默认文档目录" }}
              </div>
              <AppButton variant="secondary" @click="selectDir">更改</AppButton>
            </div>
          </div>
        </AppCard>

        <AppCard title="磁盘占用" padding="md">
          <template #icon>
            <HardDrive :size="18" class="text-brand-500" />
          </template>

          <div v-if="!storage" class="text-[12px] text-ink-muted">正在统计…</div>
          <div v-else class="flex flex-col gap-3">
            <div class="grid grid-cols-3 gap-3 text-center">
              <div class="rounded-md bg-surface-muted/40 py-2">
                <div class="font-mono text-[14px] text-ink">
                  {{ formatBytes(storage.instanceBytes) }}
                </div>
                <div class="text-[11px] text-ink-muted">实例</div>
              </div>
              <div class="rounded-md bg-surface-muted/40 py-2">
                <div class="font-mono text-[14px] text-ink">
                  {{ formatBytes(storage.cacheBytes) }}
                </div>
                <div class="text-[11px] text-ink-muted">缓存</div>
              </div>
              <div class="rounded-md bg-surface-muted/40 py-2">
                <div class="font-mono text-[14px] text-ink">
                  {{ formatBytes(storage.logBytes) }}
                </div>
                <div class="text-[11px] text-ink-muted">日志</div>
              </div>
            </div>

            <div v-if="storage.instances.length" class="flex flex-col gap-1">
              <div
                v-for="item in storage.instances.slice(0, 4)"
                :key="item.id"
                class="flex items-center justify-between text-[12px]"
              >
                <span class="truncate text-ink-soft">{{ item.name }}</span>
                <span class="shrink-0 font-mono text-ink-muted">{{ formatBytes(item.bytes) }}</span>
              </div>
            </div>

            <div class="flex items-center justify-between gap-3">
              <p class="text-[12px] text-ink-muted">
                缓存只存搜索页与版本列表，清理不会影响实例与存档。
              </p>
              <AppButton
                variant="secondary"
                size="sm"
                :loading="clearingCache"
                @click="clearCache"
              >
                清理缓存
              </AppButton>
            </div>
          </div>
        </AppCard>
      </div>

      <!-- 启动 -->
      <div v-if="settings && activeSection === 'launch'" class="flex flex-col gap-5">
        <AppCard title="游戏启动设置" padding="md">
          <template #icon>
            <Gamepad2 :size="18" class="text-brand-500" />
          </template>
          <div class="flex items-center justify-between">
            <div class="flex flex-col gap-1">
              <span class="font-medium text-ink">离线用户名</span>
              <p class="text-[12px] text-ink-muted">
                进入游戏后显示的名字（3–16 位字母/数字/下划线）。
              </p>
            </div>
            <div class="flex items-center gap-2">
              <AppInput
                v-model="settings.profile.name"
                placeholder="Player"
                class="w-40"
                @submit="saveSettings"
              />
              <AppButton variant="secondary" size="sm" @click="saveSettings">
                <Check :size="14" />
                保存
              </AppButton>
            </div>
          </div>

          <div class="mt-5 flex items-center justify-between">
            <div class="flex flex-col gap-1">
              <span class="font-medium text-ink">自动设置为简体中文</span>
              <p class="text-[12px] text-ink-muted">
                启动时强制追加 --lang zh_cn，防止旧版遗留设置变成英文。
              </p>
            </div>
            <label class="relative inline-flex cursor-pointer items-center">
              <input
                v-model="settings.logging.autoLanguageChinese"
                type="checkbox"
                class="peer sr-only"
                @change="saveSettings"
              />
              <div
                class="peer h-6 w-11 rounded-full bg-ink/20 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:bg-brand-500 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none"
              ></div>
            </label>
          </div>
        </AppCard>

        <AppCard title="Java 运行环境" padding="md">
          <template #icon>
            <Coffee :size="18" class="text-brand-500" />
          </template>

          <div class="flex flex-col gap-4">
            <div class="flex items-center justify-between">
              <p class="text-sm text-ink-muted">启动游戏所需的 Java 环境列表。</p>
              <AppButton variant="secondary" @click="scanJava" :loading="loading">
                <RefreshCw :size="14" :class="{ 'animate-spin': loading }" />
                重新扫描
              </AppButton>
            </div>

            <div
              v-if="missingJavaMajors.length"
              class="flex flex-wrap items-center gap-2 rounded-lg border border-dashed border-line/60 px-3 py-2.5"
            >
              <span class="text-[12px] text-ink-muted">缺少运行时，可自动下载：</span>
              <AppButton
                v-for="major in missingJavaMajors"
                :key="major"
                variant="secondary"
                size="sm"
                :loading="downloadingJava === major"
                :disabled="downloadingJava !== null"
                @click="installJava(major)"
              >
                Java {{ major }}
              </AppButton>
            </div>
            <p v-if="javaError" class="text-[12px] text-danger">{{ javaError }}</p>

            <div
              v-if="javas.length === 0 && !loading"
              class="rounded-lg border border-dashed border-line/60 py-4 text-center text-sm text-ink-muted"
            >
              未找到 Java 环境
            </div>

            <div v-else class="flex flex-col gap-2">
              <div
                v-for="j in javas"
                :key="j.path"
                class="flex items-center justify-between rounded-lg border border-line/60 bg-surface-muted/50 p-3"
              >
                <div class="flex min-w-0 flex-col">
                  <span class="font-mono text-sm font-medium text-ink">{{ j.versionString }}</span>
                  <span class="truncate text-[11px] text-ink-muted" :title="j.path">{{
                    j.path
                  }}</span>
                </div>
                <div class="flex shrink-0 items-center gap-2">
                  <AppBadge v-if="settings.java.customPath === j.path" variant="brand"
                    >当前使用</AppBadge
                  >
                  <AppBadge variant="brand">{{ j.arch }}</AppBadge>
                  <AppBadge variant="success">Java {{ j.majorVersion }}</AppBadge>
                  <AppButton
                    variant="secondary"
                    size="sm"
                    :disabled="settings.java.customPath === j.path"
                    @click="useJava(j)"
                  >
                    使用
                  </AppButton>
                </div>
              </div>
            </div>
          </div>
        </AppCard>
      </div>

      <!-- 关于 -->
      <div
        v-if="activeSection === 'about'"
        class="flex flex-col items-center pt-10 pb-8 text-center"
      >
        <svg
          viewBox="0 0 100 100"
          class="h-28 w-28 shrink-0 text-brand-500"
          role="img"
          aria-label="Sereno"
        >
          <defs>
            <mask
              id="sereno-about-cut"
              maskUnits="userSpaceOnUse"
              x="0"
              y="0"
              width="100"
              height="100"
            >
              <rect width="100" height="100" fill="#fff" />
              <rect
                x="36"
                y="45.5"
                width="28"
                height="5"
                fill="#000"
                transform="rotate(65 50 48)"
              />
            </mask>
          </defs>
          <path
            d="M 72 24 L 38 24 A 12 12 0 0 0 38 48 L 62 48 A 12 12 0 0 1 62 72 L 28 72"
            fill="none"
            stroke="currentColor"
            stroke-width="14"
            stroke-linecap="round"
            stroke-linejoin="round"
            mask="url(#sereno-about-cut)"
          />
        </svg>

        <h2 class="mt-7 font-display text-[28px] font-semibold tracking-tight text-ink">
          {{ APP_NAME }}
        </h2>
        <div class="mt-2 flex items-center gap-2 text-[12px] text-ink-muted">
          <span class="font-mono">v{{ APP_VERSION }}</span>
          <span class="h-3 w-px bg-line/60" />
          <span>{{ APP_AUTHOR }}</span>
        </div>

        <div class="mt-7 flex items-center gap-2.5">
          <AppButton variant="secondary" size="sm" @click="openRepo">
            <svg viewBox="0 0 16 16" class="h-3.5 w-3.5" fill="currentColor" aria-hidden="true">
              <path
                d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
              />
            </svg>
            GitHub 仓库
          </AppButton>
          <AppButton variant="primary" size="sm" @click="sponsorFor('sereno')">
            <Heart :size="14" />
            赞助
          </AppButton>
        </div>

        <p class="mt-4 max-w-sm text-[11.5px] leading-6 text-ink-muted">
          如果 Sereno 帮你省下了一点折腾的时间，可以考虑请我喝杯奶茶。赞助不会带来任何额外功能，只是让我更有底气长期维护它。
        </p>

        <div class="mt-9 h-px w-full max-w-sm bg-line/50" />
        <p class="mt-6 text-[11px] text-ink-muted">© 2026 {{ APP_AUTHOR }}</p>
      </div>

      <!-- 鸣谢 -->
      <div v-if="activeSection === 'credits'" class="pt-4">
        <div class="flex items-center gap-2">
          <HeartHandshake :size="18" class="text-brand-500" />
          <h2 class="font-display text-[20px] font-semibold tracking-tight text-ink">特别鸣谢</h2>
        </div>
        <div class="mt-5 border-t border-line/40">
          <div
            v-for="item in acknowledgements"
            :key="item.name"
            class="flex items-center justify-between gap-6 border-b border-line/40 py-5"
          >
            <div class="min-w-0">
              <h3 class="text-[15px] font-semibold text-ink">{{ item.name }}</h3>
              <p class="mt-1.5 max-w-xl text-[12px] leading-6 text-ink-muted">{{ item.desc }}</p>
            </div>
            <button
              v-if="item.supporter"
              type="button"
              class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-[12px] font-medium text-brand-600 transition-colors hover:bg-brand-500/8 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/35"
              title="打开爱发电赞助页"
              @click="sponsorFor(item.supporter)"
            >
              <Heart :size="13" />
              赞助
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
