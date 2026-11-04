<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  BadgeCheck,
  Check,
  Copy,
  ExternalLink,
  LogOut,
  Plus,
  ShieldCheck,
  Trash2,
  UserRound,
} from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import {
  activateMicrosoftAccount,
  addOfflineAccount,
  authlibLogin,
  cancelMicrosoftLogin,
  completeMicrosoftLogin,
  getAccountState,
  getMicrosoftConfig,
  logoutAccount,
  logoutAuthlibAccount,
  openMicrosoftLogin,
  removeOfflineAccount,
  startMicrosoftLogin,
  switchAuthlibAccount,
  switchOfflineAccount,
} from "@/api/account";
import { isApiError } from "@/api/invoke";
import { toMessage } from "@/utils/error";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import { askConfirm } from "@/composables/useConfirm";
import type { AccountState, MicrosoftConfig } from "@/types";

/**
 * 账户面板：离线档案列表 + 微软正版登录。
 *
 * 离线档案创建时固化 UUID（`md5("OfflinePlayer:<name>")`）。
 * Minecraft 的玩家数据按 UUID 归属，切换档案就等于换了一个玩家；
 * 想保留同一个存档里的物品，请切回创建它的那个档案。
 */
const props = withDefaults(defineProps<{ open?: boolean }>(), { open: true });
const emit = defineEmits<{ (e: "close"): void }>();

const toast = useToastStore();
const state = ref<AccountState | null>(null);
const config = ref<MicrosoftConfig | null>(null);
const newName = ref("");

const loading = ref(true);
const busy = ref("");
const error = ref("");

const deviceCode = ref("");
const verificationUri = ref("");
const copied = ref(false);
const autoCopied = ref(false);

const isMicrosoft = computed(() => state.value?.kind === "microsoft");
const isAuthlib = computed(() => state.value?.kind === "authlib");
const canLogin = computed(() => Boolean(config.value?.configured));
const ephemeral = computed(() => state.value?.credentialStoreEphemeral ?? false);
const apiPending = computed(() => error.value.includes("审批中"));
const offlineAccounts = computed(() => state.value?.offlineAccounts ?? []);
const microsoftProfile = computed(() => state.value?.microsoftProfile ?? null);
const authlibProfiles = computed(() => state.value?.authlibProfiles ?? []);

const nameValid = computed(() => /^[A-Za-z0-9_]{3,16}$/.test(newName.value.trim()));

/**
 * 账户中心的两个分区。
 *
 * 之前两类账户纵向堆在同一页：微软正版、当前档案、其它档案、新建表单、登录区从上到下排开，
 * 弹窗被拉得很长，而且右侧还压着一个突兀的浅蓝色圆斑。
 * 改成顶部**分段控制器**后，每个分区只展示自己那一类内容，弹窗高度回到一屏之内。
 */
type AccountTab = "offline" | "microsoft" | "authlib";
const activeTab = ref<AccountTab>("offline");
const tabs: { id: AccountTab; label: string }[] = [
  { id: "offline", label: "离线档案" },
  { id: "microsoft", label: "微软正版" },
  { id: "authlib", label: "第三方登录" },
];

/** 第三方登录表单。 */
const authlibForm = ref({
  apiRoot: "",
  username: "",
  password: "",
  serverName: "",
});

/** 当前正在使用的第三方档案。 */
const currentAuthlib = computed(() => {
  const id = state.value?.activeAuthlibId;
  return authlibProfiles.value.find((profile) => profile.id === id) ?? null;
});

/** 除当前使用中之外的第三方档案。 */
const otherAuthlibProfiles = computed(() => {
  const id = state.value?.activeAuthlibId;
  return authlibProfiles.value.filter((profile) => profile.id !== id);
});

/** 常见的第三方皮肤站，省得用户去翻文档。 */
const AUTHLIB_PRESETS: { label: string; apiRoot: string }[] = [
  { label: "LittleSkin", apiRoot: "https://littleskin.cn/api/yggdrasil" },
  { label: "Ely.by", apiRoot: "https://api.ely.by/api/yggdrasil" },
];

/** 当前正在使用的离线档案（微软账户占用时为空）。 */
const currentOffline = computed(() => {
  if (isMicrosoft.value) return null;
  const id = state.value?.activeOfflineId;
  return offlineAccounts.value.find((profile) => profile.id === id) ?? null;
});

