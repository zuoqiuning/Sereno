<script setup lang="ts">
import { BadgeCheck, ChevronDown, Copy, Minus, UserRound, X } from "@lucide/vue";
import { onMounted, ref } from "vue";

import AccountModal from "@/components/layout/AccountModal.vue";
import TitleNav from "@/components/layout/TitleNav.vue";
import { allNavItems } from "@/components/layout/nav";
import { getAccountState } from "@/api/account";
import { useWindowControls } from "@/composables/useWindowControls";
import type { AccountState } from "@/types";

// 最大化按钮：**产品决策关闭**（2026-11-04 用户确认不需要最大化；
// 审计 S5 曾建议开启，实测后按用户偏好收回）。窗口仍可拖拽调整大小。
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
  <!--
    标题栏是一块**不透明**的背板（bg-canvas + 底部细线）。
    之前用渐变遮罩，页面滚动时下方内容会透上来，读到一半的字被压住，
    看起来像蒙了一层灰。现在是实色，任何底下的内容都不会透出。

    导航用 `left-1/2 + translateX(-50%)` 绝对居中：左右两侧宽度不同
    （左边是短字标，右边是账户 + 窗口按钮），靠 flex-1 均分出来的只是「伪居中」。
    只保留一个 flex-1 拖拽区，右侧控件才能真正贴住窗口右边缘。
  -->
  <div
    class="pointer-events-none absolute inset-x-0 top-0 z-40 flex h-12 items-center select-none border-b border-line/60 bg-surface"
  >
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

    <!-- 弹性拖拽区：导航两侧的空白都可以按住拖动窗口 -->
    <div data-tauri-drag-region class="sereno-drag pointer-events-auto h-full flex-1" />

    <!-- 账户入口：一眼看出是正版还是离线，点开就是账户面板 -->
    <div class="pointer-events-auto flex h-full shrink-0 items-center pr-2">
      <!--
        账户入口：极简。平时就是「图标 + 名字」，没有边框也没有底色，
        只有鼠标移上去时才浮起一层淡底——在 48px 高的标题栏里，
        一个带框的按钮会显得像贴上去的补丁。
      -->
      <button
        type="button"
        class="sereno-ease group flex h-8 items-center gap-2 rounded-md border border-transparent bg-transparent px-2 transition-all hover:border-line hover:bg-ink/4 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/30 min-[1100px]:max-w-40"
        :title="account ? `账户：${account.name}` : '账户'"
        @click="openAccount"
      >
        <span
          class="flex h-5 w-5 shrink-0 items-center justify-center rounded-round"
          :class="
            account?.kind === 'microsoft'
              ? 'bg-success-soft text-success'
              : 'bg-ink/8 text-ink-muted'
          "
        >
          <BadgeCheck v-if="account?.kind === 'microsoft'" :size="12" />
          <UserRound v-else :size="12" />
        </span>
        <!--
          窄窗口（< 1100px）只留头像：绝对居中的导航需要两侧各留出空间，
          否则会压到账户入口上。
        -->
        <span class="hidden min-w-0 flex-col items-start leading-[1.15] min-[1100px]:flex">
          <span class="max-w-21 truncate text-caption font-semibold text-ink-soft">
            {{ account?.name ?? "未登录" }}
          </span>
          <span
            class="text-micro font-medium"
            :class="account?.kind === 'microsoft' ? 'text-success' : 'text-ink-muted'"
          >
            {{ account?.kind === "microsoft" ? "微软正版" : "离线" }}
          </span>
        </span>
        <ChevronDown
          :size="12"
          class="hidden shrink-0 text-ink-muted transition-transform group-hover:translate-y-0.5 min-[1100px]:block"
        />
      </button>
    </div>

    <!-- 窗口控件：右侧留白与左侧品牌边距对齐 -->
    <div class="pointer-events-auto flex h-full shrink-0 items-center pr-3">
      <button
        v-if="showMaximize"
        type="button"
        aria-label="最大化 / 还原"
        class="group flex h-12 w-10 items-center justify-center"
        @click="toggleMaximize"
      >
        <span
          class="flex h-8 w-8 items-center justify-center rounded-sm transition-colors duration-150 group-hover:bg-ink/6 group-active:scale-95"
        >
          <Copy :size="20" :stroke-width="2.4" class="text-ink-soft" />
        </span>
      </button>

      <button
        type="button"
        aria-label="最小化"
        class="group flex h-12 w-10 items-center justify-center"
        @click="minimize"
      >
        <span
          class="flex h-8 w-8 items-center justify-center rounded-sm transition-colors duration-150 group-hover:bg-ink/6 group-active:scale-95"
        >
          <Minus :size="20" :stroke-width="2.4" class="text-ink-soft" />
        </span>
      </button>

      <button
        type="button"
        aria-label="关闭"
        class="group flex h-12 w-10 items-center justify-center"
        @click="close"
      >
        <span
          class="flex h-8 w-8 items-center justify-center rounded-sm transition-colors duration-150 group-hover:bg-danger group-active:scale-95"
        >
          <X :size="20" :stroke-width="2.4" class="text-ink-soft group-hover:text-white" />
        </span>
      </button>
    </div>

    <!-- 主导航：绝对居中，不受左右两侧宽度影响 -->
    <div
      class="pointer-events-none absolute left-1/2 top-0 flex h-full -translate-x-1/2 items-center"
    >
      <div class="pointer-events-auto">
        <TitleNav :items="allNavItems" />
      </div>
    </div>

    <AccountModal :open="accountOpen" @close="closeAccount" />
  </div>
</template>
