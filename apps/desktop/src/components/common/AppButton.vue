<script setup lang="ts">
import { computed } from "vue";

import AppSpinner from "@/components/common/AppSpinner.vue";

/**
 * 自研按钮。
 *
 * ## 配色策略：大面积留白，小面积重彩
 *
 * 鸢尾蓝 `#0F64B5` 本身很深，**不要大面积铺满**（整张卡片涂蓝会非常压抑），
 * 只在「点睛之笔」的位置出现：
 *
 * - `primary` —— 实心鸢尾蓝 + 白字。这是全屏唯一允许的大块主色，
 *   因此**一屏至多一个**。
 * - `secondary` —— 纯白底 + 淡边 + 深灰字，hover 时边框与文字转鸢尾蓝。
 * - `ghost` —— 无边框，只在 hover 时浮起一层浅底。
 * - `danger` —— 实心危险色。
 *
 * ## 反馈（按下必须有回应）
 *
 * 按下时缩放 0.98 + 背景加深 + 阴影收缩，时长压到 80ms（比 hover 更快），
 * 松开时用 140ms 回弹。`transition` 明确列出属性而不是 `transition-all`——
 * 后者会把颜色与缩放绑在同一个时长上，按下瞬间「颜色慢半拍」，看起来像没响应。
 */
type Variant = "primary" | "soft" | "link" | "secondary" | "ghost" | "danger";
type Size = "xs" | "sm" | "md" | "lg";

const props = withDefaults(
  defineProps<{
    /** 配色语义 */
    variant?: Variant;
    /** 尺寸：xs 28 / sm 32 / md 36 / lg 44 */
    size?: Size;
    /** 是否禁用 */
    disabled?: boolean;
    /** 是否处于加载中（显示转圈并禁用点击） */
    loading?: boolean;
    /** 占满父容器宽度 */
    block?: boolean;
    /** 原生按钮类型 */
    type?: "button" | "submit" | "reset";
  }>(),
  {
    variant: "secondary",
    size: "md",
    disabled: false,
    loading: false,
    block: false,
    type: "button",
  },
);

const variantClass = computed(() => {
  switch (props.variant) {
    case "primary":
      return "bg-brand-500 text-white shadow-sm hover:bg-brand-600 active:bg-brand-600";
    /*
     * soft：列表行里的「主要但不喧宾夺主」的动作。
     * 一屏十几个实心蓝按钮会互相抢注意力，浅品牌底 + 品牌字既能被看见，又不压制内容。
     */
    case "soft":
      return "bg-[var(--alpha-brand-8)] text-brand-600 hover:bg-[var(--alpha-brand-12)] active:bg-[var(--alpha-brand-20)]";
    /*
     * link：列表行里的动作。平时透明无底色，悬停才浮起一层浅品牌底。
     * 一屏几十行都用实心蓝会互相抢注意力；文字本身是品牌色已经足够可辨识。
     */
    case "link":
      return "bg-transparent text-brand-500 hover:bg-[var(--alpha-brand-8)] active:bg-[var(--alpha-brand-12)]";
    case "secondary":
      return "border border-line/80 bg-surface text-ink-soft hover:border-brand-500/50 hover:text-brand-500 active:bg-surface-muted";
    case "ghost":
      return "bg-transparent text-ink-muted hover:bg-ink/6 hover:text-ink active:bg-ink/10";
    case "danger":
      return "bg-danger text-white shadow-sm hover:bg-danger/90 active:bg-danger";
    default:
      return "";
  }
});

const sizeClass = computed(() => {
  switch (props.size) {
    case "xs":
      return "h-7 px-2.5 text-micro";
    case "sm":
      return "h-8 px-3 text-caption";
    case "lg":
      return "h-11 px-6 text-h2";
    case "md":
    default:
      return "h-9 px-4 text-body";
  }
});
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    class="button-base select-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/45 disabled:pointer-events-none disabled:opacity-45"
    :class="[variantClass, sizeClass, block ? 'w-full' : '']"
  >
    <AppSpinner v-if="loading" :size="14" />
    <slot />
  </button>
</template>

<style scoped>
.button-base {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  border-radius: var(--radius-md);
  font-weight: 500;
  transition:
    background-color var(--duration-fast) var(--ease-out-soft),
    border-color var(--duration-fast) var(--ease-out-soft),
    color var(--duration-fast) var(--ease-out-soft),
    box-shadow var(--duration-fast) var(--ease-out-soft),
    transform var(--duration-fast) var(--ease-out-soft);
}

/* 按下：缩放 + 阴影收缩（「贴到桌面」），比 hover 更快 */
.button-base:active:not(:disabled) {
  transform: scale(var(--scale-press));
  box-shadow: 0 0 0 transparent;
  transition-duration: 80ms;
}
</style>
