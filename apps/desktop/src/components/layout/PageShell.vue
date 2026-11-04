<script setup lang="ts">
import type { Component } from "vue";

/**
 * 页面骨架：**所有页面必须用它包裹**。
 *
 * ## 为什么要有
 *
 * 之前 6 个页面各写一遍根容器：`max-w-4xl` / `max-w-6xl` / `max-w-7xl` 混用，
 * 内边距从 `pt-6` 到 `pt-8` 不等，页面头有的有、有的没有。
 * 结果是在 1440px 窗口里切页面时，内容区左右边界会跳 300px，
 * 用户每切一次都要重新找视觉重心——这是「不像一个产品」的第一来源。
 *
 * ## 为什么页头只有一行
 *
 * 第一版页头是「34px 图标色块 + 20px 标题 + 下方描述行」，连同内边距吃掉近 90px。
 * 在 720px 高的窗口里那是 12% 的可视高度——标题是给用户看的，不该比内容还占地方。
 * 现在压成单行：小图标 + 17px 标题 + 同行灰字描述 + 右侧操作，
 * 连同内边距约 54px（省 36px），内容区间距再从 24 收到 16（再省 8px）。
 *
 * 有左侧导航的页面（设置 / 更多 / 下载）直接 `:chrome="false"`：
 * 侧导航已经用高亮写明了「你在哪一类」，再叠一行同名标题就是纯占位——
 * 关掉后只留 20px 顶部留白，相对带页头再多出 34px。
 */
type ShellWidth = "md" | "lg" | "xl";

withDefaults(
  defineProps<{
    /** 页面标题；语义上必填，但给默认值以便 lint 规则统一 */
    title?: string;
    /** 一句话说明这个页面能做什么，别写「这里是设置」这种废话 */
    description?: string;
    icon?: Component;
    /** 单列页面用 lg；带左侧导航的页面用 xl */
    width?: ShellWidth;
    /** 是否绘制页头；左侧导航已表明位置时关掉，相对带页头再多出约 34px */
    chrome?: boolean;
    /**
     * 标题字重：
     * - `page`（默认）17px，内容密集页用，把高度让给内容；
     * - `hero` 20px，只有首页这种「内容本身就是一张大卡」的页面才用。
     */
    size?: "page" | "hero";
  }>(),
  { title: "", description: "", icon: undefined, width: "lg", chrome: true, size: "page" },
);
</script>

<template>
  <div class="page-shell" :class="[`page-shell--${width}`, { 'page-shell--bare': !chrome }]">
    <header v-if="chrome" class="page-header">
      <div class="page-header__main">
        <component :is="icon" v-if="icon" :size="16" :stroke-width="2" class="page-header__icon" />
        <h1 class="page-title" :class="{ 'page-title--hero': size === 'hero' }">{{ title }}</h1>
        <slot name="badge" />
        <p v-if="description" class="page-description">{{ description }}</p>
      </div>
      <div class="page-header__actions">
        <slot name="actions" />
      </div>
    </header>

    <div class="page-body">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.page-shell {
  display: flex;
  height: 100%;
  width: 100%;
  margin-inline: auto;
  flex-direction: column;
  padding: 12px 32px 24px;
}

/* 没有页头时，顶部留一点呼吸但不要多——省下的是内容空间 */
.page-shell--bare {
  padding-top: 20px;
}

.page-shell--md {
  max-width: 960px;
}

.page-shell--lg {
  max-width: 1152px;
}

.page-shell--xl {
  max-width: 1280px;
}

.page-header {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-bottom: 10px;
}

.page-header__main {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 8px;
}

.page-header__icon {
  flex-shrink: 0;
  color: var(--color-brand-500);
}

.page-title {
  flex-shrink: 0;
  font-family: var(--font-display);
  font-size: var(--text-page);
  line-height: var(--text-page--line-height);
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--color-ink);
}

/* 首页的问候语值得比普通页标题更重一点，但仍只占一行 */
.page-title--hero {
  font-size: var(--text-h1);
  line-height: var(--text-h1--line-height);
}

/* 描述与标题同行：省一整行高度；窄窗口直接隐藏，不挤压标题与操作 */
.page-description {
  min-width: 0;
  overflow: hidden;
  padding-left: 8px;
  border-left: 1px solid var(--alpha-ink-10);
  font-size: var(--text-micro);
  line-height: 1.4;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--color-ink-muted);
}

@media (max-width: 1080px) {
  .page-description {
    display: none;
  }
}

.page-header__actions {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: 8px;
}

.page-body {
  position: relative;
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: var(--space-md);
}
</style>