/** 除当前使用中之外的离线档案。 */
const otherAccounts = computed(() => {
  if (!isMicrosoft.value) {
    const id = state.value?.activeOfflineId;
    return offlineAccounts.value.filter((profile) => profile.id !== id);
  }
  return offlineAccounts.value;
});

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    const [account, microsoft] = await Promise.all([getAccountState(), getMicrosoftConfig()]);
    state.value = account;
    config.value = microsoft;
    newName.value = "";
  } catch (err) {
    error.value = `读取账户失败：${toMessage(err)}`;
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.open,
  (open) => {
    if (open) void load();
  },
);

/*
 * 账户类型变化时自动切到对应分区。
 * 只监听 kind 本身，因此用户手动切 tab 之后不会被立刻弹回去。
 */
watch(
  () => state.value?.kind,
  (kind) => {
    if (kind === "microsoft") activeTab.value = "microsoft";
    else if (kind === "authlib") activeTab.value = "authlib";
    else if (kind === "offline") activeTab.value = "offline";
  },
);

/** 离线用户名只允许 ASCII：输入时直接过滤，避免中文名进游戏后无法登录。 */
function onNameInput(value: string): void {
  newName.value = value.replace(/[^A-Za-z0-9_]/g, "").slice(0, 16);
}

async function run(key: string, action: () => Promise<AccountState>): Promise<void> {
  busy.value = key;
  error.value = "";
  try {
    state.value = await action();
  } catch (err) {
    error.value = toMessage(err);
    void log.error(`账户操作失败: ${error.value}`, { component: "AccountModal", action: key });
  } finally {
    busy.value = "";
  }
}

function createOffline(): void {
  if (!nameValid.value) {
    error.value = "离线用户名需为 3–16 位字母、数字或下划线";
    return;
  }
  void run("add", () => addOfflineAccount(newName.value.trim())).then(() => {
    if (!error.value) {
      toast.success("离线档案已创建", newName.value.trim());
      newName.value = "";
    }
  });
}

function switchOffline(id: string): void {
  if (!isMicrosoft.value && state.value?.activeOfflineId === id) return;
  void run(`switch-${id}`, () => switchOfflineAccount(id)).then(() => {
    if (!error.value) toast.success("已切换离线档案", state.value?.name ?? "");
  });
}

function removeOffline(id: string, name: string): void {
  void askConfirm({
    title: `删除离线档案「${name}」？`,
    message: "只删除这个档案条目，存档里的玩家数据不会被删除。",
    confirmText: "删除",
    danger: true,
  }).then((confirmed) => {
    if (!confirmed) return;
    void run(`remove-${id}`, () => removeOfflineAccount(id));
  });
}

function switchMicrosoft(): void {
  void run("activate", () => activateMicrosoftAccount()).then(() => {
    if (!error.value) toast.success("已切换到微软正版", state.value?.name ?? "");
  });
}

async function login(): Promise<void> {
  if (busy.value || !canLogin.value || !config.value) return;
  busy.value = "login";
  error.value = "";
  deviceCode.value = "";
  verificationUri.value = "";
  copied.value = false;
  autoCopied.value = false;

  try {
    const info = await startMicrosoftLogin();
    deviceCode.value = info.userCode;
    verificationUri.value = info.verificationUri;

    // 拉起浏览器前先把短码放进剪贴板，用户直接粘贴即可
    try {
      await navigator.clipboard.writeText(info.userCode);
      autoCopied.value = true;
    } catch {
      // 剪贴板不可用时短码仍然可见，可手动复制
    }
    void openMicrosoftLogin(info.verificationUri).catch(() => {});

    state.value = await completeMicrosoftLogin(info.deviceCode, info.interval, info.expiresIn);
    toast.success("微软登录成功", state.value.name);
    newName.value = "";
  } catch (err) {
    if (isApiError(err) && err.type === "cancelled") return;
    const message = toMessage(err);
    error.value = message;
    void log.error(`微软登录失败: ${message}`, { component: "AccountModal" });
  } finally {
    busy.value = "";
    deviceCode.value = "";
    verificationUri.value = "";
  }
}

async function openBrowser(): Promise<void> {
  if (!verificationUri.value) return;
  try {
    await openMicrosoftLogin(verificationUri.value);
  } catch (err) {
    error.value = toMessage(err);
  }
}

