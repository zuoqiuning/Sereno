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

function toneClass(tone: ToastTone): string {
  if (tone === "success") return "text-success";
  if (tone === "error") return "text-danger";
  if (tone === "warning") return "text-warning";
  return "text-brand-500";
}
</script>

<template>
  <Teleport to="body">
    <div
      class="pointer-events-none fixed left-1/2 top-14 z-[60] flex w-[min(420px,calc(100vw-2rem))] -translate-x-1/2 flex-col gap-2"
    >
      <TransitionGroup name="toast">
        <div
          v-for="toast in toastStore.items"
          :key="toast.id"
          class="toast-card pointer-events-auto flex items-start gap-2.5 rounded-lg border border-line/60 bg-surface/95 px-3 py-2.5 shadow-[0_10px_30px_rgba(20,32,60,0.12)] backdrop-blur-xl"
          :class="toast.leaving ? 'toast-out' : ''"
        >
          <component
            :is="iconFor(toast.tone)"
            :size="16"
            class="mt-0.5 shrink-0"
            :class="toneClass(toast.tone)"
          />
          <div class="min-w-0 flex-1">
            <div class="text-[12.5px] font-medium leading-5 text-ink">{{ toast.title }}</div>
            <div v-if="toast.message" class="mt-0.5 text-[11px] leading-4 text-ink-muted">
              {{ toast.message }}
            </div>
          </div>
          <button
            type="button"
            aria-label="关闭通知"
            class="flex h-5 w-5 shrink-0 items-center justify-center rounded text-ink-muted transition-colors hover:bg-ink/6 hover:text-ink"
            @click="toastStore.dismiss(toast.id)"
          >
            <X :size="12" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-card {
  width: 100%;
}

.toast-enter-active,
.toast-leave-active {
  transition:
    opacity 220ms ease,
    transform 220ms cubic-bezier(0.22, 0.8, 0.24, 1);
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(-10px) scale(0.97);
}

.toast-leave-to {
  opacity: 0;
  transform: translateY(-6px) scale(0.97);
}

/* 前一张退场时，后面的卡片平滑上移，而不是瞬间跳位 */
.toast-move {
  transition: transform 240ms cubic-bezier(0.22, 0.8, 0.24, 1);
}
</style>
