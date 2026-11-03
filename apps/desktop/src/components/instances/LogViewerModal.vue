<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { AlertTriangle, Info, RefreshCw, ScrollText } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import { getInstanceLogs } from "@/api/instance";
import { log } from "@/api/settings";
import type { InstanceLogs, InstanceMetadata } from "@/types";

const props = withDefaults(
  defineProps<{ instance: InstanceMetadata; running: boolean; open?: boolean }>(),
  { open: true },
);
const emit = defineEmits<{ (e: "close"): void }>();

const logs = ref<InstanceLogs | null>(null);
const loading = ref(true);
const error = ref("");
const activeCrash = ref("");

const content = computed(() => {
  if (!logs.value) return "";
  if (activeCrash.value) {
    return logs.value.crashes.find((crash) => crash.name === activeCrash.value)?.content ?? "";
  }
  return logs.value.latestLog ?? "还没有 latest.log，先启动一次游戏。";
});

/** 自动诊断结论：有崩溃时直接给出「哪里坏了 + 怎么办」。 */
const findings = computed(() => logs.value?.analysis.findings ?? []);
/** 从日志里摘出的关键报错行，免去用户在几万行里翻找。 */
const excerpt = computed(() => logs.value?.analysis.excerpt ?? []);

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    logs.value = await getInstanceLogs(props.instance.id);
  } catch (err) {
    error.value = `读取日志失败：${String(err)}`;
    void log.error(error.value, { component: "LogViewerModal", action: "read" });
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void load();
});
</script>

<template>
  <AppModal width="lg" height="fixed" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
      >
        <ScrollText :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-[16px] font-semibold leading-5 text-ink">日志与崩溃</h2>
        <p class="truncate text-[11px] text-ink-muted">
          {{ instance.displayName }} ·
          {{ running ? "游戏运行中，日志会继续写入" : "读取最近一次运行" }}
        </p>
      </div>
      <AppButton variant="secondary" size="sm" :loading="loading" @click="load">
        <RefreshCw v-if="!loading" :size="14" />
        刷新
      </AppButton>
    </template>

    <div class="flex h-full min-h-0 flex-col gap-2">
      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="h-7 rounded-md px-2.5 text-[12px] font-medium transition-colors"
          :class="
            activeCrash === ''
              ? 'bg-brand-500 text-white'
              : 'bg-ink/5 text-ink-soft hover:bg-ink/10'
          "
          @click="activeCrash = ''"
        >
          最新日志
        </button>
        <select
          v-if="logs?.crashes.length"
          v-model="activeCrash"
          class="h-7 min-w-[180px] rounded-md border border-line/70 bg-surface px-2 text-[12px] text-ink outline-none focus:border-brand-500"
        >
          <option value="">选择崩溃报告</option>
          <option v-for="crash in logs.crashes" :key="crash.name" :value="crash.name">
            {{ crash.name }}
          </option>
        </select>
        <span v-if="logs?.crashes.length" class="text-[11px] text-ink-muted">
          {{ logs.crashes.length }} 份崩溃报告
        </span>
      </div>

      <div v-if="loading" class="flex h-64 items-center justify-center">
        <AppLoading :size="56" label="正在读取日志…" />
      </div>
      <p v-else-if="error" class="text-[12px] text-danger">{{ error }}</p>
      <template v-else>
        <!-- 自动诊断：把堆栈翻译成「哪里坏了 + 下一步」 -->
        <div
          v-if="findings.length || excerpt.length"
          class="max-h-44 shrink-0 overflow-y-auto rounded-lg border border-line/50 bg-surface-muted/35 px-3 py-2.5"
        >
          <div class="mb-1.5 text-[11px] font-medium text-ink-muted">自动诊断</div>
          <div class="flex flex-col gap-2">
            <div
              v-for="(finding, index) in findings"
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
                <div class="text-[12px] font-medium leading-4 text-ink">{{ finding.title }}</div>
                <div class="mt-0.5 text-[11px] leading-4 text-ink-muted">{{ finding.detail }}</div>
                <div class="mt-0.5 text-[11px] leading-4 text-brand-600">
                  {{ finding.suggestion }}
                </div>
              </div>
            </div>
          </div>

          <div v-if="excerpt.length" class="mt-2 border-t border-line/40 pt-2">
            <div class="mb-1 text-[11px] font-medium text-ink-muted">关键报错</div>
            <div
              v-for="(line, index) in excerpt"
              :key="index"
              class="break-all font-mono text-[11px] leading-4 text-ink-soft"
            >
              {{ line }}
            </div>
          </div>
        </div>

        <pre
          class="min-h-0 flex-1 overflow-auto rounded-lg border border-line/50 bg-surface-muted/40 p-3 font-mono text-[11px] leading-5 whitespace-pre-wrap text-ink-soft"
        >{{ content }}</pre>
      </template>
    </div>
  </AppModal>
</template>
