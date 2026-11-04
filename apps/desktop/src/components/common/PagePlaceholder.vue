<script setup lang="ts">
import type { Component } from "vue";

import AppBadge from "@/components/common/AppBadge.vue";

/**
 * 未实现页面的统一占位。
 *
 * **不要留空白页**——未实现的页面一律用这个组件，
 * 让用户知道「这里有功能，只是还没做完」，而不是以为软件坏了。
 *
 * ## 设计意图
 *
 * 空状态最容易暴露「毛坯感」。这里用三层叠出层次：
 *
 * 1. **柔光光晕**（`.sereno-glow`）——品牌色从图标背后漫开，
 *    给整个页面一个视觉焦点，避免视线无处安放；
 * 2. **玻璃卡片**（`.sereno-glass`）——半透明 + 背景模糊，
 *    卡片能透出下层渐变，与窗口底色连成一体而不是贴上去的方块；
 * 3. **呼吸灯徽标**——一个缓慢 ping 的圆点，是「还在生长」的信号，
 *    比一句静态的「开发中」更有生命力。
 *
 * 所有视觉参数都来自全局令牌（main.css），组件里不写死色值。
 */
withDefaults(
  defineProps<{
    /** 图标组件（唯一来源：@lucide/vue） */
    icon: Component;
    /** 页面标题 */
    title: string;
    /** 功能说明 */
    description?: string;
    /** 是否显示「开发中」标记 */
    showBadge?: boolean;
  }>(),
  {
    description: "",
    showBadge: true,
  },
);
</script>

<template>
  <div class="flex h-full w-full flex-col items-center justify-center px-8 text-center select-none">
    <!-- 图标：光晕 + 玻璃卡片 -->
    <div class="relative mb-8 flex h-24 w-24 items-center justify-center">
      <div class="sereno-glow" />
      <div class="sereno-glass relative flex h-24 w-24 items-center justify-center rounded-xl">
        <component :is="icon" :size="36" :stroke-width="1.6" class="text-ink-soft" />
      </div>
    </div>

    <!-- 文字：标题与说明拉开层级 -->
    <h2 class="mb-3 text-[22px] font-semibold tracking-wide text-ink">
      {{ title }}
    </h2>
    <p v-if="description" class="max-w-[400px] text-body leading-relaxed text-ink-muted">
      {{ description }}
    </p>

    <!-- 徽标：呼吸灯 -->
    <AppBadge v-if="showBadge" class="mt-8" variant="brand" size="md" pulse> 开发中 </AppBadge>
  </div>
</template>
