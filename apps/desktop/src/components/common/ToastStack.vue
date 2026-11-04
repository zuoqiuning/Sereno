<script setup lang="ts">
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "@lucide/vue";

import { useToastStore } from "@/stores/toast";
import type { ToastTone } from "@/stores/toast";

const toastStore = useToastStore();

function iconFor(tone: ToastTone) {
  if (tone === "success") return CheckCircle2;
  if (tone === "error") return XCircle;
  if (tone === "warning") return AlertTriangle;
  return Info;
}

/** 图标底色：只让图标带一点语义色，卡片本身保持中性。 */
function bubbleClass(tone: ToastTone): string {
  if (tone === "success") return "bg-success-soft text-success";
  if (tone === "error") return "bg-danger-soft text-danger";
  if (tone === "warning") return "bg-warning-soft text-warning";
  return "bg-brand-500/10 text-brand-500";
}
</script>

<template>
  <Teleport to="body">
    <div
      class="pointer-events-none fixed left-1/2 top-14 z-[60] w-[min(420px,calc(100vw-2rem))] -translate-x-1/2"
    >
      <!-- relative 供离场卡片绝对定位；卡片宽度随内容自适应，最多铺满这一列 -->
      <TransitionGroup name="toast" tag="div" class="relative flex flex-col items-center gap-2">
        <div
          v-for="toast in toastStore.items"
          :key="toast.id"
          class="toast-card pointer-events-auto flex w-fit max-w-full items-start gap-2.5 rounded-xl border border-line/60 bg-surface/95 py-2.5 pl-3 pr-2.5 shadow-[0_2px_6px_-1px_rgba(16,24,40,0.06),0_14px_36px_-12px_rgba(16,24,40,0.24)] backdrop-blur-xl"
        >
          <span
            class="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md"
            :class="bubbleClass(toast.tone)"
          >
            <component :is="iconFor(toast.tone)" :size="14" />
          </span>
          <div class="min-w-0 flex-1 pt-px">
            <div class="wrap-break-word text-caption font-semibold leading-[1.45] text-ink">
              {{ toast.title }}
            </div>
            <div
              v-if="toast.message"
              class="mt-0.5 wrap-break-word text-caption leading-[1.55] text-ink-muted"
            >
              {{ toast.message }}
            </div>
          </div>
          <button
            type="button"
            aria-label="关闭通知"
            class="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-ink/6 hover:text-ink"
            @click="toastStore.dismiss(toast.id)"
          >
            <X :size="13" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-card {
  position: relative;
  isolation: isolate;
  will-change: transform, opacity;
}

/* 卡片周围一圈模糊光晕：浅色主题是淡淡的灰，深色主题会自动变成微光 */
.toast-card::before {
  content: "";
  position: absolute;
  inset: -8px;
  z-index: -1;
  border-radius: 18px;
  background: color-mix(in srgb, var(--color-ink) 9%, transparent);
  filter: blur(12px);
  opacity: 0.55;
  pointer-events: none;
}

.toast-enter-active {
  transition:
    opacity 180ms ease,
    transform 320ms cubic-bezier(0.22, 1, 0.36, 1);
}

/*
 * 离场卡片脱离文档流，后面的卡片才能通过 FLIP 平滑补位；
 * 保持 max-content 宽度与居中，避免离场瞬间尺寸/位置跳变。
 */
.toast-leave-active {
  position: absolute;
  left: 50%;
  width: max-content;
  max-width: 100%;
  pointer-events: none;
  transition:
    opacity 180ms ease,
    transform 220ms cubic-bezier(0.4, 0, 0.7, 0.2);
}

.toast-leave-from {
  transform: translateX(-50%);
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(-10px) scale(0.96);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-4px) scale(0.96);
}

/* 补位动画比淡出略长，避免视觉上「等一下就跳过去」 */
.toast-move {
  transition: transform 320ms cubic-bezier(0.22, 1, 0.36, 1);
}
</style>
