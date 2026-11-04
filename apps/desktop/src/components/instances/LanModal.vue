<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { Copy, DoorOpen, LogIn, RadioTower, Square, Users } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppModal from "@/components/common/AppModal.vue";
import {
  getMultiplayerStatus,
  detectWorldPort,
  hostMultiplayer,
  joinMultiplayer,
  stopMultiplayer,
} from "@/api/multiplayer";
import { toMessage } from "@/utils/error";
import { log } from "@/api/settings";
import type { InstanceMetadata, MultiplayerStatus, Phase } from "@/types";

/**
 * 联机弹窗：房主开房 / 房客加入。
 *
 * 交互原则：
 * - 会话在弹窗关闭后**继续运行**（用户要回游戏里玩），
 *   只有「停止联机」或退出应用才会结束；
 * - 状态 1.5 秒轮询一次（低频状态，不值得加事件通道）；
 * - 房间码输入自动纠偏：小写转大写、I→1、O→0、按 4 位分组。
 */
const props = withDefaults(defineProps<{ instance: InstanceMetadata; open?: boolean }>(), {
  open: true,
});
const emit = defineEmits<{ close: [] }>();

type Tab = "host" | "guest";
const tab = ref<Tab>("host");

const mcPort = ref(25565);
const roomCodeInput = ref("");
const generatedCode = ref("");
const working = ref(false);
const error = ref("");
const detecting = ref(false);
const detectedInfo = ref("");
/** TUN 模式：创建虚拟网卡（需 UAC），换取游戏内自动发现世界。默认关。 */
const tunMode = ref(false);
const status = ref<MultiplayerStatus | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

/** 当前会话是否属于本弹窗打开的实例（简化：任何活跃会话都展示）。 */
const active = computed(() => status.value !== null && status.value.phase !== "stopped");

/** 徽章配色：绿=已连接，黄=进行中，红=错误，灰=未连接。 */
const phaseBadge = computed(() => {
  switch (status.value?.phase) {
    case "connected":
      return { text: "已连接", class: "bg-success/15 text-success" };
    case "starting":
    case "waitingPeer":
      return { text: "连接中", class: "bg-warning/15 text-warning" };
    case "error":
      return { text: "出错", class: "bg-danger/15 text-danger" };
    default:
      return { text: "未连接", class: "bg-surface-muted text-ink-muted" };
  }
});

/** 链路类型徽章：直连（延迟低）/ 中继（稳定）。 */
const linkBadge = computed(() => {
  switch (status.value?.linkType) {
    case "direct":
      return { text: "直连", class: "bg-success/15 text-success" };
    case "relay":
      return { text: "中继", class: "bg-brand-500/15 text-brand-600" };
    default:
      return null;
  }
});

const phaseLabel: Record<Phase, string> = {
  starting: "启动中",
  waitingPeer: "等待世界",
  connected: "已连接",
  error: "出错",
  stopped: "已停止",
};

