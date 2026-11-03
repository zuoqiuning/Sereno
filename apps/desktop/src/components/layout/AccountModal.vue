<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Check, Copy, ExternalLink, LogOut, ShieldCheck, User } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import {
  cancelMicrosoftLogin,
  completeMicrosoftLogin,
  getAccountState,
  getMicrosoftConfig,
  logoutAccount,
  openMicrosoftLogin,
  setOfflineAccount,
  startMicrosoftLogin,
} from "@/api/account";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import type { AccountState, MicrosoftConfig } from "@/types";

/**
 * 账户弹窗：微软正版登录 + 离线用户名。
 *
 * 微软登录用设备码流程：显示短码 → 用户在任意浏览器授权 →
 * 这里同时轮询结果，成功后直接切到正版账户。
 *
 * 客户端 ID 未配置时，登录按钮保持可见但给出明确提示，
 * 避免用户点了之后只看到一个 invalid_client 报错。
 */
const props = withDefaults(defineProps<{ open?: boolean }>(), { open: true });
const emit = defineEmits<{ (e: "close"): void }>();

const state = ref<AccountState | null>(null);
const config = ref<MicrosoftConfig | null>(null);
const offlineName = ref("");

const loading = ref(true);
const busy = ref(false);
const error = ref("");

const deviceCode = ref("");
const verificationUri = ref("");
const copied = ref(false);

const isMicrosoft = computed(() => state.value?.kind === "microsoft");
const canLogin = computed(() => Boolean(config.value?.configured));
/** 系统凭据管理器不可用时，令牌只能留在内存里。 */
const ephemeral = computed(() => state.value?.credentialStoreEphemeral ?? false);

function toMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    const [account, microsoft] = await Promise.all([getAccountState(), getMicrosoftConfig()]);
    state.value = account;
    config.value = microsoft;
    offlineName.value = account.kind === "offline" ? account.name : "";
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

async function login(): Promise<void> {
  if (busy.value || !canLogin.value || !config.value) return;
  busy.value = true;
  error.value = "";
  deviceCode.value = "";
  verificationUri.value = "";
  copied.value = false;

  try {
    const info = await startMicrosoftLogin();
    deviceCode.value = info.userCode;
    verificationUri.value = info.verificationUri;
    // 直接拉起浏览器，省掉「复制链接再粘贴」这一步；失败时短码仍然可见
    void openMicrosoftLogin(info.verificationUri).catch(() => {});

    // 立刻开始轮询：用户看到短码的同时，后端已经在等授权结果
    state.value = await completeMicrosoftLogin(
      info.deviceCode,
      info.interval,
      info.expiresIn,
    );
    offlineName.value = "";
  } catch (err) {
    // 关弹窗触发的取消不是错误，不应弹红字
    if (isApiError(err) && err.type === "cancelled") return;
    const message = toMessage(err);
    error.value = message;
    void log.error(`微软登录失败: ${message}`, { component: "AccountModal" });
  } finally {
    busy.value = false;
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
  // 正在轮询时先让后端停止，否则关掉弹窗后它还会继续等授权
  if (busy.value) void cancelMicrosoftLogin().catch(() => {});
  emit("close");
}

async function copyCode(): Promise<void> {
  if (!deviceCode.value) return;
  try {
    await navigator.clipboard.writeText(deviceCode.value);
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1600);
  } catch {
    // 剪贴板不可用时用户仍可手动抄写短码
  }
}

async function logout(): Promise<void> {
  busy.value = true;
  error.value = "";
  try {
    state.value = await logoutAccount();
    offlineName.value = "";
  } catch (err) {
    error.value = `退出登录失败：${toMessage(err)}`;
  } finally {
    busy.value = false;
  }
}

