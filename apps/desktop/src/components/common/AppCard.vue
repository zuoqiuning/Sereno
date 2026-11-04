<script setup lang="ts">
/**
 * 统一卡片：**页面主体内容只允许用它**。
 *
 * ## 从「玻璃」改成「纯白」
 *
 * 之前卡片是半透明 + 背景模糊。它压在同一块纯色背景上时，模糊几乎不可见，
 * 却要多付一次合成开销；更麻烦的是页面里还有一堆手写的 `sereno-glass`
 * 区块，圆角、内边距、阴影强度各不相同，同一屏看起来像两套设计。
 *
 * 现在的规则很简单：
 * - **页面主体 = 纯白卡片 + 细边 + 柔和阴影**（本组件）；
 * - **浮层才用玻璃**：弹窗、Toast、下拉、标题栏的账户入口。
 *
 * ## 悬停
 *
 * 可点击的卡片上浮 2px + 阴影加深。人眼对「位置变化」比「颜色变化」敏感，
 * 2px 是「被轻轻托起」，8px 就变成「跳起来」了。
 */
type Padding = "none" | "xs" | "sm" | "md" | "lg";

const props = withDefaults(
  defineProps<{
    /** 标题（不给就用 `#header` 插槽自己排） */
    title?: string;
    /** 内边距档位：xs 12 / sm 16 / md 20 / lg 24 */
    padding?: Padding;
    /** 是否给出悬停反馈（用于「可点击」的卡片） */
    hoverable?: boolean;
    /** 是否显示为「选中」态（如当前实例） */
    selected?: boolean;
  }>(),
  {
    title: "",
    padding: "md",
    hoverable: false,
    selected: false,
  },
);

const paddingClass: Record<Padding, string> = {
  none: "p-0",
  xs: "p-3",
  sm: "p-4",
  md: "p-5",
  lg: "p-6",
};
</script>

<template>
  <div
    class="app-card flex flex-col overflow-hidden"
    :class="[
      paddingClass[props.padding],
      props.hoverable ? 'lift-on-hover cursor-pointer' : '',
      props.selected ? 'is-selected' : '',
    ]"
  >
    <div v-if="$slots.header || title" class="mb-3 flex items-center justify-between gap-3">
      <slot name="header">
        <h3 v-if="title" class="flex items-center gap-2 text-h2 font-semibold text-ink">
          <slot name="icon" />
          {{ title }}
        </h3>
      </slot>
    </div>

    <div class="flex min-h-0 flex-1 flex-col justify-between">
      <slot />
    </div>

    <div v-if="$slots.footer" class="mt-3 border-t border-line/40 pt-3">
      <slot name="footer" />
    </div>
  </div>
</template>

<style scoped>
.app-card {
  border: 1px solid color-mix(in srgb, var(--color-line) 92%, transparent);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
  transition:
    box-shadow var(--duration-base) var(--ease-out-soft),
    border-color var(--duration-base) var(--ease-out-soft),
    transform var(--duration-base) var(--ease-out-soft);
}

.app-card.lift-on-hover:hover {
  /* 专业风 hover：边框微亮 + 背景微变，阴影不增长（边界交给边框） */
  border-color: color-mix(in srgb, var(--color-brand-500) 38%, transparent);
  background: color-mix(in srgb, var(--color-brand-500) 2%, var(--color-surface));
  box-shadow: var(--shadow-card);
}

.app-card.is-selected {
  border-color: color-mix(in srgb, var(--color-brand-500) 55%, transparent);
  box-shadow:
    0 0 0 1px color-mix(in srgb, var(--color-brand-500) 22%, transparent),
    var(--shadow-card);
}
</style>