/** 房间码输入纠偏 + 分组显示。 */
const formattedInput = computed(() => {
  const upper = roomCodeInput.value.toUpperCase().replace(/I/g, "1").replace(/O/g, "0");
  const body = upper.replace(/[^0-9A-Z]/g, "").replace(/^U\//, "");
  return (body.match(/.{1,4}/g) ?? []).join("-");
});

function onCodeInput(value: string): void {
  roomCodeInput.value = value;
}

// 切到房主标签时自动探测一次：多数情况玩家已经开着游戏
watch(tab, (value) => {
  if (value === "host" && !active.value) void autoDetect();
});

/** 轮询状态。 */
function startPolling(): void {
  stopPolling();
  pollTimer = setInterval(() => {
    void getMultiplayerStatus()
      .then((s) => {
        status.value = s;
        // 房主生成的房间码回显
        if (s.roomCode && s.role === "host") generatedCode.value = s.roomCode;
      })
      .catch(() => undefined);
  }, 1500);
}

function stopPolling(): void {
  if (pollTimer !== null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

/** 自动探测游戏世界端口（Java 进程监听端口 + MC 状态探测）。 */
async function autoDetect(): Promise<void> {
  if (detecting.value) return;
  detecting.value = true;
  try {
    const world = await detectWorldPort();
    if (world) {
      mcPort.value = world.port;
      detectedInfo.value = `已检测到世界「${world.motd.trim() || "未命名"}」（${world.version}），端口 ${world.port}`;
    } else {
      detectedInfo.value = "未检测到开放的世界——请先在游戏内「对局域网开放」，或手动输入端口";
    }
  } catch (err) {
    detectedInfo.value = `探测失败：${toMessage(err)}`;
  } finally {
    detecting.value = false;
  }
}

async function startHost(): Promise<void> {
  if (working.value) return;
  working.value = true;
  error.value = "";
  try {
    status.value = await hostMultiplayer(generatedCode.value || null, mcPort.value);
    if (status.value.roomCode) generatedCode.value = status.value.roomCode;
  } catch (err) {
    error.value = `开房失败：${toMessage(err)}`;
    void log.error(error.value, { component: "LanModal", action: "host" });
  } finally {
    working.value = false;
  }
}

async function startJoin(): Promise<void> {
  if (working.value) return;
  working.value = true;
  error.value = "";
  try {
    status.value = await joinMultiplayer(formattedInput.value, mcPort.value, tunMode.value);
  } catch (err) {
    error.value = `加入失败：${toMessage(err)}`;
    void log.error(error.value, { component: "LanModal", action: "join" });
  } finally {
    working.value = false;
  }
}

async function stop(): Promise<void> {
  working.value = true;
  error.value = "";
  try {
    await stopMultiplayer();
    status.value = await getMultiplayerStatus();
  } catch (err) {
    error.value = `停止失败：${toMessage(err)}`;
  } finally {
    working.value = false;
  }
}

async function copyText(text: string | null | undefined): Promise<void> {
  if (!text) return;
  await navigator.clipboard.writeText(text);
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      void getMultiplayerStatus()
        .then((s) => {
          status.value = s;
          if (s.roomCode && s.role === "host") generatedCode.value = s.roomCode;
        })
        .catch(() => undefined);
      startPolling();
    } else {
      stopPolling();
    }
  },
  { immediate: true },
);

onBeforeUnmount(stopPolling);
</script>

<template>
  <AppModal width="md" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <Users :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-h2 font-semibold leading-5 text-ink">联机</h2>
        <p class="truncate text-micro text-ink-muted">
          {{ instance.displayName }} · Minecraft {{ instance.gameVersion }}
        </p>
      </div>
      <span :class="phaseBadge.class" class="rounded-xs px-1.5 py-0.5 text-micro font-medium">{{ phaseBadge.text }}</span>
      <span
        v-if="linkBadge"
        :class="linkBadge.class"
        class="rounded-xs px-1.5 py-0.5 text-micro font-medium"
      >
        {{ linkBadge.text }}
      </span>
    </template>

    <!-- 已有会话：展示状态面板 -->
    <div v-if="active && status" class="flex flex-col gap-4">
      <div class="flex items-center gap-2 text-caption text-ink-soft">
        <RadioTower :size="14" />
        {{ status.role === "host" ? "当前是房主" : "当前是房客" }} ·
        {{ phaseLabel[status.phase] }}
      </div>

      <p class="text-body leading-5 text-ink">{{ status.detail }}</p>

      <div
        v-if="status.role === 'host' && status.roomCode"
        class="flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/60 px-4 py-3"
      >
        <div class="text-micro text-ink-muted">房间码（发给朋友）</div>
        <div class="flex items-center gap-2">
          <code class="flex-1 font-mono text-title font-semibold tracking-wide text-ink">
            {{ status.roomCode }}
          </code>
          <AppButton variant="secondary" size="sm" @click="copyText(status.roomCode)">
            <Copy :size="13" /> 复制
          </AppButton>
        </div>
      </div>

      <div
        v-if="status.role === 'guest' && status.localAddr"
        class="flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/60 px-4 py-3"
      >
        <div class="text-micro text-ink-muted">游戏内「直接连接」地址</div>
        <div class="flex items-center gap-2">
          <code class="flex-1 font-mono text-body font-semibold text-ink">
            {{ status.localAddr }}
          </code>
          <AppButton variant="secondary" size="sm" @click="copyText(status.localAddr)">
            <Copy :size="13" /> 复制
          </AppButton>
        </div>
      </div>

      <div
        v-if="status.lastLog.length"
        class="rounded-lg bg-surface-muted/60 px-3 py-2 font-mono text-micro leading-4 text-ink-muted"
      >
        <div v-for="(line, index) in status.lastLog" :key="index" class="truncate">
          {{ line }}
        </div>
      </div>

      <AppButton variant="secondary" :loading="working" @click="stop">
        <Square :size="13" /> 停止联机
      </AppButton>
    </div>

    <!-- 无会话：开房 / 加入 双标签 -->
    <template v-else>
      <div class="mb-4 flex gap-1 rounded-lg bg-surface-muted p-1">
        <button
          type="button"
          class="flex flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-2 text-caption font-medium transition-colors"
          :class="
            tab === 'host' ? 'bg-surface text-ink shadow-sm' : 'text-ink-muted hover:text-ink'
          "
          @click="tab = 'host'"
        >
          <DoorOpen :size="14" /> 开房间
        </button>
        <button
          type="button"
          class="flex flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-2 text-caption font-medium transition-colors"
          :class="
            tab === 'guest' ? 'bg-surface text-ink shadow-sm' : 'text-ink-muted hover:text-ink'
          "
          @click="tab = 'guest'"
        >
          <LogIn :size="14" /> 加入房间
        </button>
      </div>

      <p v-if="error" class="mb-3 text-caption text-danger">{{ error }}</p>

      <!-- 房主 -->
      <div v-if="tab === 'host'" class="flex flex-col gap-3">
        <label class="flex flex-col gap-1.5">
          <span class="text-caption text-ink-soft">游戏世界端口</span>
          <div class="flex items-center gap-2">
            <input
              v-model.number="mcPort"
              type="number"
              min="1024"
              max="65535"
              class="h-9 min-w-0 flex-1 rounded-md border border-line/80 bg-surface px-3 font-mono text-body text-ink outline-none focus:border-brand-500"
            />
            <AppButton variant="secondary" size="sm" :loading="detecting" @click="autoDetect">
              自动检测
            </AppButton>
          </div>
          <span v-if="detectedInfo" class="text-micro text-success">{{ detectedInfo }}</span>
          <span v-else class="text-micro text-ink-muted">
            专用服务端默认 25565；单人世界点「自动检测」即可，无需去聊天栏找端口
          </span>
        </label>

        <div
          v-if="generatedCode"
          class="flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/60 px-4 py-3"
        >
          <div class="text-micro text-ink-muted">你的房间码（发给朋友）</div>
          <div class="flex items-center gap-2">
            <code class="flex-1 font-mono text-title font-semibold tracking-wide text-ink">
              {{ generatedCode }}
            </code>
            <AppButton variant="secondary" size="sm" @click="copyText(generatedCode)">
              <Copy :size="13" /> 复制
            </AppButton>
          </div>
        </div>

        <AppButton variant="primary" :loading="working" @click="startHost">
          <DoorOpen :size="14" />
          {{ generatedCode ? "重新开房" : "创建房间" }}
        </AppButton>
        <p class="text-micro leading-4 text-ink-muted">
          首次开房会下载联机内核并请求一次系统授权（UAC），之后房客加入全程无需任何权限。
        </p>
      </div>

      <!-- 房客 -->
      <div v-else class="flex flex-col gap-3">
        <label class="flex flex-col gap-1.5">
          <span class="text-caption text-ink-soft">房间码</span>
          <input
            :value="formattedInput"
            type="text"
            placeholder="U/XXXX-XXXX-XXXX-XXXX"
            autocomplete="off"
            spellcheck="false"
            class="h-9 rounded-md border border-line/80 bg-surface px-3 font-mono text-body tracking-wide text-ink outline-none placeholder:text-ink-muted/50 focus:border-brand-500"
            @input="onCodeInput(($event.target as HTMLInputElement).value)"
          />
          <span class="text-micro text-ink-muted">自动纠错：小写、I/1、O/0、缺分隔符都能识别</span>
        </label>

        <label class="flex flex-col gap-1.5">
          <span class="text-caption text-ink-soft">房主的世界端口</span>
          <input
            v-model.number="mcPort"
            type="number"
            min="1024"
            max="65535"
            class="h-9 rounded-md border border-line/80 bg-surface px-3 font-mono text-body text-ink outline-none focus:border-brand-500"
          />
          <span class="text-micro text-ink-muted">房主开房时设置的端口（默认 25565）</span>
        </label>

        <label class="flex cursor-pointer items-start gap-2">
          <input
            v-model="tunMode"
            type="checkbox"
            class="mt-0.5 h-4 w-4 accent-[var(--color-brand-500)]"
          />
          <span class="flex flex-col gap-0.5">
            <span class="text-caption text-ink">虚拟网卡模式（自动发现世界）</span>
            <span class="text-micro text-ink-muted">
              在游戏多人列表直接看到房主的世界，无需复制地址。需要一次 UAC 授权；
              默认关闭（免授权，手动输入地址）。
            </span>
          </span>
        </label>

        <AppButton
          variant="primary"
          :loading="working"
          :disabled="formattedInput.length < 19"
          @click="startJoin"
        >
          <LogIn :size="14" /> 加入房间
        </AppButton>
        <p class="text-micro leading-4 text-ink-muted">
          {{
            tunMode
              ? "加入成功后，游戏内「多人游戏」列表会自动出现房主的世界，双击即可进入。"
              : "加入成功后，在游戏内「多人游戏 → 直接连接」输入弹窗给出的地址即可，无需输入 IP。"
          }}
        </p>
      </div>
    </template>
  </AppModal>
</template>
