<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute } from "vue-router";

import type { NavItem } from "@/components/layout/nav";
import { useTaskStore } from "@/stores/task";

/**
 * 标题栏内的横向导航。
 *
 * ## 为什么在标题栏里
 *
 * 导航与品牌字标同处一条 48px 的 chrome，窗口顶部不再是空荡荡的「标题栏」，
 * 而是「品牌 + 导航 + 窗口按钮」的一体化工具条——主流桌面应用的做法。
 *
 * ## 激活态：一条**共享**的下划线
 *
 * 之前每个激活项各自渲染一条下划线，切换时「旧的一条消失、新的一条出现」，
 * 中间没有任何连续感。现在整条导航只有一条指示器，通过
 * `translateX + width` 滑到激活项下方——用户能看到「它从哪来」。
 *
 * 首次挂载用 `ready` 标志禁用过渡，避免页面刚打开时指示器从 0px 飞过来。
 *
 * ## 激活态自己算，不用 vue-router 的 active-class
 *
 * `active-class` 是**前缀匹配**：路由 `/` 会被所有页面包含，
 * 于是「主页」在每个页面都高亮。这里用 `useRoute()` 自己判定。
 */
defineProps<{ items: NavItem[] }>();

const route = useRoute();
const taskStore = useTaskStore();

/** 有任务正在运行时，导航里的「任务」带一个脉动计数。 */
const runningCount = computed(() => taskStore.runningCount);

/** 该导航项在当前路由下是否处于激活态。 */
function isActive(to: string): boolean {
  const path = route.path;
  if (to === "/") return path === "/";
  return path === to || path.startsWith(`${to}/`);
}

const navRef = ref<HTMLElement | null>(null);
const ready = ref(false);
const indicator = ref({ x: 0, width: 0, visible: false });

/** 把指示器移到激活项下方。 */
function syncIndicator(): void {
  const nav = navRef.value;
  if (!nav) return;
  const active = nav.querySelector<HTMLElement>("[data-active='true']");
  if (!active) {
    indicator.value = { ...indicator.value, visible: false };
    return;
  }
  const navRect = nav.getBoundingClientRect();
  const rect = active.getBoundingClientRect();
  // 指示器是一小截（20px），居中在激活项下方——不是一条压满整项的长线
  indicator.value = {
    x: rect.left - navRect.left + rect.width / 2 - 10,
    width: 20,
    visible: true,
  };
}

onMounted(async () => {
  await nextTick();
  syncIndicator();
  // 下一帧才启用过渡：首次定位不该有动画
  requestAnimationFrame(() => {
    ready.value = true;
  });
});

watch(() => route.path, () => void nextTick(syncIndicator));

// 窗口尺寸变化时重新对齐（无边框窗口可自由缩放）
let observer: ResizeObserver | null = null;
onMounted(() => {
  if (typeof ResizeObserver === "undefined") return;
  observer = new ResizeObserver(() => syncIndicator());
  if (navRef.value) observer.observe(navRef.value);
});

onBeforeUnmount(() => {
  observer?.disconnect();
  observer = null;
});
</script>

<template>
  <nav ref="navRef" aria-label="主导航" class="relative flex h-12 items-center gap-0.5">
    <RouterLink
      v-for="item in items"
      :key="item.to"
      :to="item.to"
      :data-active="isActive(item.to) ? 'true' : 'false'"
      :aria-current="isActive(item.to) ? 'page' : undefined"
      class="nav-item group"
      :class="{ 'is-active': isActive(item.to) }"
    >
      <span class="relative flex items-center">
        <component :is="item.icon" :size="15" :stroke-width="2.1" class="nav-item__icon" />
        <span
          v-if="item.to === '/tasks' && runningCount > 0"
          class="absolute -right-2 -top-2 flex h-3.5 min-w-3.5 items-center justify-center rounded-round bg-brand-500 px-1 text-micro font-semibold text-white shadow-sm"
        >
          {{ runningCount }}
        </span>
      </span>
      <span class="nav-item__label">
        {{ item.label }}
      </span>
    </RouterLink>

    <!-- 共享指示器：切换时滑过去，而不是「一个消失一个出现」 -->
    <span
      class="nav-indicator"
      :class="{ 'is-ready': ready, 'is-hidden': !indicator.visible }"
      :style="{ transform: `translateX(${indicator.x}px)`, width: `${indicator.width}px` }"
    />
  </nav>
</template>

<style scoped>
.nav-item {
  position: relative;
  display: flex;
  height: 100%;
  align-items: center;
  gap: 6px;
  padding: 0 14px;
  color: var(--color-ink-soft);
  transition:
    color var(--duration-fast) var(--ease-out-soft),
    background-color var(--duration-fast) var(--ease-out-soft);
}

.nav-item::before {
  content: "";
  position: absolute;
  inset: 8px 4px;
  border-radius: var(--radius-md);
  background: transparent;
  transition: background-color var(--duration-fast) var(--ease-out-soft);
}

.nav-item:hover::before {
  background: var(--alpha-ink-4);
}

.nav-item.is-active::before {
  /* 激活项不再铺色块：只靠「文字变蓝 + 底部短横线」表达，避免双重强调 */
  background: transparent;
}

.nav-item:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--color-brand-500) 55%, transparent);
  outline-offset: -2px;
  border-radius: var(--radius-md);
}

.nav-item__icon {
  position: relative;
  color: inherit;
  transition: color var(--duration-fast) var(--ease-out-soft);
}

.nav-item:hover .nav-item__icon {
  color: var(--color-brand-500);
}

.nav-item__label {
  position: relative;
  font-size: 12.5px;
  font-weight: 500;
  line-height: 1;
  transition: color var(--duration-fast) var(--ease-out-soft);
}

.nav-item.is-active {
  color: var(--color-brand-600);
}

.nav-item.is-active .nav-item__icon {
  color: var(--color-brand-500);
}

.nav-item.is-active .nav-item__label {
  font-weight: 600;
}

.nav-indicator {
  position: absolute;
  bottom: 0;
  left: 0;
  height: 3px;
  border-radius: 999px;
  background: var(--color-brand-500);
  box-shadow: 0 0 10px color-mix(in srgb, var(--color-brand-500) 45%, transparent);
  pointer-events: none;
  opacity: 1;
}

.nav-indicator.is-hidden {
  opacity: 0;
}

/* 首次定位不播放动画，之后才启用过渡 */
.nav-indicator.is-ready {
  transition:
    transform var(--duration-base) var(--ease-out-soft),
    width var(--duration-base) var(--ease-out-soft),
    opacity var(--duration-fast) var(--ease-out-soft);
}

@media (prefers-reduced-motion: reduce) {
  .nav-indicator.is-ready {
    transition: none;
  }
}
</style>
