<script setup lang="ts">
import { RouterLink, useRoute } from "vue-router";
import { computed } from "vue";

import type { NavItem } from "@/components/layout/nav";
import { useTaskStore } from "@/stores/task";

/**
 * 标题栏内的横向导航。
 *
 * ## 为什么在标题栏里
 *
 * 导航与品牌字标同处一条 48px 的 chrome，窗口顶部不再是一条空荡荡的
 * 「标题栏」，而是「品牌 + 导航 + 窗口按钮」的一体化工具条——
 * 这也是主流桌面应用（VS Code / Figma）的做法。
 *
 * ## 尺寸
 *
 * 48px 高度里放「图标 + 文字」的横向组合：
 * 图标 15px、文字 12.5px、整条导航占满 48px 高度（点击区域更大），
 * 项与项之间留 2px，不再套玻璃胶囊。
 *
 * ## 激活态：底部细线 + 品牌色
 *
 * 不做实心色块、也不做浮起芯片——那两种都容易显得像“按钮”或“旧式工具栏”。
 * 这里用现代标签式导航：激活项带一层极淡的品牌底色、文字与图标转品牌蓝，
 * 底部压一条 3px 实心品牌短线（带轻微辉光）。选中状态明确，但不会变成大色块。
 *
 * ## 激活态自己算，不用 vue-router 的 active-class
 *
 * vue-router 的 `active-class` 是**前缀匹配**：路由 `/` 会被
 * `/instances`、`/settings` 等所有页面包含，于是「主页」在每个页面都高亮。
 *
 * 这里改用 `useRoute()` 自己判定——根路由必须精确匹配，
 * 其余路由仍然允许子路径命中（为将来的 `/instances/:id` 留位）。
 *
 * ## 交互
 *
 * 导航项**不带** `data-tauri-drag-region`，点击只触发路由跳转。
 * 拖动窗口请按住字标或右侧的空白区。
 */

defineProps<{ items: NavItem[] }>();

const route = useRoute();
const taskStore = useTaskStore();

/** 有任务正在运行时，导航里的「任务」带一个脉动计数。 */
const runningCount = computed(() => taskStore.runningCount);

/** 该导航项在当前路由下是否处于激活态。 */
function isActive(to: string): boolean {
  const path = route.path;

  // 根路由必须精确匹配，否则「主页」会在所有页面高亮
  if (to === "/") {
    return path === "/";
  }

  return path === to || path.startsWith(`${to}/`);
}
</script>

<template>
  <nav aria-label="主导航" class="flex h-12 items-center gap-0.5">
    <RouterLink
      v-for="item in items"
      :key="item.to"
      :to="item.to"
      :aria-current="isActive(item.to) ? 'page' : undefined"
      class="sereno-ease group relative flex h-full items-center gap-1.5 px-3.5 transition-colors duration-200 select-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-500/35"
      :class="isActive(item.to) ? 'bg-brand-500/[0.055]' : 'hover:bg-ink/[0.02]'"
    >
      <span class="relative flex items-center">
        <component
          :is="item.icon"
          :size="15"
          :stroke-width="2.1"
          class="transition-colors duration-200"
          :class="isActive(item.to) ? 'text-brand-500' : 'text-ink-muted group-hover:text-brand-500'"
        />
        <span
          v-if="item.to === '/tasks' && runningCount > 0"
          class="absolute -right-2 -top-2 flex h-3.5 min-w-3.5 animate-pulse items-center justify-center rounded-full bg-brand-500 px-1 text-[9px] font-semibold text-white shadow-sm"
        >
          {{ runningCount }}
        </span>
      </span>
      <span
        class="text-[12.5px] leading-none transition-colors duration-200"
        :class="
          isActive(item.to)
            ? 'font-semibold text-brand-600'
            : 'font-medium text-ink-muted group-hover:text-ink-soft'
        "
      >
        {{ item.label }}
      </span>
      <span
        v-if="isActive(item.to)"
        class="absolute inset-x-2 bottom-0 h-[3px] rounded-full bg-brand-500 shadow-[0_0_10px_rgb(90_126_199_/_0.45)]"
      />
    </RouterLink>
  </nav>
</template>
