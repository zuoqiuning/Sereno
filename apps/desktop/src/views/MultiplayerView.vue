<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  CheckCircle2,
  Copy,
  DoorOpen,
  Globe,
  LogIn,
  RadioTower,
  Square,
  Users,
  XCircle,
} from "@lucide/vue";

import AppBadge from "@/components/common/AppBadge.vue";
import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import PageShell from "@/components/layout/PageShell.vue";
import {
  checkWorldReachable,
  detectWorldPort,
  getMultiplayerStatus,
  hostMultiplayer,
  joinMultiplayer,
  stopMultiplayer,
} from "@/api/multiplayer";
import { toMessage } from "@/utils/error";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import type { MultiplayerStatus, ServerStatus } from "@/bindings";

/**
 * 联机页（顶层导航独立页面）。
 *
 * ## 设计要点
 * - 会话在离开页面后**继续运行**（用户要回游戏里玩），回来时状态自动恢复；
 * - **世界自检**：房主/房客都能一键对本机端口做 Server List Ping——
 *   能 ping 通 = 端口 + 隧道 + 世界三者就绪，解决「开了房自己没法测」的问题；
 * - 房间码输入自动纠偏：小写转大写、I→1、O→0、按 4 位分组；
 * - 状态 1.5 秒轮询（低频状态，不值得加事件通道）。
 */

const toast = useToastStore();

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

/** 世界自检状态。 */
const checking = ref(false);
const worldCheck = ref<ServerStatus | null>(null);
const worldCheckError = ref("");

let pollTimer: ReturnType<typeof setInterval> | null = null;

const active = computed(() => status.value !== null && status.value.phase !== "stopped");

const phaseText: Record<string, string> = {
  starting: "启动中",
  waitingPeer: "等待世界",
  connected: "已连接",
  error: "出错",
  stopped: "已停止",
};

/** 阶段徽章配色：绿=已连接，黄=进行中，红=错误，灰=未连接。 */
const phaseBadge = computed(() => {
  switch (status.value?.phase) {
    case "connected":
      return { text: "已连接", variant: "success" as const };
    case "starting":
    case "waitingPeer":
      return { text: "连接中", variant: "warning" as const };
    case "error":
      return { text: "出错", variant: "danger" as const };
    default:
      return { text: "未连接", variant: "default" as const };
  }
});

/** 链路类型徽章：直连（延迟低）/ 中继（稳定）。 */
const linkBadge = computed(() => {
  switch (status.value?.linkType) {
    case "direct":
      return { text: "直连", variant: "success" as const };
    case "relay":
      return { text: "中继", variant: "brand" as const };
    default:
      return null;
  }
});

