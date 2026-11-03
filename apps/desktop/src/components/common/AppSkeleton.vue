<script setup lang="ts">
/**
 * 骨架屏占位块。
 *
 * ## 为什么用骨架而不是假数据
 *
 * 填「1.20.1 生存模式」这类虚构内容，用户会当成真实状态；
 * 骨架条在设计语言里就是「这里将来会有内容」，**不会被误读成真实数据**。
 *
 * ## 尺寸
 *
 * `width` / `height` / `rounded` 收**完整的 Tailwind 类名**而不是数值——
 * 骨架条要和它将来替换的真实元素对齐（文字行高 20px、标题 28px），
 * 数值参数反而限制这种对齐。
 *
 * ## 闪烁
 *
 * 默认 `pulse`（`animate-pulse`）。`prefers-reduced-motion` 下由全局规则
 * 自动静止，不会给晕动症用户造成困扰。
 */
withDefaults(
  defineProps<{
    /** 宽度类名，如 `w-full` / `w-40` */
    width?: string;
    /** 高度类名，如 `h-4` / `h-5` */
    height?: string;
    /** 圆角类名，如 `rounded-xs` / `rounded-full` */
    rounded?: string;
    /** 是否闪烁 */
    pulse?: boolean;
  }>(),
  {
    width: "w-full",
    height: "h-4",
    rounded: "rounded-xs",
    pulse: true,
  },
);
</script>

<template>
  <div class="sereno-skeleton" :class="[width, height, rounded, pulse ? 'animate-pulse' : '']" />
</template>