function handleClose(): void {
  if (busy.value === "login") void cancelMicrosoftLogin().catch(() => {});
  emit("close");
}

async function copyCode(): Promise<void> {
  if (!deviceCode.value) return;
  try {
    await navigator.clipboard.writeText(deviceCode.value);
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1600);
  } catch {
    // 剪贴板不可用时可手动抄写
  }
}

async function logout(): Promise<void> {
  busy.value = "logout";
  error.value = "";
  try {
    state.value = await logoutAccount();
    toast.success("已退出微软账户", "已切回离线档案");
  } catch (err) {
    error.value = `退出登录失败：${toMessage(err)}`;
  } finally {
    busy.value = "";
  }
}

/* ---------------- 第三方登录（Yggdrasil） ---------------- */

/** 填入常见的皮肤站地址，用户改个域名就能用。 */
function applyAuthlibPreset(apiRoot: string, serverName: string): void {
  authlibForm.value.apiRoot = apiRoot;
  authlibForm.value.serverName = serverName;
}

async function loginAuthlib(): Promise<void> {
  if (busy.value) return;
  const form = authlibForm.value;
  if (!form.apiRoot.trim() || !form.username.trim() || !form.password) {
    error.value = "请填写服务器地址、账号与密码";
    return;
  }

  busy.value = "authlib-login";
  error.value = "";
  try {
    state.value = await authlibLogin(
      form.apiRoot.trim(),
      form.username.trim(),
      form.password,
      form.serverName.trim() || null,
    );
    toast.success("第三方登录成功", state.value.name);
    // 密码只在这一刻存在于内存里，用完立刻清掉
    authlibForm.value.password = "";
    activeTab.value = "authlib";
  } catch (err) {
    const message = toMessage(err);
    error.value = message;
    void log.error(`第三方登录失败: ${message}`, { component: "AccountModal" });
  } finally {
    busy.value = "";
  }
}

function switchAuthlib(id: string): void {
  if (isAuthlib.value && state.value?.activeAuthlibId === id) return;
  void run(`switch-authlib-${id}`, () => switchAuthlibAccount(id)).then(() => {
    if (!error.value) toast.success("已切换第三方账户", state.value?.name ?? "");
  });
}

function removeAuthlib(id: string, name: string): void {
  void askConfirm({
    title: `退出第三方账户「${name}」？`,
    message: "会删除本机保存的登录令牌，皮肤站上的账号不受影响。",
    confirmText: "退出",
    danger: true,
  }).then((confirmed) => {
    if (!confirmed) return;
    void run(`remove-authlib-${id}`, () => logoutAuthlibAccount(id));
  });
}
</script>