/** 房间码输入纠偏 + 分组显示。 */
const formattedInput = computed(() => {
  const upper = roomCodeInput.value.toUpperCase().replace(/I/g, "1").replace(/O/g, "0");
  const body = upper.replace(/[^0-9A-Z]/g, "").replace(/^U\//, "");
  return (body.match(/.{1,4}/g) ?? []).join("-");
});

async function copyText(text: string | null | undefined, label: string): Promise<void> {
  if (!text) return;
  await navigator.clipboard.writeText(text);
  toast.success("已复制", label);
}

// ---- 状态轮询 ----

async function refreshStatus(): Promise<void> {
  try {
    const s = await getMultiplayerStatus();
    status.value = s;
    if (s.roomCode && s.role === "host") generatedCode.value = s.roomCode;
  } catch {
    // 状态拉取失败不打断页面
  }
}

function startPolling(): void {
  stopPolling();
  pollTimer = setInterval(() => void refreshStatus(), 1500);
}

function stopPolling(): void {
  if (pollTimer !== null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

// ---- 世界探测与自检 ----

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

/**
 * 世界自检：对本机端口做一次 Server List Ping。
 *
 * 房主测自己的世界（游戏开了没有、端口对不对）；
 * 房客测隧道（EasyTier 虚拟网 + 端口映射 + 世界，一路通到底才算数）。
 */
async function checkWorld(): Promise<void> {
  if (checking.value) return;
  const port = status.value?.phase === "connected" ? status.value.mcPort : mcPort.value;
  checking.value = true;
  worldCheck.value = null;
  worldCheckError.value = "";
  try {
    worldCheck.value = await checkWorldReachable(port);
  } catch (err) {
    worldCheckError.value = toMessage(err);
  } finally {
    checking.value = false;
  }
}

// ---- 会话操作 ----

async function startHost(): Promise<void> {
  if (working.value) return;
  working.value = true;
  error.value = "";
  try {
    status.value = await hostMultiplayer(generatedCode.value || null, mcPort.value);
    if (status.value.roomCode) generatedCode.value = status.value.roomCode;
  } catch (err) {
    error.value = `开房失败：${toMessage(err)}`;
    void log.error(error.value, { component: "MultiplayerView", action: "host" });
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
    void log.error(error.value, { component: "MultiplayerView", action: "join" });
  } finally {
    working.value = false;
  }
}

async function stop(): Promise<void> {
  working.value = true;
  error.value = "";
  try {
    await stopMultiplayer();
    await refreshStatus();
    worldCheck.value = null;
    worldCheckError.value = "";
  } catch (err) {
    error.value = `停止失败：${toMessage(err)}`;
  } finally {
    working.value = false;
  }
}

onMounted(() => {
  void refreshStatus();
  // 开房标签下自动探测一次：多数情况玩家已经开着游戏
  if (tab.value === "host") void autoDetect();
  startPolling();
});

onBeforeUnmount(stopPolling);
</script>

<template>
  <PageShell
    title="联机"
    :description="
      active && status
        ? `${status.role === 'host' ? '当前是房主' : '当前是房客'} · ${phaseText[status.phase] ?? status.phase}`
        : '和朋友用房间码联机：开房间 / 加入房间，无需公网 IP'
    "
    :icon="Users"
    width="md"
  >
    <template #actions>
      <div v-if="active && status" class="flex items-center gap-1.5">
        <AppBadge :variant="phaseBadge.variant">{{ phaseBadge.text }}</AppBadge>
        <AppBadge v-if="linkBadge" :variant="linkBadge.variant">{{ linkBadge.text }}</AppBadge>
      </div>
    </template>

    <p v-if="error" class="mb-3 text-caption text-danger">{{ error }}</p>

    <!-- 已有会话：状态面板 -->
    <div v-if="active && status" class="flex flex-col gap-4">
      <AppCard padding="md">
        <div class="flex items-center gap-2 text-caption text-ink-soft">
          <RadioTower :size="14" />
          {{ status.role === "host" ? "当前是房主" : "当前是房客" }} ·
          {{ phaseText[status.phase] ?? status.phase }}
        </div>

        <p class="mt-2 text-body leading-5 text-ink">{{ status.detail }}</p>

        <!-- 房主：房间码 -->
        <div
          v-if="status.role === 'host' && status.roomCode"
          class="mt-4 flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/60 px-4 py-3"
        >
          <div class="text-micro text-ink-muted">房间码（发给朋友，对方在「加入房间」里输入）</div>
          <div class="flex items-center gap-2">
            <code class="flex-1 font-mono text-title font-semibold tracking-wide text-ink">
              {{ status.roomCode }}
            </code>
            <AppButton
              variant="secondary"
              size="sm"
              @click="copyText(status.roomCode, '房间码')"
            >
              <Copy :size="13" /> 复制
            </AppButton>
          </div>
        </div>

        <!-- 房客：连接地址 -->
        <div
          v-if="status.role === 'guest' && status.localAddr"
          class="mt-4 flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/60 px-4 py-3"
        >
          <div class="text-micro text-ink-muted">游戏内「直接连接」地址</div>
          <div class="flex items-center gap-2">
            <code class="flex-1 font-mono text-body font-semibold text-ink">
              {{ status.localAddr }}
            </code>
            <AppButton
              variant="secondary"
              size="sm"
              @click="copyText(status.localAddr, '连接地址')"
            >
              <Copy :size="13" /> 复制
            </AppButton>
          </div>
        </div>

        <!-- 世界自检 -->
        <div class="mt-4 flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/40 px-4 py-3">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col">
              <span class="text-caption font-medium text-ink">世界自检</span>
              <span class="text-micro text-ink-muted">
                {{
                  status.role === "host"
                    ? "确认游戏世界已开放、端口正确——能 ping 通才联得出去"
                    : "确认隧道已通到房主的世界——能 ping 通就可以进游戏了"
                }}
              </span>
            </div>
            <AppButton variant="secondary" size="sm" :loading="checking" @click="checkWorld">
              <Globe :size="13" /> 检测
            </AppButton>
          </div>
          <div
            v-if="worldCheck"
            class="flex items-start gap-2 rounded-md bg-success/10 px-3 py-2 text-caption text-ink"
          >
            <CheckCircle2 :size="15" class="mt-0.5 shrink-0 text-success" />
            <span>
              世界在线：<span class="font-medium">{{ worldCheck.motd.trim() || "未命名世界" }}</span>
              · {{ worldCheck.version }} · 玩家 {{ worldCheck.playersOnline }}/{{
                worldCheck.playersMax
              }}
              <span class="block text-micro text-ink-muted">
                {{ status.role === "host" ? "朋友现在可以通过房间码加入了" : "游戏内「直接连接」输入上方地址即可进入" }}
              </span>
            </span>
          </div>
          <div
            v-else-if="worldCheckError"
            class="flex items-start gap-2 rounded-md bg-danger/10 px-3 py-2 text-caption text-ink"
          >
            <XCircle :size="15" class="mt-0.5 shrink-0 text-danger" />
            <span>
              世界不可达：{{ worldCheckError }}
              <span class="block text-micro text-ink-muted">
                {{
                  status.role === "host"
                    ? "请确认游戏已启动且已「对局域网开放」（或专用服务端在运行），端口填写正确"
                    : "房主可能还没开世界，稍等片刻再试；持续失败请联系房主检查"
                }}
              </span>
            </span>
          </div>
        </div>

        <!-- 内核日志 -->
        <div
          v-if="status.lastLog.length"
          class="mt-4 rounded-lg bg-surface-muted/60 px-3 py-2 font-mono text-micro leading-4 text-ink-muted"
        >
          <div v-for="(line, index) in status.lastLog" :key="index" class="truncate">
            {{ line }}
          </div>
        </div>
      </AppCard>

      <AppButton variant="secondary" :loading="working" @click="stop">
        <Square :size="13" /> 停止联机
      </AppButton>
    </div>

    <!-- 无会话：开房 / 加入 -->
    <template v-else>
      <div class="mb-4 flex gap-1 rounded-lg bg-surface-muted p-1">
        <button
          type="button"
          class="flex flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-2 text-caption font-medium transition-colors"
          :class="tab === 'host' ? 'bg-surface text-ink shadow-sm' : 'text-ink-muted hover:text-ink'"
          @click="tab = 'host'"
        >
          <DoorOpen :size="14" /> 开房间
        </button>
        <button
          type="button"
          class="flex flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-2 text-caption font-medium transition-colors"
          :class="tab === 'guest' ? 'bg-surface text-ink shadow-sm' : 'text-ink-muted hover:text-ink'"
          @click="tab = 'guest'"
        >
          <LogIn :size="14" /> 加入房间
        </button>
      </div>

      <!-- 房主 -->
      <AppCard v-if="tab === 'host'" padding="md" class="flex flex-col gap-3">
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
          <span v-if="detectedInfo" class="text-micro" :class="detectedInfo.startsWith('已检测') ? 'text-success' : 'text-ink-muted'">
            {{ detectedInfo }}
          </span>
          <span v-else class="text-micro text-ink-muted">
            专用服务端默认 25565；单人世界点「自动检测」即可，无需去聊天栏找端口
          </span>
        </label>

        <!-- 开房前就可以自检世界 -->
        <div class="flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/40 px-4 py-3">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col">
              <span class="text-caption font-medium text-ink">世界自检</span>
              <span class="text-micro text-ink-muted">开房前先确认游戏世界已开放、端口正确</span>
            </div>
            <AppButton variant="secondary" size="sm" :loading="checking" @click="checkWorld">
              <Globe :size="13" /> 检测
            </AppButton>
          </div>
          <div
            v-if="worldCheck"
            class="flex items-start gap-2 rounded-md bg-success/10 px-3 py-2 text-caption text-ink"
          >
            <CheckCircle2 :size="15" class="mt-0.5 shrink-0 text-success" />
            <span>
              世界在线：<span class="font-medium">{{ worldCheck.motd.trim() || "未命名世界" }}</span>
              · {{ worldCheck.version }} · 玩家 {{ worldCheck.playersOnline }}/{{
                worldCheck.playersMax
              }}
            </span>
          </div>
          <div
            v-else-if="worldCheckError"
            class="flex items-start gap-2 rounded-md bg-danger/10 px-3 py-2 text-caption text-ink"
          >
            <XCircle :size="15" class="mt-0.5 shrink-0 text-danger" />
            <span>
              世界不可达：{{ worldCheckError }}
              <span class="block text-micro text-ink-muted">
                请确认游戏已启动且已「对局域网开放」（或专用服务端在运行），端口填写正确
              </span>
            </span>
          </div>
        </div>

        <div
          v-if="generatedCode"
          class="flex flex-col gap-2 rounded-lg border border-line/50 bg-surface-muted/60 px-4 py-3"
        >
          <div class="text-micro text-ink-muted">你的房间码（发给朋友）</div>
          <div class="flex items-center gap-2">
            <code class="flex-1 font-mono text-title font-semibold tracking-wide text-ink">
              {{ generatedCode }}
            </code>
            <AppButton variant="secondary" size="sm" @click="copyText(generatedCode, '房间码')">
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
      </AppCard>

      <!-- 房客 -->
      <AppCard v-else padding="md" class="flex flex-col gap-3">
        <label class="flex flex-col gap-1.5">
          <span class="text-caption text-ink-soft">房间码</span>
          <input
            :value="formattedInput"
            type="text"
            placeholder="U/XXXX-XXXX-XXXX-XXXX"
            autocomplete="off"
            spellcheck="false"
            class="h-9 rounded-md border border-line/80 bg-surface px-3 font-mono text-body tracking-wide text-ink outline-none placeholder:text-ink-muted/50 focus:border-brand-500"
            @input="roomCodeInput = ($event.target as HTMLInputElement).value"
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
              : "加入成功后，在游戏内「多人游戏 → 直接连接」输入页面给出的地址即可，无需输入 IP。"
          }}
        </p>
      </AppCard>
    </template>
  </PageShell>
</template>
