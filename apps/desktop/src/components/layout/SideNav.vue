<script setup lang="ts">
import type { Component } from "vue";

/**
 * 左侧分类导航：下载 / 设置 / 更多三个页面共用。
 *
 * ## 统一激活态语言
 *
 * 之前三个页面各写了一遍左侧导航，激活态是「实心品牌色 + 白字」，
 * 和标题栏主导航的「底部短线」是两种语言，切页时视觉上像两个产品。
 * 这里统一成**浅品牌底 + 品牌文字**：主导航表示「位置」，侧导航表示「分类」，
 * 视觉强度递减，层级才清楚。
 */

export interface SideNavItem {
  id: string;
  label: string;
  icon?: Component;
  /** 右侧计数（可选），如「任务 3」 */
  count?: number;
}

withDefaults(
  defineProps<{
    title?: string;
    items: SideNavItem[];
    modelValue: string;
  }>(),
  { title: "" },
);

defineEmits<{ "update:modelValue": [id: string] }>();
</script>

<template>
  <aside class="side-nav">
    <h2 v-if="title" class="side-nav__title">{{ title }}</h2>
    <nav class="side-nav__list">
      <button
        v-for="item in items"
        :key="item.id"
        type="button"
        class="side-nav__item"
        :class="{ 'is-active': item.id === modelValue }"
        :aria-current="item.id === modelValue ? 'page' : undefined"
        @click="$emit('update:modelValue', item.id)"
      >
        <component :is="item.icon" v-if="item.icon" :size="15" :stroke-width="2" class="shrink-0" />
        <span class="side-nav__label">{{ item.label }}</span>
        <span v-if="item.count" class="side-nav__count tabular">{{ item.count }}</span>
      </button>
    </nav>
  </aside>
</template>

<style scoped>
.side-nav {
  width: 180px;
  flex-shrink: 0;
  align-self: flex-start;
  padding: 8px;
  border: 1px solid color-mix(in srgb, var(--color-line) 92%, transparent);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
}

.side-nav__title {
  padding: 6px 10px 8px;
  font-size: var(--text-micro);
  font-weight: 600;
  letter-spacing: 0.14em;
  color: var(--color-ink-muted);
}

.side-nav__list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.side-nav__item {
  position: relative;
  display: flex;
  height: 34px;
  align-items: center;
  gap: 10px;
  padding: 0 10px;
  border-radius: var(--radius-md);
  font-size: var(--text-body);
  font-weight: 500;
  color: var(--color-ink-muted);
  transition:
    background-color var(--duration-fast) var(--ease-out-soft),
    color var(--duration-fast) var(--ease-out-soft);
}

.side-nav__item:hover {
  background: var(--alpha-ink-4);
  color: var(--color-ink-soft);
}

.side-nav__item.is-active {
  /* 极简：激活态不铺色块，只靠「文字变蓝 + 左缘竖条」表达 */
  background: transparent;
  color: var(--color-brand-600);
  font-weight: 600;
}

/* 激活项左缘的引导条：让「当前在哪」不需要靠背景色的深浅去猜 */
.side-nav__item.is-active::before {
  content: "";
  position: absolute;
  left: 0;
  top: 20%;
  height: 60%;
  width: 3px;
  border-radius: 0 3px 3px 0;
  background: var(--color-brand-500);
}

.side-nav__label {
  flex: 1;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.side-nav__count {
  font-size: var(--text-micro);
  color: var(--color-ink-muted);
}
</style>
