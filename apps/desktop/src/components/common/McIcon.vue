<script setup lang="ts">
import type { Component } from "vue";

/**
 * Minecraft 图标的统一尺寸容器。
 *
 * ## 为什么需要它
 *
 * `minecraft-svg-vue` 里的组件是**纯 SVG**，没有 `size` 属性——
 * 而 lucide 图标有。侧导航用 `<component :is="icon" :size="15">` 统一渲染，
 * 直接换成 MC 图标会因为属性不匹配而尺寸错乱。
 *
 * 这里把两边统一成「外层 div 定尺寸 + inner 铺满」：
 * 无论传进来的是 lucide（认 `size`）还是 MC 图标（纯 SVG），
 * 视觉尺寸都一致。
 *
 * ## 像素风
 *
 * MC 图标是 16×16 的方块画，放大时必须 `image-rendering: pixelated`
 * 才不会糊成一片——这是它们看起来「像 Minecraft」的关键。
 */
withDefaults(
  defineProps<{
    /** 图标组件 */
    icon: Component;
    /** 边长（像素） */
    size?: number;
    /** 额外的 class */
    class?: string;
  }>(),
  { size: 15, class: "" },
);
</script>

<template>
  <span
    class="mc-icon"
    :style="{ width: `${size}px`, height: `${size}px` }"
    :class="$props.class"
    aria-hidden="true"
  >
    <component :is="icon" class="mc-icon__inner" :size="size" :stroke-width="2" />
  </span>
</template>

<style scoped>
.mc-icon {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}

.mc-icon__inner {
  width: 100%;
  height: 100%;
  /* 像素风的关键：放大时保持硬边缘，不做平滑插值 */
  image-rendering: pixelated;
  shape-rendering: crispEdges;
}

/* lucide 图标是描边风格，用 currentColor 跟随文字颜色 */
.mc-icon__inner :deep(svg) {
  width: 100%;
  height: 100%;
}
</style>
