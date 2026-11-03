<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Gamepad2 } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppModal from "@/components/common/AppModal.vue";
import { selectJavaExecutable, updateInstanceOptions } from "@/api/instance";
import { getJavaCandidates } from "@/api/java";
import { isApiError, callCommand } from "@/api/invoke";
import { commands } from "@/bindings";
import { log } from "@/api/settings";
import type { InstanceMetadata, JavaCandidate } from "@/types";

/**
 * 实例级启动配置。
 *
 * 只覆盖三项真正需要“按实例区分”的：Java 运行时、最大内存、附加 JVM 参数。
 * 其余（用户名、语言等）仍由全局设置统一管理，避免同一件事在两个地方配置。
 */
const props = withDefaults(defineProps<{ instance: InstanceMetadata; open?: boolean }>(), {
  open: true,
});
const emit = defineEmits<{
  (e: "close"): void;
  (e: "saved", value: InstanceMetadata): void;
}>();

const javas = ref<JavaCandidate[]>([]);
const globalMemoryMb = ref<number | null>(null);
const javaPath = ref(props.instance.javaPath ?? "");
const memoryMb = ref(props.instance.memoryMb ?? 2048);
const jvmArgsText = ref((props.instance.jvmArgs ?? []).join("\n"));
const saving = ref(false);
const error = ref("");

/** 下拉选项：已探测到的 Java；实例里存了但已探测不到的自定义路径也保留。 */
const javaOptions = computed(() => {
  const options = javas.value.map((java) => ({
    value: java.path,
    label: `Java ${java.majorVersion} · ${java.versionString}${java.arch === "x86" ? "（32 位）" : ""}`,
  }));

  if (javaPath.value && !options.some((option) => option.value === javaPath.value)) {
    options.unshift({ value: javaPath.value, label: `自定义 · ${javaPath.value}` });
  }
  return options;
});

async function browseJava(): Promise<void> {
  try {
    const path = await selectJavaExecutable();
    if (path) javaPath.value = path;
  } catch (err) {
    error.value = `选择文件失败：${String(err)}`;
  }
}

async function save(): Promise<void> {
  if (saving.value) return;
  saving.value = true;
  error.value = "";
  try {
    const updated = await updateInstanceOptions(
      props.instance.id,
      javaPath.value.trim() || null,
      memoryMb.value,
      jvmArgsText.value.split("\n"),
    );
    void log.info(`实例启动配置已更新: ${props.instance.id}`, {
      component: "InstanceSettingsModal",
      action: "save",
    });
    emit("saved", updated);
    emit("close");
  } catch (err) {
    error.value = isApiError(err) ? err.message : `保存失败：${String(err)}`;
    void log.error(error.value, { component: "InstanceSettingsModal", action: "save" });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  try {
    javas.value = await getJavaCandidates();
  } catch (err) {
    void log.error(`扫描 Java 失败: ${String(err)}`, { component: "InstanceSettingsModal" });
  }

  try {
    const settings = await callCommand(() => commands.getSettings());
    globalMemoryMb.value = settings.java.defaultMemoryMb ?? null;
    // 没单独设置过的实例，滑杆从全局默认值起步
    if (!props.instance.memoryMb) memoryMb.value = globalMemoryMb.value ?? 2048;
  } catch {
    // 读不到全局设置就用 2048，用户仍可手动调整
  }
});
</script>

<template>
  <AppModal width="sm" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
      >
        <Gamepad2 :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-[16px] font-semibold leading-5 text-ink">实例设置</h2>
        <p class="truncate text-[11px] text-ink-muted">
          {{ instance.displayName }} · Minecraft {{ instance.gameVersion }}
        </p>
      </div>
    </template>

    <div class="flex flex-col gap-5">
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <span class="text-[13px] font-medium text-ink">Java 运行时</span>
          <button
            type="button"
            class="text-[12px] font-medium text-brand-600 transition-colors hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/35"
            @click="browseJava"
          >
            选择文件
          </button>
        </div>
        <select
          v-model="javaPath"
          class="w-full rounded-md border border-line bg-surface px-3 py-2 text-[13px] text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/35"
        >
          <option value="">自动匹配（跟随全局设置）</option>
          <option v-for="option in javaOptions" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>
        <p class="text-[11px] text-ink-muted">留空时按游戏版本自动挑选已安装的 Java。</p>
      </div>

      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between text-[13px]">
          <span class="font-medium text-ink">最大内存</span>
          <span class="font-mono text-brand-500">{{ memoryMb }} MB</span>
        </div>
        <input
          v-model.number="memoryMb"
          type="range"
          min="512"
          max="16384"
          step="512"
          class="w-full accent-brand-500"
        />
        <p class="text-[11px] text-ink-muted">传给 Java 的 -Xmx；大型整合包建议 4096 MB 以上。</p>
      </div>

      <div class="flex flex-col gap-1.5">
        <span class="text-[13px] font-medium text-ink">附加 JVM 参数</span>
        <textarea
          v-model="jvmArgsText"
          rows="4"
          spellcheck="false"
          placeholder="-XX:+UseG1GC&#10;-Dfile.encoding=UTF-8"
          class="w-full resize-none rounded-md border border-line bg-surface px-3 py-2 font-mono text-[12px] leading-5 text-ink placeholder:text-ink-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/35"
        />
        <p class="text-[11px] text-ink-muted">一行一个参数，空行会被忽略。</p>
      </div>
    </div>

    <p v-if="error" class="mt-3 text-[12px] text-danger">{{ error }}</p>

    <template #footer>
      <div class="flex justify-end gap-2">
        <AppButton variant="secondary" :disabled="saving" @click="emit('close')">取消</AppButton>
        <AppButton variant="primary" :loading="saving" @click="save">保存</AppButton>
      </div>
    </template>
  </AppModal>
</template>
