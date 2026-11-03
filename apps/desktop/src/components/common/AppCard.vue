<script setup lang="ts">
/**
 * 统一卡片。
 *
 * ## 为什么要封装
 *
 * 首页四张卡、以后的实例卡、下载卡、账户卡，视觉要求完全一致：
 * 流光白半透明底 + 背景模糊 + 细边 + 柔和阴影 + 16px 圆角 + 24px 内边距。
 * 不封装的话，每个页面都会各写一遍 `border-line/60 bg-glass/60 shadow-sm
 * backdrop-blur-md`，改一次风格要翻十几个文件。
 *
 * 玻璃质感的具体参数集中在 `main.css` 的 `.sereno-glass` 里，
 * 这里只负责**结构**（头 / 体 / 脚）与**内边距档位**。
 *
 * ## 插槽
 *
 * - `#header` —— 头部整行（标题 + 右侧操作区）。给了它就不渲染 `title`。
 * - `#icon` —— `title` 前的图标位。
 * - 默认插槽 —— 卡片主体，内部 `justify-between` 分布子元素。
 * - `#footer` —— 底部条（带一条淡分隔线）。
 */
type Padding = "none" | "sm" | "md" | "lg";

withDefaults(
  defineProps<{
    /** 标题（不给就用 `#header` 插槽自己排） */
    title?: string;
    /** 内边距档位 */
    padding?: Padding;
    /** 是否给出悬停反馈（用于「可点击」的卡片） */
    hoverable?: boolean;
  }>(),
  {
    title: "",
    padding: "md",
    hoverable: false,
  },
);

/** 内边距档位 → Tailwind 类。md = 24px，与设计体系的 `--space-lg` 一致。 */
const paddingClass: Record<Padding, string> = {
  none: "p-0",
  sm: "p-4",
  md: "p-6",
  lg: "p-8",
};
</script>

<template>
  <div
    class="sereno-glass flex flex-col overflow-hidden rounded-xl"
    :class="[paddingClass[padding], hoverable ? 'sereno-glass-hover cursor-pointer' : '']"
  >
    <!-- 头部：给了 header 插槽就整行交给调用方 -->
    <div v-if="$slots.header || title" class="mb-4 flex items-center justify-between gap-3">
      <slot name="header">
        <h3 v-if="title" class="flex items-center gap-2 text-[16px] font-semibold text-ink">
          <slot name="icon" />
          {{ title }}
        </h3>
      </slot>
    </div>

    <!-- 主体：flex-1 + justify-between，让子元素自动分布到上下两端 -->
    <div class="flex min-h-0 flex-1 flex-col justify-between">
      <slot />
    </div>

    <!-- 底部条 -->
    <div v-if="$slots.footer" class="mt-4 border-t border-line/40 pt-3">
      <slot name="footer" />
    </div>
  </div>
</template>
