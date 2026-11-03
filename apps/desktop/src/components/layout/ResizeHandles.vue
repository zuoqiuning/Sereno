<script setup lang="ts">
import { onMounted } from "vue";

import { useWindowControls } from "@/composables/useWindowControls";
import type { ResizeDirection } from "@/composables/useWindowControls";

/**
 * 自绘缩放热区。
 *
 * 没有原生边框，就没有系统的缩放边。这里在窗口四边和四角放置透明热区。
 *
 * 要点：
 * - **四角必须排在四边之后**，保证角上的热区优先命中；
 * - 热区 `z-30`，低于标题栏的 `z-40`，避免右上角盖住窗口按钮；
 * - 最大化状态下隐藏热区（最大化窗口不应再出现缩放光标）；
 * - 只处理鼠标左键，避免右键菜单触发缩放；
 * - 热区自身透明，不影响页面点击。
 *
 * 尺寸按 4–6px 设计：1px 在 150% DPI 下只有 1.5 物理像素，手感偏细。
 */

interface ResizeZone {
  direction: ResizeDirection;
  /** 热区位置类 */
  class: string;
}

const zones: ResizeZone[] = [
  // ---- 四边 ----
  { direction: "North", class: "top-0 left-2 right-2 h-1.5 cursor-n-resize" },
  { direction: "South", class: "bottom-0 left-2 right-2 h-1.5 cursor-s-resize" },
  { direction: "West", class: "left-0 top-2 bottom-2 w-1.5 cursor-w-resize" },
  { direction: "East", class: "right-0 top-2 bottom-2 w-1.5 cursor-e-resize" },
  // ---- 四角（必须排在四边之后）----
  { direction: "NorthWest", class: "left-0 top-0 h-3 w-3 cursor-nw-resize" },
  { direction: "NorthEast", class: "right-0 top-0 h-3 w-3 cursor-ne-resize" },
  { direction: "SouthWest", class: "bottom-0 left-0 h-3 w-3 cursor-sw-resize" },
  { direction: "SouthEast", class: "bottom-0 right-0 h-3 w-3 cursor-se-resize" },
];

const { isMaximized, startResizeDragging, syncMaximized, bindResizeListener } = useWindowControls();

/** 开始缩放拖拽。只响应鼠标左键。 */
async function onResizeStart(event: MouseEvent, direction: ResizeDirection): Promise<void> {
  if (event.button !== 0) {
    return;
  }
  event.preventDefault();
  await startResizeDragging(direction);
}

onMounted(() => {
  void syncMaximized();
  bindResizeListener();
});
</script>

<template>
  <div v-if="!isMaximized" class="pointer-events-none absolute inset-0 z-30">
    <div
      v-for="zone in zones"
      :key="zone.direction"
      class="pointer-events-auto absolute"
      :class="zone.class"
      @mousedown="onResizeStart($event, zone.direction)"
    />
  </div>
</template>
