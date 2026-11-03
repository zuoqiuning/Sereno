<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  AlertTriangle,
  CheckCircle2,
  HeartPulse,
  Info,
  RefreshCw,
  XCircle,
} from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import { checkInstanceHealth } from "@/api/health";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import type { HealthCheck, HealthReport, InstanceMetadata } from "@/types";

/**
 * 实例健康检查：把「启动前会出什么问题」一次性列清楚。
 *
 * 每条结论都带一个动作入口（修复 / 选 Java / 打 Mod 目录 / 看日志），
 * 真正的执行交给实例页已有的流程，弹窗本身不重复实现修复逻辑。
 */
const props = withDefaults(
  defineProps<{ instance: InstanceMetadata; running: boolean; open?: boolean }>(),
  { open: true },
);
const emit = defineEmits<{
  (e: "close"): void;
  (e: "action", kind: string): void;
}>();

const report = ref<HealthReport | null>(null);
const loading = ref(false);
const error = ref("");

const counts = computed(() => {
  const checks = report.value?.checks ?? [];
  return {
    error: checks.filter((check) => check.status === "error").length,
    warn: checks.filter((check) => check.status === "warn").length,
    ok: checks.filter((check) => check.status === "ok").length,
  };
});

const modIssues = computed(() => report.value?.mods ?? null);

function toMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

function statusIcon(check: HealthCheck) {
  if (check.status === "error") return XCircle;
  if (check.status === "warn") return AlertTriangle;
  return CheckCircle2;
}

