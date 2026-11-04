<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  Coffee,
  Download,
  FolderOpen,
  Gamepad2,
  HardDrive,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
} from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppBadge from "@/components/common/AppBadge.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppSlider from "@/components/common/AppSlider.vue";
import AppSwitch from "@/components/common/AppSwitch.vue";
import SettingsRow from "@/components/common/SettingsRow.vue";
import PageShell from "@/components/layout/PageShell.vue";
import SideNav from "@/components/layout/SideNav.vue";
import type { SideNavItem } from "@/components/layout/SideNav.vue";
import { commands } from "@/bindings";
import { callCommand } from "@/api/invoke";
import { installJavaRuntime } from "@/api/java";
import { clearStorageCache, getStorageUsage } from "@/api/storage";
import { checkAppUpdate, setUpdateAutoCheck } from "@/api/update";
import { setOfflineAccount } from "@/api/account";
import { isApiError } from "@/api/invoke";
import { useToastStore } from "@/stores/toast";
import { formatBytes } from "@/utils/format";
import { log } from "@/api/settings";
import { applyTheme } from "@/utils/theme";
import type { AppSettings_Serialize, JavaCandidate, StorageUsage } from "@/types";

type SectionId = "general" | "launch" | "update";

const sections: SideNavItem[] = [
  { id: "general", label: "通用", icon: SlidersHorizontal },
  { id: "launch", label: "启动", icon: Gamepad2 },
  { id: "update", label: "更新", icon: RefreshCw },
];

const route = useRoute();
const router = useRouter();
const toast = useToastStore();

/** 把任意抛出物转成可读文案。 */
function toMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

/** 当前分区：支持命令面板用 `?section=launch` 直接跳进来。 */
const activeSection = ref<SectionId>(
  route.query.section === "launch"
    ? "launch"
    : route.query.section === "update"
      ? "update"
      : "general",
);
const loading = ref(false);
const javas = ref<JavaCandidate[]>([]);
const settings = ref<AppSettings_Serialize | null>(null);
const settingsError = ref("");
const downloadingJava = ref<number | null>(null);
const javaError = ref("");
const storage = ref<StorageUsage | null>(null);
const clearingCache = ref(false);
/** 手动检查更新的进行态。 */
const checkingUpdate = ref(false);
/** 离线用户名的输入框内容（与设置里的 profile.name 分开，避免每次输入都写盘）。 */
const offlineName = ref("");
/** 切换离线档案的进行态。 */
const switchingOffline = ref(false);
const offlineNameValid = computed(() => /^[A-Za-z0-9_]{3,16}$/.test(offlineName.value.trim()));

/**
 * 应用离线用户名。
 *
 * 走 `setOfflineAccount` 而不是直接改 `profile.name`：
 * 同名会切到已有档案，新名则新建一个——两条路径统一到
 * 账户中心的档案列表，不会出现「设置里改了名、账户列表里还是旧的」。
 */
async function applyOfflineName(): Promise<void> {
  const name = offlineName.value.trim();
  if (!offlineNameValid.value || switchingOffline.value) return;
  switchingOffline.value = true;
  try {
    const account = await setOfflineAccount(name);
    if (settings.value) settings.value.profile.name = account.name;
    offlineName.value = account.name;
    toast.success(account.name === name ? "已切换离线档案" : "已创建离线档案", account.name);
  } catch (e) {
    toast.error("切换失败", toMessage(e));
  } finally {
    switchingOffline.value = false;
  }
}

/** 切换「启动时自动检查更新」。 */
async function onToggleAutoCheck(value: boolean): Promise<void> {
  const previous = settings.value?.update.autoCheck ?? true;
  if (settings.value) settings.value.update.autoCheck = value;
  try {
    await setUpdateAutoCheck(value);
    toast.success(value ? "已开启自动检查" : "已关闭自动检查");
  } catch (e) {
    if (settings.value) settings.value.update.autoCheck = previous;
    toast.error("保存设置失败", toMessage(e));
  }
}

