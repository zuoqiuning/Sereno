<script setup lang="ts">
import { Copy, Minus, User, X } from "@lucide/vue";
import { onMounted, ref } from "vue";

import AccountModal from "@/components/layout/AccountModal.vue";
import TitleNav from "@/components/layout/TitleNav.vue";
import { allNavItems } from "@/components/layout/nav";
import { getAccountState } from "@/api/account";
import { useWindowControls } from "@/composables/useWindowControls";
import type { AccountState } from "@/types";

const showMaximize = false;
const { minimize, toggleMaximize, close, syncMaximized, bindResizeListener } = useWindowControls();

/** 当前账户：标题栏只展示名字与类型，点开才是完整账户面板。 */
const account = ref<AccountState | null>(null);
const accountOpen = ref(false);

async function loadAccount(): Promise<void> {
  try {
    account.value = await getAccountState();
  } catch {
    // 账户读取失败不影响窗口控件，保持上一次的值
  }
}

function openAccount(): void {
  accountOpen.value = true;
}

function closeAccount(): void {
  accountOpen.value = false;
  void loadAccount();
}

onMounted(() => {
  void syncMaximized();
  bindResizeListener();
  void loadAccount();
});
</script>

<template>
  <div class="pointer-events-none absolute inset-x-0 top-0 z-40 flex h-12 items-center select-none">
    <!-- 品牌字标 -->
    <div
      data-tauri-drag-region
      class="sereno-drag pointer-events-auto flex h-full shrink-0 items-center pl-5 pr-4"
    >
      <span
        data-tauri-drag-region
        class="font-display text-[20px] font-semibold leading-none tracking-[0.03em] text-brand-500 select-none"
      >
        Sereno
      </span>
    </div>

    <!-- 左侧弹性拖动区 -->
    <div data-tauri-drag-region class="sereno-drag pointer-events-auto h-full flex-1" />

    <!-- 居中导航区：包含主页、实例、下载、设置 -->
    <div class="pointer-events-auto flex h-full shrink-0 items-center justify-center">
      <TitleNav :items="allNavItems" />
    </div>

    <!-- 右侧弹性拖动区 -->
    <div data-tauri-drag-region class="sereno-drag pointer-events-auto h-full flex-1" />

    <!-- 账户入口：点开是登录 / 切换账户面板 -->
    <div class="pointer-events-auto flex h-full shrink-0 items-center pr-1">
      <button
        type="button"
        class="sereno-ease group flex h-8 max-w-[132px] items-center gap-1.5 rounded-md px-2 transition-colors hover:bg-ink/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/35"
        :title="account ? `账户：${account.name}` : '账户'"
        @click="openAccount"
      >
        <span
          class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[10px] font-semibold"
          :class="
            account?.kind === 'microsoft'
              ? 'bg-success-soft text-success'
              : 'bg-brand-500/12 text-brand-600'
          "
        >
          <User v-if="!account" :size="12" />
          <template v-else>{{ account.name.slice(0, 1).toUpperCase() }}</template>
        </span>
        <span class="truncate text-[12px] font-medium text-ink-soft group-hover:text-ink">
          {{ account?.name ?? "账户" }}
        </span>
      </button>
    </div>

    <!-- 窗口控件：右侧留白与左侧品牌边距对齐 -->
    <div class="pointer-events-auto flex h-full shrink-0 items-center pr-3">
      <button
        v-if="showMaximize"
        type="button"
        aria-label="最大化 / 还原"
        class="group flex h-12 w-[42px] items-center justify-center"
        @click="toggleMaximize"
      >
        <span
          class="flex h-9 w-9 items-center justify-center rounded-sm transition-colors duration-150 group-hover:bg-ink/6 group-active:scale-95"
        >
          <Copy :size="24" :stroke-width="2.8" class="text-ink-soft" />
        </span>
      </button>

      <button
        type="button"
        aria-label="最小化"
        class="group flex h-12 w-[42px] items-center justify-center"
        @click="minimize"
      >
        <span
          class="flex h-9 w-9 items-center justify-center rounded-sm transition-colors duration-150 group-hover:bg-ink/6 group-active:scale-95"
        >
          <Minus :size="24" :stroke-width="2.8" class="text-ink-soft" />
        </span>
      </button>

      <button
        type="button"
        aria-label="关闭"
        class="group flex h-12 w-[42px] items-center justify-center"
        @click="close"
      >
        <span
          class="flex h-9 w-9 items-center justify-center rounded-sm transition-colors duration-150 group-hover:bg-danger group-active:scale-95"
        >
          <X :size="24" :stroke-width="2.8" class="text-ink-soft group-hover:text-white" />
        </span>
      </button>
    </div>

    <AccountModal :open="accountOpen" @close="closeAccount" />
  </div>
</template>