async function useOffline(): Promise<void> {
  const name = offlineName.value.trim();
  if (!name) {
    error.value = "离线用户名不能为空";
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    state.value = await setOfflineAccount(name);
  } catch (err) {
    error.value = `切换离线账户失败：${toMessage(err)}`;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AppModal width="sm" :open="open" @close="handleClose">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
      >
        <User :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-[16px] font-semibold leading-5 text-ink">账户</h2>
        <p class="truncate text-[11px] text-ink-muted">
          {{ isMicrosoft ? "已登录微软正版账户" : "当前使用离线账户" }}
        </p>
      </div>
    </template>

    <div v-if="loading" class="flex h-40 items-center justify-center">
      <AppLoading :size="56" label="正在读取账户…" />
    </div>

    <div v-else class="flex flex-col gap-5">
      <!-- 当前账户 -->
      <div class="flex items-center gap-3 rounded-lg border border-line/60 bg-surface/60 px-3 py-2.5">
        <div
          class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-[13px] font-semibold"
          :class="
            isMicrosoft ? 'bg-success-soft text-success' : 'bg-brand-500/12 text-brand-600'
          "
        >
          {{ (state?.name ?? "?").slice(0, 1).toUpperCase() }}
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-1.5">
            <span class="truncate text-[14px] font-semibold text-ink">{{ state?.name }}</span>
            <span
              class="shrink-0 rounded px-1.5 py-0.5 text-[10px]"
              :class="
                isMicrosoft ? 'bg-success-soft text-success' : 'bg-ink/6 text-ink-muted'
              "
            >
              {{ isMicrosoft ? "正版" : "离线" }}
            </span>
          </div>
          <div class="truncate text-[11px] text-ink-muted">
            {{ isMicrosoft ? (state?.uuid ?? "") : "离线账户，可使用本地皮肤与联机 mod" }}
          </div>
        </div>
        <AppButton
          v-if="isMicrosoft"
          variant="ghost"
          size="sm"
          :disabled="busy"
          @click="logout"
        >
          <LogOut :size="13" class="mr-1" />
          退出
        </AppButton>
      </div>

      <p
        v-if="ephemeral"
        class="rounded-md bg-warning-soft px-3 py-2 text-[12px] text-warning"
      >
        系统凭据管理器不可用，登录令牌只在本次运行内有效，重启后需要重新登录。
      </p>

      <!-- 微软登录 -->
      <div class="flex flex-col gap-2">
        <div class="flex items-center gap-1.5 text-[13px] font-medium text-ink">
          <ShieldCheck :size="14" class="text-brand-500" />
          微软正版登录
        </div>

        <p v-if="!canLogin" class="rounded-md bg-warning-soft px-3 py-2 text-[12px] text-warning">
          登录尚未开放：需要先在 Azure 门户注册应用，并把客户端 ID 填入
          <code class="font-mono">SERENO_MS_CLIENT_ID</code> 或代码中的预留常量。
        </p>

        <template v-else-if="!isMicrosoft">
          <!-- 设备码 -->
          <div
            v-if="deviceCode"
            class="flex flex-col items-center gap-2 rounded-lg border border-brand-500/25 bg-brand-500/5 px-3 py-3"
          >
            <p class="text-[12px] text-ink-muted">
              在浏览器打开
              <span class="font-medium text-brand-600">{{ verificationUri }}</span>
              并输入下面的代码
            </p>
            <div class="flex items-center gap-2">
              <span class="font-mono text-[20px] font-semibold tracking-[0.12em] text-ink">
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
                title="打开浏览器"
                @click="openBrowser"
              >
                <ExternalLink :size="14" />
              </button>
            </div>
            <p class="flex items-center gap-1.5 text-[11px] text-ink-muted">
              <AppLoading :size="14" />
              正在等待你在浏览器中完成授权…
            </p>
          </div>

          <AppButton v-else variant="secondary" :loading="busy" @click="login">
            使用 Microsoft 账号登录
          </AppButton>
          <p class="text-[11px] text-ink-muted">
            登录信息只保存在系统凭据管理器，不会写进设置文件或日志。
          </p>
        </template>
      </div>

      <div class="h-px bg-line/60" />

      <!-- 离线账户 -->
      <div class="flex flex-col gap-2">
        <div class="text-[13px] font-medium text-ink">离线账户</div>
        <div class="flex gap-2">
          <AppInput
            v-model="offlineName"
            class="flex-1"
            placeholder="3–16 位字母、数字或下划线"
            @keyup.enter="useOffline"
          />
          <AppButton
            variant="secondary"
            :disabled="busy || isMicrosoft"
            @click="useOffline"
          >
            使用离线
          </AppButton>
        </div>
        <p v-if="isMicrosoft" class="text-[11px] text-ink-muted">
          当前是正版账户；切换离线会退出微软登录。
        </p>
      </div>

      <p v-if="error" class="text-[12px] text-danger">{{ error }}</p>
    </div>
  </AppModal>
</template>