<template>
  <AppModal width="sm" :open="open" @close="handleClose">
    <template #header>
      <div class="flex min-w-0 flex-1 items-center gap-3">
        <div
          class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
        >
          <BadgeCheck v-if="isMicrosoft" :size="18" />
          <ShieldCheck v-else-if="isAuthlib" :size="18" />
          <UserRound v-else :size="18" />
        </div>
        <div class="min-w-0">
          <h2 class="text-h2 font-semibold leading-5 text-ink">账户中心</h2>
          <p class="truncate text-micro text-ink-muted">
            {{ isMicrosoft ? "微软正版账户" : isAuthlib ? "第三方账户" : "离线档案" }} ·
            {{ state?.name ?? "读取中" }}
          </p>
        </div>
      </div>
    </template>

    <div v-if="loading" class="flex h-48 items-center justify-center">
      <AppLoading :size="56" label="正在读取账户…" />
    </div>

    <template v-else>
      <!-- 分段控制器：两类账户各占一个分区，弹窗不再被拉得很长 -->
      <div class="mb-4 flex gap-1 rounded-lg bg-surface-muted p-1">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          type="button"
          class="flex-1 rounded-md py-1.5 text-body font-medium transition-fast"
          :class="
            activeTab === tab.id
              ? 'bg-surface font-semibold text-ink shadow-sm'
              : 'text-ink-muted hover:text-ink-soft'
          "
          @click="activeTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </div>

      <!-- ================= 离线档案 ================= -->
      <div v-if="activeTab === 'offline'" class="flex flex-col gap-4">
        <!-- 当前正在使用的档案 -->
        <div
          v-if="currentOffline"
          class="flex items-center gap-3 rounded-lg border border-line bg-surface-muted/50 px-4 py-3"
        >
          <span
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-surface text-h2 font-semibold text-ink-soft"
          >
            {{ currentOffline.name.slice(0, 1).toUpperCase() }}
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="truncate text-h2 font-semibold text-ink">{{ currentOffline.name }}</span>
              <span
                class="shrink-0 rounded-xs bg-ink/6 px-1.5 py-0.5 text-micro font-medium text-ink-muted"
              >
                使用中
              </span>
            </div>
            <p class="mt-0.5 truncate font-mono text-micro text-ink-muted">
              UUID {{ currentOffline.uuid.slice(0, 8) }}…
            </p>
          </div>
        </div>

        <!-- 其它档案：一个整体列表，行间用极淡的分隔线，不再一个大框套小框 -->
        <div v-if="otherAccounts.length" class="flex flex-col gap-2">
          <h3 class="px-1 text-caption font-medium text-ink-muted">切换其他档案</h3>
          <div
            class="flex flex-col divide-y divide-line/50 overflow-hidden rounded-lg border border-line bg-surface"
          >
            <div
              v-for="profile in otherAccounts"
              :key="profile.id"
              class="group flex items-center justify-between px-4 py-2.5 transition-fast hover:bg-surface-muted"
            >
              <button
                type="button"
                class="flex min-w-0 flex-1 items-center gap-3 text-left disabled:cursor-not-allowed"
                :disabled="!!busy"
                @click="switchOffline(profile.id)"
              >
                <span
                  class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-surface-muted text-body font-semibold text-ink-soft"
                >
                  {{ profile.name.slice(0, 1).toUpperCase() }}
                </span>
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-body font-medium text-ink-soft">
                    {{ profile.name }}
                  </span>
                  <span class="block truncate font-mono text-micro text-ink-muted">
                    {{ profile.uuid.slice(0, 8) }}…
                  </span>
                </span>
              </button>
              <!-- 删除按钮平时不出现，悬停该行才浮现 -->
              <button
                type="button"
                class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-muted opacity-0 transition-all hover:bg-danger-soft hover:text-danger group-hover:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger/35"
                :title="`删除 ${profile.name}`"
                :disabled="!!busy"
                @click="removeOffline(profile.id, profile.name)"
              >
                <Trash2 :size="13" />
              </button>
            </div>
          </div>
        </div>

        <!-- 新建离线档案 -->
        <div class="flex flex-col gap-2 rounded-lg border border-dashed border-line px-4 py-3">
          <div class="flex items-center gap-1.5 text-body font-semibold text-ink">
            <Plus :size="14" class="text-ink-muted" />
            新建离线档案
          </div>
          <div class="flex gap-2">
            <AppInput
              :model-value="newName"
              class="flex-1"
              placeholder="3–16 位字母、数字或下划线（不支持中文）"
              :invalid="newName.length > 0 && !nameValid"
              @update:model-value="onNameInput"
              @submit="createOffline"
            />
            <AppButton
              variant="primary"
              :disabled="!nameValid || !!busy"
              :loading="busy === 'add'"
              @click="createOffline"
            >
              创建
            </AppButton>
          </div>
          <p class="text-micro leading-5 text-ink-muted">
            档案创建时会固化一个
            UUID。切换档案会切换到对应的玩家数据；想保留存档里的物品，请切回原来的档案。
          </p>
        </div>
      </div>

      <!-- ================= 微软正版 ================= -->
      <div v-else-if="activeTab === 'microsoft'" class="flex flex-col gap-4">
        <p v-if="ephemeral" class="rounded-md bg-warning-soft px-3 py-2 text-caption text-warning">
          系统凭据管理器不可用，登录令牌只在本次运行内有效，重启后需要重新登录。
        </p>

        <!-- 正在使用微软账户 -->
        <div v-if="isMicrosoft" class="flex flex-col items-center gap-3 py-2 text-center">
          <div
            class="flex h-14 w-14 items-center justify-center rounded-full bg-success-soft text-success"
          >
            <BadgeCheck :size="26" />
          </div>
          <div>
            <h3 class="text-h2 font-semibold text-ink">
              {{ microsoftProfile?.name ?? state?.name }}
            </h3>
            <p class="mt-1 text-caption text-success">正版账户已登录</p>
          </div>
          <AppButton variant="secondary" size="sm" :disabled="!!busy" @click="logout">
            <LogOut :size="13" />
            退出登录
          </AppButton>
        </div>

        <!-- 已登录，但当前使用的是离线档案 -->
        <div v-else-if="microsoftProfile" class="flex flex-col items-center gap-3 py-2 text-center">
          <div
            class="flex h-14 w-14 items-center justify-center rounded-full bg-surface-muted text-ink-soft"
          >
            <BadgeCheck :size="26" />
          </div>
          <div>
            <h3 class="text-h2 font-semibold text-ink">{{ microsoftProfile.name }}</h3>
            <p class="mt-1 text-caption text-ink-muted">微软正版 · 登录状态已保存</p>
          </div>
          <AppButton variant="primary" size="sm" :disabled="!!busy" @click="switchMicrosoft">
            切换到微软正版
          </AppButton>
        </div>

        <!-- 尚未登录 -->
        <template v-else>
          <p
            v-if="!canLogin"
            class="rounded-md bg-warning-soft px-3 py-2 text-caption text-warning"
          >
            微软正版登录正在申请权限，暂未开放。开放后即可直接用 Microsoft
            账号登录，无需任何额外配置。
          </p>

          <div
            v-else-if="deviceCode"
            class="flex flex-col items-center gap-2 rounded-lg border border-line bg-surface-muted/50 px-3 py-4"
          >
            <p class="text-caption text-ink-muted">浏览器已打开，输入下面的代码即可完成授权</p>
            <div class="flex items-center gap-2">
              <span class="font-mono text-[22px] font-semibold tracking-[0.14em] text-ink">
                {{ deviceCode }}
              </span>
              <button
                type="button"
                class="flex h-7 w-7 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-ink/6 hover:text-ink"
                :title="copied ? '已复制' : '复制代码'"
                @click="copyCode"
              >
                <Check v-if="copied" :size="14" class="text-success" />
                <Copy v-else :size="14" />
              </button>
              <button
                type="button"
                class="flex h-7 w-7 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-ink/6 hover:text-ink"
                title="重新打开浏览器"
                @click="openBrowser"
              >
                <ExternalLink :size="14" />
              </button>
            </div>
            <p class="text-micro" :class="autoCopied ? 'text-success' : 'text-ink-muted'">
              {{ autoCopied ? "代码已自动复制到剪贴板" : "代码未自动复制，请点上面的按钮复制" }}
            </p>
            <p class="flex items-center gap-1.5 text-micro text-ink-muted">
              <AppLoading :size="14" />
              正在等待你在浏览器中完成授权…
            </p>
          </div>

          <div v-else class="flex flex-col items-center gap-3 py-2 text-center">
            <p class="text-caption text-ink-muted">登录正版账户，即可使用官方皮肤与联机服务。</p>
            <AppButton
              variant="primary"
              :loading="busy === 'login'"
              :disabled="!!busy"
              @click="login"
            >
              使用 Microsoft 账号登录
            </AppButton>
            <p class="text-micro text-ink-muted">登录后会自动复制设备码并打开浏览器</p>
          </div>
        </template>
      </div>

      <!-- ================= 第三方登录 ================= -->
      <div v-else class="flex flex-col gap-4">
        <p v-if="ephemeral" class="rounded-md bg-warning-soft px-3 py-2 text-caption text-warning">
          系统凭据管理器不可用，登录令牌只在本次运行内有效，重启后需要重新登录。
        </p>

        <!-- 正在使用第三方账户 -->
        <div
          v-if="currentAuthlib"
          class="flex items-center gap-3 rounded-lg border border-line bg-surface-muted/50 px-4 py-3"
        >
          <span
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-surface text-h2 font-semibold text-ink-soft"
          >
            {{ currentAuthlib.name.slice(0, 1).toUpperCase() }}
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="truncate text-h2 font-semibold text-ink">
                {{ currentAuthlib.name }}
              </span>
              <span
                class="shrink-0 rounded-xs bg-ink/6 px-1.5 py-0.5 text-micro font-medium text-ink-muted"
              >
                使用中
              </span>
            </div>
            <p class="mt-0.5 truncate font-mono text-micro text-ink-muted">
              {{ currentAuthlib.serverName || currentAuthlib.apiRoot }}
            </p>
          </div>
          <AppButton
            variant="secondary"
            size="sm"
            :disabled="!!busy"
            @click="removeAuthlib(currentAuthlib.id, currentAuthlib.name)"
          >
            退出
          </AppButton>
        </div>

        <!-- 已登录但当前使用的是别的账户 -->
        <div v-if="otherAuthlibProfiles.length" class="flex flex-col gap-2">
          <h3 class="px-1 text-caption font-medium text-ink-muted">切换其他第三方账户</h3>
          <div
            class="flex flex-col divide-y divide-line/50 overflow-hidden rounded-lg border border-line bg-surface"
          >
            <div
              v-for="profile in otherAuthlibProfiles"
              :key="profile.id"
              class="group flex items-center justify-between px-4 py-2.5 transition-fast hover:bg-surface-muted"
            >
              <button
                type="button"
                class="flex min-w-0 flex-1 items-center gap-3 text-left disabled:cursor-not-allowed"
                :disabled="!!busy"
                @click="switchAuthlib(profile.id)"
              >
                <span
                  class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-surface-muted text-body font-semibold text-ink-soft"
                >
                  {{ profile.name.slice(0, 1).toUpperCase() }}
                </span>
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-body font-medium text-ink-soft">
                    {{ profile.name }}
                  </span>
                  <span class="block truncate font-mono text-micro text-ink-muted">
                    {{ profile.serverName || profile.apiRoot }}
                  </span>
                </span>
              </button>
              <button
                type="button"
                class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-muted opacity-0 transition-all hover:bg-danger-soft hover:text-danger group-hover:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger/35"
                :title="`退出 ${profile.name}`"
                :disabled="!!busy"
                @click="removeAuthlib(profile.id, profile.name)"
              >
                <Trash2 :size="13" />
              </button>
            </div>
          </div>
        </div>

        <!-- 登录表单 -->
        <div class="flex flex-col gap-2.5 rounded-lg border border-dashed border-line px-4 py-3">
          <div class="flex items-center gap-1.5 text-body font-semibold text-ink">
            <ShieldCheck :size="14" class="text-ink-muted" />
            使用第三方皮肤站账号登录
          </div>

          <div class="flex gap-1.5">
            <button
              v-for="preset in AUTHLIB_PRESETS"
              :key="preset.apiRoot"
              type="button"
              class="rounded-md border border-line px-2 py-1 text-micro font-medium text-ink-muted transition-colors hover:border-brand-500 hover:text-ink-soft"
              :disabled="!!busy"
              @click="applyAuthlibPreset(preset.apiRoot, preset.label)"
            >
              {{ preset.label }}
            </button>
          </div>

          <div class="flex flex-col gap-1.5">
            <span class="text-caption text-ink-muted">Yggdrasil 服务器地址</span>
            <AppInput
              v-model="authlibForm.apiRoot"
              placeholder="https://littleskin.cn/api/yggdrasil"
              :disabled="!!busy"
            />
          </div>

          <div class="flex gap-2">
            <div class="flex flex-1 flex-col gap-1.5">
              <span class="text-caption text-ink-muted">账号</span>
              <AppInput
                v-model="authlibForm.username"
                placeholder="皮肤站用户名或邮箱"
                :disabled="!!busy"
              />
            </div>
            <div class="flex flex-1 flex-col gap-1.5">
              <span class="text-caption text-ink-muted">密码</span>
              <AppInput
                v-model="authlibForm.password"
                type="password"
                placeholder="皮肤站密码"
                :disabled="!!busy"
                @submit="loginAuthlib"
              />
            </div>
          </div>

          <AppButton
            variant="primary"
            :loading="busy === 'authlib-login'"
            :disabled="
              !!busy ||
              !authlibForm.apiRoot.trim() ||
              !authlibForm.username.trim() ||
              !authlibForm.password
            "
            @click="loginAuthlib"
          >
            登录
          </AppButton>

          <p class="text-micro leading-5 text-ink-muted">
            密码只用于换取登录令牌，不会保存。令牌存在系统凭据管理器里， 重启后仍可自动续期。
          </p>
        </div>
      </div>

      <p
        v-if="error"
        class="mt-4 text-caption"
        :class="apiPending ? 'rounded-md bg-warning-soft px-3 py-2 text-warning' : 'text-danger'"
      >
        {{ error }}
      </p>
    </template>
  </AppModal>
</template>
