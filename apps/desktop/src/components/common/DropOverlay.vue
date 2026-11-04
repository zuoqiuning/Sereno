<script setup lang="ts">
import { Download } from "@lucide/vue";

import { dragging } from "@/composables/useFileDrop";
</script>

<template>
  <!--
    拖放遮罩：文件悬停在窗口上时出现。
    没有这层提示的话，用户拖进来却什么都没发生，会以为功能坏了。
  -->
  <Transition name="drop-fade">
    <div
      v-if="dragging"
      class="pointer-events-none fixed inset-0 z-[45] flex items-center justify-center bg-canvas/80 backdrop-blur-sm"
    >
      <div
        class="flex flex-col items-center gap-3 rounded-xl border-2 border-dashed border-brand-500/45 bg-surface px-10 py-8 shadow-lg"
      >
        <div
          class="flex h-12 w-12 items-center justify-center rounded-full bg-brand-500/10 text-brand-500"
        >
          <Download :size="22" />
        </div>
        <p class="text-body font-semibold text-ink">松开以导入</p>
        <p class="text-caption text-ink-muted">支持 .jar / .zip / .mrpack</p>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.drop-fade-enter-active,
.drop-fade-leave-active {
  transition: opacity var(--duration-fast) var(--ease-out-soft);
}

.drop-fade-enter-from,
.drop-fade-leave-to {
  opacity: 0;
}
</style>
