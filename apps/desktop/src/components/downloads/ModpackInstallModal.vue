<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Package } from "@lucide/vue";
import { useRouter } from "vue-router";

import AppButton from "@/components/common/AppButton.vue";
import AppInput from "@/components/common/AppInput.vue";
import AppLoading from "@/components/common/AppLoading.vue";
import AppModal from "@/components/common/AppModal.vue";
import { createInstance } from "@/api/instance";
import { isApiError } from "@/api/invoke";
import {
  getCfModpackInfo,
  getModpackInfo,
  installCfModpack,
  installModpack,
} from "@/api/modpack";
import { log } from "@/api/settings";
import { useTaskStore } from "@/stores/task";
import type { DownloadProgress, ModProject, ModpackInfo } from "@/types";

/**
 * 整合包安装确认。
 *
 * 整合包会**新建一个实例**（游戏版本由整合包决定），因此这里先读一次
 * 整合包索引，让用户看清版本、加载器与文件数，再开始下载。
 */
const props = withDefaults(
  defineProps<{
    item: ModProject;
    versionId: string;
    versionTitle: string;
    open?: boolean;
  }>(),
  { open: true },
);

const emit = defineEmits<{ (e: "close"): void }>();

const router = useRouter();
const taskStore = useTaskStore();

const info = ref<ModpackInfo | null>(null);
const instanceName = ref(props.item.title);
const loading = ref(true);
const installing = ref(false);
const error = ref("");

const LOADER_LABELS: Record<string, string> = {
  fabric: "Fabric",
  neoforge: "NeoForge",
  forge: "Forge",
  quilt: "Quilt",
};

function toErrorMessage(value: unknown): string {
  if (isApiError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}

/**
 * CurseForge 的整合包以「文件」形式分发，`versionId` 形如 `cf:4612979`。
 *
 * Modrinth 用的是版本 ID（纯字符串），两者语义不同，用前缀区分。
 */
function cfFileId(): number | null {
  if (props.item.source !== "curseforge") return null;
  const raw = props.versionId.startsWith("cf:") ? props.versionId.slice(3) : props.versionId;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    const fileId = cfFileId();
    info.value =
      fileId !== null
        ? await getCfModpackInfo(Number(props.item.id), fileId)
        : await getModpackInfo(props.item.id, props.versionId);
    if (info.value.name) instanceName.value = info.value.name;
  } catch (err) {
    error.value = `读取整合包信息失败：${toErrorMessage(err)}`;
  } finally {
    loading.value = false;
  }
}

async function install(): Promise<void> {
  if (installing.value || !info.value) return;
  const name = instanceName.value.trim();
  if (!name) {
    error.value = "实例名称不能为空";
    return;
  }

  installing.value = true;
  error.value = "";
  let instanceId = "";

  try {
    const meta = await createInstance(info.value.minecraftVersion, name);
    instanceId = meta.id;

    taskStore.start(instanceId, name);
    emit("close");
    router.push("/tasks");

    const fileId = cfFileId();
    const onProgress = (progress: DownloadProgress): void => {
      if (progress.kind === "session") {
        taskStore.updateProgress(instanceId, progress.summary, null);
      } else {
        taskStore.updateProgress(instanceId, null, progress.snapshot);
      }
    };

    if (fileId !== null) {
      await installCfModpack(instanceId, Number(props.item.id), fileId, onProgress);
    } else {
      await installModpack(instanceId, props.item.id, props.versionId, onProgress);
    }

    taskStore.finish(instanceId);
  } catch (err) {
    const message = `整合包安装失败：${toErrorMessage(err)}`;
    if (instanceId) {
      taskStore.setError(instanceId, message);
    } else {
      error.value = message;
    }
    void log.error(message, { component: "ModpackInstallModal", action: "install" });
  } finally {
    installing.value = false;
  }
}

onMounted(() => {
  void load();
});
</script>

<template>
  <AppModal width="sm" :open="open" @close="emit('close')">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-muted text-ink-soft"
      >
        <Package :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-h2 font-semibold leading-5 text-ink">安装整合包</h2>
        <p class="truncate text-micro text-ink-muted">{{ versionTitle }}</p>
      </div>
    </template>

    <div v-if="loading" class="flex h-40 items-center justify-center">
      <AppLoading :size="56" label="正在读取整合包…" />
    </div>

    <div v-else-if="info" class="flex flex-col gap-4">
      <dl class="flex flex-col gap-2 text-body">
        <div class="flex items-center justify-between">
          <dt class="text-ink-muted">Minecraft</dt>
          <dd class="font-mono text-ink">{{ info.minecraftVersion || "未声明" }}</dd>
        </div>
        <div class="flex items-center justify-between">
          <dt class="text-ink-muted">加载器</dt>
          <dd class="text-ink">
            {{ info.loader ? (LOADER_LABELS[info.loader] ?? info.loader) : "原版" }}
            <span v-if="info.loaderVersion" class="font-mono text-ink-muted">
              {{ info.loaderVersion }}
            </span>
          </dd>
        </div>
        <div class="flex items-center justify-between">
          <dt class="text-ink-muted">附加文件</dt>
          <dd class="text-ink">{{ info.fileCount }} 个</dd>
        </div>
      </dl>

      <div class="flex flex-col gap-1.5">
        <span class="text-body font-medium text-ink">实例名称</span>
        <AppInput v-model="instanceName" placeholder="整合包实例名称" />
        <p class="text-micro text-ink-muted">将新建一个独立实例，不会影响已有实例。</p>
      </div>
    </div>

    <p v-if="error" class="mt-3 text-caption text-danger">{{ error }}</p>

    <template #footer>
      <div class="flex justify-end gap-2">
        <AppButton variant="secondary" :disabled="installing" @click="emit('close')"
          >取消</AppButton
        >
        <AppButton
          variant="primary"
          :loading="installing"
          :disabled="loading || !info"
          @click="install"
        >
          开始安装
        </AppButton>
      </div>
    </template>
  </AppModal>
</template>