function statusClass(status: string): string {
  if (status === "error") return "text-danger";
  if (status === "warn") return "text-warning";
  return "text-success";
}

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    report.value = await checkInstanceHealth(props.instance.id);
  } catch (err) {
    error.value = `健康检查失败：${toMessage(err)}`;
    void log.error(error.value, {
      component: "HealthModal",
      instanceId: props.instance.id,
    });
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

function runAction(kind: string): void {
  emit("action", kind);
}
</script>

<template>
  <AppModal width="md" height="fixed" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
      >
        <HeartPulse :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-[16px] font-semibold leading-5 text-ink">健康检查</h2>
        <p class="truncate text-[11px] text-ink-muted">
          {{ instance.displayName }} · Minecraft {{ instance.gameVersion }}
          <span v-if="running">· 游戏运行中</span>
        </p>
      </div>
      <AppButton variant="secondary" size="sm" :loading="loading" @click="load">
        <RefreshCw v-if="!loading" :size="14" />
        重新检查
      </AppButton>
    </template>

    <div v-if="loading" class="flex h-64 items-center justify-center">
      <AppLoading :size="56" label="正在检查实例…" />
    </div>
    <p v-else-if="error" class="text-[12px] text-danger">{{ error }}</p>

    <div v-else-if="report" class="flex flex-col gap-4">
      <div class="flex items-center gap-4 text-[12px]">
        <span class="flex items-center gap-1.5 text-success">
          <CheckCircle2 :size="14" />{{ counts.ok }} 项正常
        </span>
        <span class="flex items-center gap-1.5 text-warning">
          <AlertTriangle :size="14" />{{ counts.warn }} 项警告
        </span>
        <span class="flex items-center gap-1.5 text-danger">
          <XCircle :size="14" />{{ counts.error }} 项失败
        </span>
      </div>

      <div class="flex flex-col gap-2">
        <div
          v-for="check in report.checks"
          :key="check.id"
          class="flex items-start gap-2.5 rounded-lg border border-line/50 bg-surface/50 px-3 py-2.5"
        >
          <component
            :is="statusIcon(check)"
            :size="15"
            class="mt-0.5 shrink-0"
            :class="statusClass(check.status)"
          />
          <div class="min-w-0 flex-1">
            <div class="text-[13px] font-medium leading-4 text-ink">{{ check.title }}</div>
            <div class="mt-1 text-[11px] leading-4 text-ink-muted">{{ check.detail }}</div>
          </div>
          <AppButton
            v-if="check.action"
            variant="secondary"
            size="sm"
            @click="runAction(check.action.kind)"
          >
            {{ check.action.label }}
          </AppButton>
        </div>
      </div>

      <!-- 依赖图：只在有问题时展开，正常时保持安静 -->
      <div
        v-if="
          modIssues &&
          (modIssues.duplicates.length ||
            modIssues.missing.length ||
            modIssues.mismatched.length)
        "
        class="rounded-lg border border-line/50 bg-surface/50 px-3 py-2.5"
      >
        <div class="mb-2 text-[12px] font-medium text-ink">Mod 依赖图</div>

        <div v-if="modIssues.missing.length" class="mb-2 flex flex-col gap-1">
          <div class="flex items-center gap-1.5 text-[11px] font-medium text-danger">
            <XCircle :size="13" />缺失前置
          </div>
          <div
            v-for="issue in modIssues.missing"
            :key="`missing-${issue.modId}`"
            class="flex items-start justify-between gap-2 text-[11px] leading-4 text-ink-soft"
          >
            <span class="min-w-0">
              <span class="font-medium text-ink">{{ issue.modId }}</span>
              <span class="text-ink-muted"> ← {{ issue.requiredBy.join("、") }}</span>
              <span v-if="issue.requirement" class="text-ink-muted">
                （要求 {{ issue.requirement }}）
              </span>
            </span>
            <button
              type="button"
              class="shrink-0 text-[11px] text-brand-600 hover:underline"
              @click="runAction(`download:${issue.modId}`)"
            >
              去下载
            </button>
          </div>
        </div>

        <div v-if="modIssues.mismatched.length" class="mb-2 flex flex-col gap-1">
          <div class="flex items-center gap-1.5 text-[11px] font-medium text-warning">
            <AlertTriangle :size="13" />版本不满足
          </div>
          <div
            v-for="issue in modIssues.mismatched"
            :key="`mismatch-${issue.modId}`"
            class="flex items-start justify-between gap-2 text-[11px] leading-4 text-ink-soft"
          >
            <span class="min-w-0">
              <span class="font-medium text-ink">{{ issue.modId }}</span>
              <span class="text-ink-muted">
                已装 {{ issue.installedVersion }}，要求 {{ issue.requirement }}（{{
                  issue.requiredBy.join("、")
                }}）
              </span>
            </span>
            <button
              type="button"
              class="shrink-0 text-[11px] text-brand-600 hover:underline"
              @click="runAction(`download:${issue.modId}`)"
            >
              去下载
            </button>
          </div>
        </div>

        <div v-if="modIssues.duplicates.length" class="flex flex-col gap-1">
          <div class="flex items-center gap-1.5 text-[11px] font-medium text-danger">
            <XCircle :size="13" />重复安装
          </div>
          <div
            v-for="duplicate in modIssues.duplicates"
            :key="`duplicate-${duplicate.modId}`"
            class="text-[11px] leading-4 text-ink-soft"
          >
            <span class="font-medium text-ink">{{ duplicate.name }}</span>
            <span class="text-ink-muted">：{{ duplicate.files.join("、") }}</span>
          </div>
        </div>
      </div>

      <!-- 崩溃分析：按「哪里坏了 + 怎么办」展示 -->
      <div
        v-if="report.crash.findings.length"
        class="rounded-lg border border-line/50 bg-surface/50 px-3 py-2.5"
      >
        <div class="mb-2 flex items-center justify-between">
          <div class="text-[12px] font-medium text-ink">最近一次崩溃</div>
          <AppButton variant="ghost" size="sm" @click="runAction('open-logs')">
            查看完整日志
          </AppButton>
        </div>
        <div class="flex flex-col gap-2">
          <div
            v-for="(finding, index) in report.crash.findings.slice(0, 3)"
            :key="index"
            class="flex items-start gap-2"
          >
            <AlertTriangle
              v-if="finding.severity === 'error'"
              :size="14"
              class="mt-0.5 shrink-0 text-danger"
            />
            <Info v-else :size="14" class="mt-0.5 shrink-0 text-warning" />
            <div class="min-w-0">
              <div class="text-[12px] font-medium leading-4 text-ink">
                {{ finding.title }}
                <span v-if="finding.confidence !== 'high'" class="text-[10px] font-normal text-ink-muted">
                  可能有其它原因
                </span>
              </div>
              <div class="mt-0.5 text-[11px] leading-4 text-ink-muted">
                {{ finding.suggestion }}
              </div>
            </div>
            <AppButton
              v-if="finding.actionKind"
              variant="secondary"
              size="sm"
              @click="runAction(finding.actionKind!)"
            >
              {{ finding.actionLabel }}
            </AppButton>
          </div>
        </div>
      </div>
    </div>
  </AppModal>
</template>
