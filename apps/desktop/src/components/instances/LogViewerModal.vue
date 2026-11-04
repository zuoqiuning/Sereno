<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { AlertTriangle, Info, Play, RefreshCw, ScrollText } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import AppSelect from "@/components/common/AppSelect.vue";
import { getInstanceLogs } from "@/api/instance";
import { launchGame } from "@/api/launch";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
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
const restarting = ref(false);
const toast = useToastStore();

/**
 * 崩溃后一键重启。
 *
 * 崩溃分析弹窗是用户看完诊断后的第一个动作现场——在这里给「重启」，
 * 比让用户关掉弹窗、回到列表再点一次启动顺手得多。
 */
async function restart(): Promise<void> {
  if (restarting.value) return;
  restarting.value = true;
  try {
    await launchGame(props.instance.id);
    toast.success("已重新启动游戏", props.instance.displayName);
    emit("close");
  } catch (err) {
    toast.error("重启失败", err instanceof Error ? err.message : String(err));
  } finally {
    restarting.value = false;
  }
}

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

const crashOptions = computed(() => [
  { value: "", label: "最新日志" },
  ...(logs.value?.crashes ?? []).map((crash) => ({ value: crash.name, label: crash.name })),
]);

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
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <ScrollText :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-h2 font-semibold leading-5 text-ink">日志与崩溃</h2>
        <p class="truncate text-micro text-ink-muted">
          {{ instance.displayName }} ·
          {{ running ? "游戏运行中，日志会继续写入" : "读取最近一次运行" }}
        </p>
      </div>
      <AppButton
        v-if="!running"
        variant="soft"
        size="sm"
        :loading="restarting"
        @click="restart"
      >
        <Play v-if="!restarting" :size="14" />
        重启游戏
      </AppButton>
      <AppButton variant="secondary" size="sm" :loading="loading" @click="load">
        <RefreshCw v-if="!loading" :size="14" />
        刷新
      </AppButton>
    </template>

    <div class="flex h-full min-h-0 flex-col gap-2">
      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="h-7 rounded-md px-2.5 text-caption font-medium transition-colors"
          :class="
            activeCrash === ''
              ? 'bg-brand-500 text-white'
              : 'bg-ink/5 text-ink-soft hover:bg-ink/10'
          "
          @click="activeCrash = ''"
        >
          最新日志
        </button>
        <div v-if="logs?.crashes.length" class="w-[200px]">
          <AppSelect v-model="activeCrash" :options="crashOptions" size="sm" />
        </div>
        <span v-if="logs?.crashes.length" class="text-micro text-ink-muted">
          {{ logs.crashes.length }} 份崩溃报告
        </span>
      </div>

      <div v-if="loading" class="flex h-64 items-center justify-center">
        <AppLoading :size="56" label="正在读取日志…" />
      </div>
      <p v-else-if="error" class="text-caption text-danger">{{ error }}</p>
      <template v-else>
        <!-- 自动诊断：把堆栈翻译成「哪里坏了 + 下一步」 -->
        <div
          v-if="findings.length || excerpt.length"
          class="selectable max-h-44 shrink-0 overflow-y-auto rounded-lg border border-line/50 bg-surface-muted/35 px-3 py-2.5"
        >
          <div class="mb-1.5 text-micro font-medium text-ink-muted">自动诊断</div>
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
                <div class="text-caption font-medium leading-4 text-ink">{{ finding.title }}</div>
                <div class="mt-0.5 text-micro leading-4 text-ink-muted">{{ finding.detail }}</div>
                <div class="mt-0.5 text-micro leading-4 text-brand-600">
                  {{ finding.suggestion }}
                </div>
              </div>
            </div>
          </div>

          <div v-if="excerpt.length" class="mt-2 border-t border-line/40 pt-2">
            <div class="mb-1 text-micro font-medium text-ink-muted">关键报错</div>
            <div
              v-for="(line, index) in excerpt"
              :key="index"
              class="break-all font-mono text-micro leading-4 text-ink-soft"
            >
              {{ line }}
            </div>
          </div>
        </div>

        <pre
          class="selectable min-h-0 flex-1 overflow-auto rounded-lg border border-line/50 bg-surface-muted/40 p-3 font-mono text-micro leading-5 whitespace-pre-wrap text-ink-soft"
        >{{ content }}</pre>
      </template>
    </div>
  </AppModal>
</template>
