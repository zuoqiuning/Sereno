<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { FolderOpen, Import, Package } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppModal from "@/components/common/AppModal.vue";
import AppSelect from "@/components/common/AppSelect.vue";
import { importInstance, scanImportCandidates, selectImportDirectory } from "@/api/instance";
import { isApiError } from "@/api/invoke";
import { log } from "@/api/settings";
import { useToastStore } from "@/stores/toast";
import type { ImportCandidate, ImportScan, InstanceMetadata } from "@/types";

/**
 * 从外部启动器导入实例。
 *
 * **导入的是「用户数据」而不是「游戏本体」**：Mod、配置与存档搬过来，
 * 游戏本体与库文件由 Sereno 自己的下载器重新拉一遍——
 * 更快、更准，还能用上镜像与 SHA-1 校验。
 */
const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "imported", instance: InstanceMetadata): void;
}>();

const toast = useToastStore();

const sourcePath = ref("");
const scan = ref<ImportScan | null>(null);
const selected = ref("");
const displayName = ref("");
const scanning = ref(false);
const importing = ref(false);
const error = ref("");

const candidates = computed<ImportCandidate[]>(() => scan.value?.candidates ?? []);

const candidateOptions = computed(() =>
  candidates.value.map((item) => ({
    value: item.label,
    label: candidateLabel(item),
  })),
);

const selectedCandidate = computed(
  () => candidates.value.find((item) => item.label === selected.value) ?? candidates.value[0],
);

const canImport = computed(
  () =>
    !!sourcePath.value &&
    !!selectedCandidate.value &&
    !!selectedCandidate.value.gameVersion &&
    !!displayName.value.trim() &&
    !importing.value,
);

function candidateLabel(item: ImportCandidate): string {
  const loader =
    item.loader.type === "vanilla"
      ? "原版"
      : `${item.loader.type} ${item.loader.version ?? ""}`.trim();
  const version = item.gameVersion || "未知版本";
  return `${item.label} · ${version} · ${loader}`;
}

function toErrorMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

async function pickDirectory(): Promise<void> {
  try {
    const path = await selectImportDirectory();
    if (!path) return;
    sourcePath.value = path;
    await rescan();
  } catch (err) {
    error.value = `选择目录失败：${toErrorMessage(err)}`;
  }
}

async function rescan(): Promise<void> {
  if (!sourcePath.value) {
    scan.value = null;
    return;
  }
  scanning.value = true;
  error.value = "";
  try {
    const result = await scanImportCandidates(sourcePath.value);
    scan.value = result;
    selected.value = result.candidates[0]?.label ?? "";
    if (!displayName.value) {
      displayName.value = result.candidates[0]?.label ?? "导入的实例";
    }
  } catch (err) {
    scan.value = null;
    error.value = `扫描目录失败：${toErrorMessage(err)}`;
  } finally {
    scanning.value = false;
  }
}

async function doImport(): Promise<void> {
  const candidate = selectedCandidate.value;
  if (!candidate || !canImport.value) return;

  importing.value = true;
  error.value = "";
  try {
    const metadata = await importInstance(
      sourcePath.value,
      candidate.label,
      candidate.gameVersion,
      candidate.loader.type,
      candidate.loader.version ?? null,
      displayName.value.trim(),
    );

    toast.success(
      "实例已导入",
      `${metadata.displayName} · 已搬运 Mod 与配置，接下来会自动补齐游戏文件`,
    );
    void log.info(`实例导入完成: ${metadata.id}`, {
      component: "ImportInstanceModal",
      instanceId: metadata.id,
      action: "import",
    });

    emit("imported", metadata);
    emit("close");
  } catch (err) {
    const message = toErrorMessage(err);
    error.value = `导入失败：${message}`;
    void log.error(`实例导入失败: ${message}`, {
      component: "ImportInstanceModal",
      action: "import",
    });
  } finally {
    importing.value = false;
  }
}

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    error.value = "";
    scan.value = null;
    selected.value = "";
    displayName.value = "";
    sourcePath.value = "";
  },
);
</script>

<template>
  <AppModal width="sm" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <Import :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-h2 font-semibold leading-5 text-ink">导入已有实例</h2>
        <p class="truncate text-micro text-ink-muted">从 PCL / HMCL / MultiMC 等导入</p>
      </div>
    </template>

    <div class="flex flex-col gap-4">
      <!-- 目录选择 -->
      <div class="flex flex-col gap-1.5">
        <span class="text-caption font-medium text-ink">实例目录</span>
        <div class="flex gap-2">
          <AppInput
            v-model="sourcePath"
            placeholder="选择包含游戏文件的目录"
            class="flex-1"
            @keyup.enter="rescan"
          />
          <AppButton variant="secondary" :disabled="scanning" @click="pickDirectory">
            <FolderOpen :size="14" />
            浏览
          </AppButton>
        </div>
        <p class="text-micro text-ink-muted">
          支持官方启动器 / PCL / HMCL（含 <code>versions/</code>）与
          MultiMC / Prism（含 <code>.minecraft/</code>）。
        </p>
      </div>

      <!-- 扫描结果 -->
      <div v-if="scanning" class="py-2 text-body text-ink-muted">正在扫描目录…</div>

      <div v-else-if="scan" class="flex flex-col gap-1.5">
        <span class="text-caption font-medium text-ink">找到的版本</span>
        <AppSelect
          v-model="selected"
          :options="candidateOptions"
          placeholder="没有识别到版本"
        />
        <p v-if="candidates.length === 0" class="text-micro text-ink-muted">
          没有识别出版本信息，你仍然可以手动指定版本后导入存档与配置。
        </p>
        <p v-else class="text-micro text-ink-muted">
          <template v-if="selectedCandidate">
            Mod {{ selectedCandidate.modCount }} 个 · 存档 {{ selectedCandidate.saveCount }} 个
          </template>
        </p>
      </div>

      <!-- 实例名称 -->
      <div v-if="scan" class="flex flex-col gap-1.5">
        <span class="text-caption font-medium text-ink">实例名称</span>
        <AppInput v-model="displayName" placeholder="导入的实例名称" />
      </div>

      <!-- 说明 -->
      <div class="flex items-start gap-2.5 rounded-md bg-surface-muted px-3 py-2.5">
        <Package :size="15" class="mt-0.5 shrink-0 text-ink-muted" />
        <p class="text-micro leading-relaxed text-ink-muted">
          只搬运 <strong class="font-medium text-ink-soft">Mod、配置与存档</strong>；
          游戏本体与库文件会用 Sereno 的下载器重新获取，能走镜像也带校验。
        </p>
      </div>
    </div>

    <p v-if="error" class="mt-3 text-caption text-danger">{{ error }}</p>

    <template #footer>
      <div class="flex justify-end gap-2">
        <AppButton variant="secondary" :disabled="importing" @click="emit('close')">取消</AppButton>
        <AppButton
          variant="primary"
          :loading="importing"
          :disabled="!canImport"
          @click="doImport"
        >
          开始导入
        </AppButton>
      </div>
    </template>
  </AppModal>
</template>