/** 手动检查一次更新。 */
async function checkNow(): Promise<void> {
  if (checkingUpdate.value) return;
  checkingUpdate.value = true;
  try {
    const result = await checkAppUpdate();
    if (result.available) {
      toast.success(
        `发现新版本 v${result.latestVersion}`,
        `当前版本 v${result.currentVersion}`,
      );
    } else {
      toast.success("已是最新版本", `v${result.currentVersion}`);
    }
  } catch (e) {
    toast.error("检查更新失败", toMessage(e));
  } finally {
    checkingUpdate.value = false;
  }
}

/** 跳到独立的更新页。 */
function goUpdatePage(): void {
  void router.push("/update");
}

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

/** 代理地址：空串表示不走代理（配置里存 `null`）。 */
const proxy = computed({
  get: () => settings.value?.http.proxy ?? "",
  set: (value: string) => {
    if (!settings.value) return;
    const trimmed = value.trim();
    settings.value.http.proxy = trimmed.length > 0 ? trimmed : null;
  },
});

async function loadSettings(): Promise<void> {
  try {
    settings.value = await callCommand(() => commands.getSettings());
    applyTheme(settings.value.theme);
    // 离线用户名的输入框初值取当前档案名
    offlineName.value = settings.value.profile.name;
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
function setTheme(value: boolean): void {
  if (!settings.value) return;
  settings.value.theme = value ? "dark" : "light";
  applyTheme(settings.value.theme);
  void saveSettings();
}

/** 选中某个 Java 作为默认启动运行时。 */
async function useJava(candidate: JavaCandidate): Promise<void> {
  if (!settings.value) return;
  settings.value.java.customPath = candidate.path;
  await saveSettings();
}

/**
 * 命令面板跳转：`?section=launch&focus=java`。
 *
 * 只切分区不够——用户还要在一屏卡片里继续找那一行，
 * 所以再滚到目标行并短暂高亮，视线有落点。
 */
async function applyRouteFocus(): Promise<void> {
  const section = route.query.section;
  if (section === "launch" || section === "general" || section === "update") {
    activeSection.value = section;
  }

  const focus = route.query.focus;
  if (typeof focus !== "string") return;

  await nextTick();
  const target = document.getElementById(`setting-${focus}`);
  if (!target) return;
  target.scrollIntoView({ block: "center", behavior: "smooth" });
  target.classList.add("is-highlight");
  window.setTimeout(() => target.classList.remove("is-highlight"), 1400);
}

watch(
  () => [route.query.section, route.query.focus],
  () => void applyRouteFocus(),
);

onMounted(() => {
  void loadSettings();
  void scanJava();
  void loadStorage();
  void applyRouteFocus();
});
</script>

<template>
  <PageShell :chrome="false" width="xl">
    <div class="flex min-h-0 flex-1 gap-6">
      <SideNav v-model="activeSection" :items="sections" title="设置" />

      <div class="min-h-0 flex-1 overflow-y-auto pb-2">
        <div
          v-if="!settings"
          class="flex h-64 flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-line/70 text-ink-muted"
        >
          <p class="text-body">{{ settingsError || "正在读取设置…" }}</p>
          <AppButton v-if="settingsError" variant="secondary" size="sm" @click="loadSettings">
            重试
          </AppButton>
        </div>

        <Transition v-else name="tab" mode="out-in">
          <!-- 通用 -->
          <div v-if="activeSection === 'general'" key="general" class="flex flex-col gap-4">
            <AppCard title="下载与网络" padding="lg">
              <template #icon>
                <Download :size="16" class="text-brand-500" />
              </template>

              <SettingsRow
                title="最大下载线程数"
                description="范围 1–32，默认 8。线程数过高反而会触发镜像限流。"
              >
                <div class="flex w-[248px] items-center gap-3">
                  <AppSlider
                    :model-value="settings.download.maxConcurrent"
                    :min="1"
                    :max="32"
                    label="最大下载线程数"
                    class="flex-1"
                    @update:model-value="(value) => (settings!.download.maxConcurrent = value)"
                    @change="saveSettings"
                  />
                  <span class="value-chip">{{ settings.download.maxConcurrent }}</span>
                </div>
              </SettingsRow>

              <SettingsRow
                title="下载限速"
                description="整个启动器的总带宽上限，0 表示不限。"
              >
                <div class="flex w-[248px] items-center gap-3">
                  <AppSlider
                    :model-value="limitMbps"
                    :min="0"
                    :max="20"
                    :step="0.5"
                    label="下载限速"
                    class="flex-1"
                    @update:model-value="(value) => (limitMbps = value)"
                    @change="saveSettings"
                  />
                  <span class="value-chip">
                    {{ limitMbps > 0 ? `${limitMbps} MB/s` : "不限" }}
                  </span>
                </div>
              </SettingsRow>

              <SettingsRow
                id="setting-memory"
                title="最大内存"
                description="启动时传给 Java 的 -Xmx，大型整合包建议 4096 以上。"
              >
                <div class="flex w-[248px] items-center gap-3">
                  <AppSlider
                    :model-value="memoryMb"
                    :min="512"
                    :max="8192"
                    :step="512"
                    label="最大内存"
                    class="flex-1"
                    @update:model-value="(value) => (memoryMb = value)"
                    @change="saveSettings"
                  />
                  <span class="value-chip">{{ memoryMb }} MB</span>
                </div>
              </SettingsRow>

              <SettingsRow
                title="优先使用国内镜像"
                description="游戏与加载器走 BMCLAPI，Mod 搜索走 MCIM；关闭后回退官方源。"
                orientation="inline"
              >
                <AppSwitch
                  :model-value="settings.download.preferMirror"
                  label="优先使用国内镜像"
                  @update:model-value="
                    (value) => {
                      settings!.download.preferMirror = value;
                      saveSettings();
                    }
                  "
                />
              </SettingsRow>

              <SettingsRow title="深色主题" description="立即生效，不需要重启。">
                <AppSwitch
                  :model-value="settings.theme === 'dark'"
                  label="深色主题"
                  @update:model-value="setTheme"
                />
              </SettingsRow>

              <SettingsRow
                title="网络代理"
                description="支持 http:// 与 socks5://，留空表示直连。修改后下一次请求生效。"
                orientation="stack"
              >
                <AppInput
                  v-model="proxy"
                  placeholder="例如 http://127.0.0.1:7890"
                  @change="saveSettings"
                />
              </SettingsRow>
            </AppCard>

            <AppCard title="存储路径" padding="lg">
              <template #icon>
                <FolderOpen :size="16" class="text-brand-500" />
              </template>

              <SettingsRow
                id="setting-storage"
                title="实例存放位置"
                description="游戏文件、Mod、存档都会下载到这里。"
                orientation="stack"
              >
                <div class="flex items-center gap-2">
                  <div
                    class="flex-1 truncate rounded-md border border-line/60 bg-surface-muted px-3 py-2 font-mono text-caption text-ink"
                    :title="settings!.instancesRoot || '默认文档目录'"
                  >
                    {{ settings!.instancesRoot || "默认文档目录" }}
                  </div>
                  <AppButton variant="secondary" size="sm" @click="selectDir">更改</AppButton>
                </div>
              </SettingsRow>
            </AppCard>

            <AppCard title="磁盘占用" padding="lg">
              <template #icon>
                <HardDrive :size="16" class="text-brand-500" />
              </template>

              <div v-if="!storage" class="py-4 text-caption text-ink-muted">正在统计…</div>
              <div v-else class="flex flex-col gap-4">
                <div class="grid grid-cols-3 gap-3 text-center">
                  <div class="rounded-md bg-surface-muted py-3">
                    <div class="font-mono text-h2 text-ink tabular">
                      {{ formatBytes(storage.instanceBytes) }}
                    </div>
                    <div class="mt-1 text-micro text-ink-muted">实例</div>
                  </div>
                  <div class="rounded-md bg-surface-muted py-3">
                    <div class="font-mono text-h2 text-ink tabular">
                      {{ formatBytes(storage.cacheBytes) }}
                    </div>
                    <div class="mt-1 text-micro text-ink-muted">缓存</div>
                  </div>
                  <div class="rounded-md bg-surface-muted py-3">
                    <div class="font-mono text-h2 text-ink tabular">
                      {{ formatBytes(storage.logBytes) }}
                    </div>
                    <div class="mt-1 text-micro text-ink-muted">日志</div>
                  </div>
                </div>

                <div v-if="storage.instances.length" class="flex flex-col divide-y divide-line/50">
                  <div
                    v-for="item in storage.instances.slice(0, 5)"
                    :key="item.id"
                    class="flex items-center justify-between py-1.5 text-caption"
                  >
                    <span class="truncate text-ink-soft">{{ item.name }}</span>
                    <span class="shrink-0 font-mono text-ink-muted tabular">
                      {{ formatBytes(item.bytes) }}
                    </span>
                  </div>
                </div>

                <div class="flex items-center justify-between gap-3 border-t border-line/50 pt-3">
                  <p class="text-micro text-ink-muted">
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
          <div v-if="activeSection === 'launch'" key="launch" class="flex flex-col gap-4">
            <AppCard title="游戏启动设置" padding="lg">
              <template #icon>
                <Gamepad2 :size="16" class="text-brand-500" />
              </template>

              <SettingsRow
                title="离线用户名"
                description="进入游戏后显示的名字（3–16 位字母 / 数字 / 下划线）。"
              >
                <div class="flex items-center gap-2">
                  <AppInput
                    v-model="offlineName"
                    placeholder="Player"
                    class="w-40"
                    :invalid="offlineName.length > 0 && !offlineNameValid"
                    @submit="applyOfflineName"
                  />
                  <AppButton
                    variant="secondary"
                    size="sm"
                    :disabled="!offlineNameValid || !!switchingOffline"
                    :loading="switchingOffline"
                    @click="applyOfflineName"
                  >
                    应用
                  </AppButton>
                </div>
              </SettingsRow>

              <SettingsRow
                title="自动设置为简体中文"
                description="启动时追加 --lang zh_cn，并把 options.txt 的语言改好。"
              >
                <AppSwitch
                  :model-value="settings!.logging.autoLanguageChinese"
                  label="自动设置为简体中文"
                  @update:model-value="
                    (value) => {
                      settings!.logging.autoLanguageChinese = value;
                      saveSettings();
                    }
                  "
                />
              </SettingsRow>
            </AppCard>

            <AppCard id="setting-java" title="Java 运行环境" padding="lg">
              <template #icon>
                <Coffee :size="16" class="text-brand-500" />
              </template>

              <div class="flex items-center justify-between pb-3">
                <p class="text-caption text-ink-muted">
                  启动游戏需要匹配的 Java 主版本，Sereno 会自动挑选。
                </p>
                <AppButton variant="secondary" size="sm" :loading="loading" @click="scanJava">
                  <RefreshCw :size="14" :class="{ 'animate-spin': loading }" />
                  重新扫描
                </AppButton>
              </div>

              <div
                v-if="missingJavaMajors.length"
                class="mb-3 flex flex-wrap items-center gap-2 rounded-md border border-dashed border-line/70 px-3 py-2"
              >
                <span class="text-micro text-ink-muted">缺少运行时，可自动下载：</span>
                <AppButton
                  v-for="major in missingJavaMajors"
                  :key="major"
                  variant="secondary"
                  size="xs"
                  :loading="downloadingJava === major"
                  :disabled="downloadingJava !== null"
                  @click="installJava(major)"
                >
                  Java {{ major }}
                </AppButton>
              </div>

              <p v-if="javaError" class="mb-3 text-caption text-danger">{{ javaError }}</p>

              <p
                v-if="javas.length === 0 && !loading"
                class="rounded-md border border-dashed border-line/70 py-6 text-center text-caption text-ink-muted"
              >
                未找到 Java 环境，可以先自动下载，或在实例设置里手动指定路径。
              </p>

              <!--
                这里用完整的 v-if 而不是 v-else：
                上面隔着 `<p v-if="javaError">`，v-else 必须紧邻前一个兄弟元素才成立。
                （而且加上更新分区后，顶层也需要一条完整的 v-if / v-else-if 链。）
              -->
              <div
                v-if="javas.length > 0 || loading"
                class="flex flex-col divide-y divide-line/50"
              >
                <div
                  v-for="j in javas"
                  :key="j.path"
                  class="flex items-center justify-between gap-3 py-2.5"
                >
                  <div class="flex min-w-0 flex-col">
                    <div class="flex items-center gap-2">
                      <span class="font-mono text-h3 font-medium text-ink">
                        {{ j.versionString }}
                      </span>
                      <AppBadge variant="success" size="sm">Java {{ j.majorVersion }}</AppBadge>
                      <AppBadge variant="default" size="sm" outline>{{ j.arch }}</AppBadge>
                    </div>
                    <span class="truncate text-micro text-ink-muted" :title="j.path">{{ j.path }}</span>
                  </div>
                  <AppButton
                    :variant="settings!.java.customPath === j.path ? 'ghost' : 'secondary'"
                    size="sm"
                    :disabled="settings!.java.customPath === j.path"
                    @click="useJava(j)"
                  >
                    {{ settings!.java.customPath === j.path ? "当前使用" : "使用" }}
                  </AppButton>
                </div>
              </div>
            </AppCard>
          </div>

          <!-- 更新 -->
          <div v-if="activeSection === 'update'" key="update" class="flex flex-col gap-4">
            <AppCard title="启动器更新" padding="lg">
              <template #icon>
                <RefreshCw :size="16" class="text-brand-500" />
              </template>

              <SettingsRow
                title="启动时自动检查更新"
                description="启动器打开时自动检查一次有没有新版本。只发一个请求，不影响启动速度。"
              >
                <AppSwitch
                  :model-value="settings!.update.autoCheck"
                  label="启动时自动检查更新"
                  @update:model-value="onToggleAutoCheck"
                />
              </SettingsRow>

              <SettingsRow
                title="检查更新"
                description="手动检查一次。更新包必须通过数字签名校验，验签不通过不会被安装。"
              >
                <AppButton variant="secondary" size="sm" :loading="checkingUpdate" @click="checkNow">
                  <RefreshCw :size="13" :class="{ 'animate-spin': checkingUpdate }" />
                  检查更新
                </AppButton>
              </SettingsRow>
            </AppCard>

            <AppCard title="更新通道" padding="lg">
              <template #icon>
                <ShieldCheck :size="16" class="text-brand-500" />
              </template>

              <div class="flex flex-col gap-2 text-caption leading-6 text-ink-muted">
                <p>
                  更新包从官方渠道下载，并用内置的 minisign
                  公钥验签——签名不匹配的更新包不会被安装。
                </p>
                <p>
                  下载优先走腾讯云 COS，失败时自动回落到 GitHub Release；
                  两个端点都不可用时会明确报错，不会静默失败。
                </p>
              </div>

              <div class="mt-4">
                <AppButton variant="soft" size="sm" @click="goUpdatePage">
                  打开更新页
                </AppButton>
              </div>
            </AppCard>
          </div>
        </Transition>
      </div>
    </div>
  </PageShell>
</template>

<style scoped>
/* 数值标签：把「32 / 8192 MB」做成一个等宽小胶囊，
   右对齐后一列数值天然成一条竖线，比裸文字整齐得多 */
.value-chip {
  min-width: 56px;
  padding: 3px 8px;
  border-radius: var(--radius-sm);
  background: var(--color-surface-muted);
  color: var(--color-ink-soft);
  font-family: var(--font-mono);
  font-size: var(--text-caption);
  line-height: 1.4;
  text-align: center;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

/* 命令面板跳转后的短暂高亮：让用户一眼看到「要找的是这一行」 */
:deep(.is-highlight) {
  animation: setting-highlight 1.4s var(--ease-out-soft);
}

@keyframes setting-highlight {
  0%,
  100% {
    background: transparent;
  }
  15%,
  55% {
    background: var(--alpha-brand-8);
  }
}
</style>
