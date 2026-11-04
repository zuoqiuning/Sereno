<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import {
  Boxes,
  Coffee,
  Download,
  FolderOpen,
  HardDrive,
  ListTodo,
  Moon,
  Rocket,
  Settings,
  SlidersHorizontal,
  Sun,
} from "@lucide/vue";
import type { Component } from "vue";

import AppModal from "@/components/common/AppModal.vue";
import { getSettings, saveSettings } from "@/api/settings";
import { applyTheme } from "@/utils/theme";
import { PALETTE_EVENT } from "@/composables/useKeyboardShortcuts";

/**
 * 命令面板（`Ctrl/Cmd + K`）。
 *
 * ## 为什么值得做
 *
 * 快捷键的问题在于「记不住」，命令面板的问题在于「要打开」。
 * 两者结合后，用户只需要记住一个键：按下 K，输入「内存」就能直接跳到设置项。
 * 这也是主流桌面应用（VS Code / Linear / Raycast）的通用范式。
 */
interface Command {
  id: string;
  label: string;
  hint?: string;
  keywords: string[];
  icon: Component;
  /** 返回值不关心（router.push 会返回导航结果），统一用 unknown 兼容 */
  run: () => unknown;
}

const router = useRouter();
const open = ref(false);
const query = ref("");
const activeIndex = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);
const theme = ref<string>("light");

const commands = computed<Command[]>(() => [
  {
    id: "nav-home",
    label: "前往「启动」",
    hint: "Ctrl+1",
    keywords: ["home", "qidong", "start", "launch"],
    icon: Rocket,
    run: () => router.push("/"),
  },
  {
    id: "nav-instances",
    label: "前往「实例」",
    hint: "Ctrl+2",
    keywords: ["instances", "shili", "版本"],
    icon: Boxes,
    run: () => router.push("/instances"),
  },
  {
    id: "nav-downloads",
    label: "前往「下载」",
    hint: "Ctrl+3",
    keywords: ["downloads", "xiazai", "mod", "整合包"],
    icon: Download,
    run: () => router.push("/downloads"),
  },
  {
    id: "nav-tasks",
    label: "前往「任务」",
    hint: "Ctrl+4",
    keywords: ["tasks", "renwu", "下载队列"],
    icon: ListTodo,
    run: () => router.push("/tasks"),
  },
  {
    id: "nav-settings",
    label: "前往「设置」",
    hint: "Ctrl+5",
    keywords: ["settings", "shezhi"],
    icon: Settings,
    run: () => router.push("/settings"),
  },
  {
    id: "setting-java",
    label: "设置 · Java 运行环境",
    hint: "设置",
    keywords: ["java", "jdk", "运行时", "java路径"],
    icon: Coffee,
    run: () => router.push({ path: "/settings", query: { section: "launch", focus: "java" } }),
  },
  {
    id: "setting-memory",
    label: "设置 · 内存分配",
    hint: "设置",
    keywords: ["memory", "neicun", "xmx", "内存"],
    icon: SlidersHorizontal,
    run: () => router.push({ path: "/settings", query: { section: "launch", focus: "memory" } }),
  },
  {
    id: "setting-storage",
    label: "设置 · 存储路径",
    hint: "设置",
    keywords: ["storage", "cunchu", "path", "实例目录"],
    icon: FolderOpen,
    run: () => router.push({ path: "/settings", query: { section: "general", focus: "storage" } }),
  },
  {
    id: "setting-cache",
    label: "设置 · 磁盘占用与缓存",
    hint: "设置",
    keywords: ["disk", "cache", "huancun", "磁盘"],
    icon: HardDrive,
    run: () => router.push({ path: "/settings", query: { section: "general", focus: "storage" } }),
  },
  {
    id: "action-theme",
    label: theme.value === "dark" ? "切换到浅色主题" : "切换到深色主题",
    keywords: ["theme", "zhuti", "dark", "light", "深色", "浅色"],
    icon: theme.value === "dark" ? Sun : Moon,
    run: async () => {
      const settings = await getSettings();
      const next = settings.theme === "dark" ? "light" : "dark";
      settings.theme = next;
      await saveSettings(settings);
      applyTheme(next);
      theme.value = next;
    },
  },
]);

const filtered = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  if (!keyword) return commands.value;
  return commands.value.filter((command) => {
    return (
      command.label.toLowerCase().includes(keyword) ||
      command.keywords.some((item) => item.includes(keyword))
    );
  });
});

watch(filtered, () => {
  activeIndex.value = 0;
});

async function openPalette(): Promise<void> {
  query.value = "";
  activeIndex.value = 0;
  open.value = true;
  await nextTick();
  inputRef.value?.focus();
  // 顺带刷新一次主题状态（用户可能刚在设置页改过）
  void getSettings()
    .then((settings) => {
      theme.value = settings.theme;
    })
    .catch(() => undefined);
}

function close(): void {
  open.value = false;
}

function run(command: Command): void {
  close();
  void command.run();
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "ArrowDown") {
    event.preventDefault();
    activeIndex.value = Math.min(filtered.value.length - 1, activeIndex.value + 1);
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    activeIndex.value = Math.max(0, activeIndex.value - 1);
  } else if (event.key === "Enter") {
    event.preventDefault();
    const command = filtered.value[activeIndex.value];
    if (command) run(command);
  }
}

onMounted(() => window.addEventListener(PALETTE_EVENT, openPalette));
onBeforeUnmount(() => window.removeEventListener(PALETTE_EVENT, openPalette));
</script>

<template>
  <AppModal :open="open" width="sm" :padded="false" aria-label="命令面板" @close="close">
    <template #header>
      <input
        ref="inputRef"
        v-model="query"
        type="text"
        placeholder="搜索页面、设置项…"
        class="h-9 w-full border-0 bg-transparent text-body text-ink outline-none placeholder:text-ink-muted/60"
        @keydown="onKeydown"
      />
    </template>

    <div class="max-h-80 overflow-y-auto p-1">
      <button
        v-for="(command, index) in filtered"
        :key="command.id"
        type="button"
        class="flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left transition-fast"
        :class="
          index === activeIndex
            ? 'bg-brand-500/10 text-brand-600'
            : 'text-ink-soft hover:bg-ink/6 hover:text-ink'
        "
        @click="run(command)"
        @mouseenter="activeIndex = index"
      >
        <component :is="command.icon" :size="15" class="shrink-0" />
        <span class="flex-1 truncate text-caption">{{ command.label }}</span>
        <span v-if="command.hint" class="shrink-0 text-micro text-ink-muted">{{ command.hint }}</span>
      </button>

      <p v-if="filtered.length === 0" class="px-3 py-8 text-center text-caption text-ink-muted">
        没有匹配的命令
      </p>
    </div>

    <template #footer>
      <div class="flex items-center gap-4 text-micro text-ink-muted">
        <span><kbd class="kbd">↑</kbd><kbd class="kbd">↓</kbd> 选择</span>
        <span><kbd class="kbd">Enter</kbd> 执行</span>
        <span><kbd class="kbd">Esc</kbd> 关闭</span>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.kbd {
  display: inline-flex;
  height: 18px;
  min-width: 18px;
  align-items: center;
  justify-content: center;
  padding: 0 4px;
  border: 1px solid color-mix(in srgb, var(--color-line) 90%, transparent);
  border-radius: var(--radius-xs);
  background: var(--color-surface-muted);
  font-family: var(--font-mono);
  font-size: 10px;
  line-height: 1;
  color: var(--color-ink-muted);
}
</style>
