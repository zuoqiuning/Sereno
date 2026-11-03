<script setup lang="ts">
import { LoaderCircle } from "@lucide/vue";

/**
 * 加载指示器。
 *
 * 只用 `@lucide/vue` 的图标（项目唯一的图标来源），不引第三方图标库。
 * 尺寸跟随字号缩放，线宽略粗一点以在小尺寸下依然清晰。
 */
withDefaults(
  defineProps<{
    /** 图标直径（px） */
    size?: number;
    /** 无障碍标签。为空时不加 role，交给调用方提供上下文。 */
    label?: string;
  }>(),
  {
    size: 16,
    label: "",
  },
);
</script>

<template>
  <span
    class="inline-flex shrink-0 items-center justify-center"
    :role="label ? 'status' : undefined"
    :aria-label="label || undefined"
  >
    <LoaderCircle :size="size" :stroke-width="2.4" class="animate-spin" />
  </span>
</template>
