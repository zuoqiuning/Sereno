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
 * - `secondary` —— 流光白底 + 淡边 + 深灰字，hover 时边框与文字转鸢尾蓝。
 * - `ghost` —— 无边框，只在 hover 时浮起一层浅底。
 * - `danger` —— 实心危险色。
 *
 * ## 圆角
 *
 * `--radius-md`（8px）。**不要**用胶囊形：桌面端界面克制优先，
 * `rounded-round` 只留给圆形头像与胶囊形标签。
 */
type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";

const props = withDefaults(
  defineProps<{
    /** 配色语义 */
    variant?: Variant;
    /** 尺寸 */
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
      // 主按钮：纯正鸢尾蓝，hover 加深，按下缩放
      return "bg-brand-500 text-white shadow-sm hover:bg-brand-600 active:scale-[0.98]";
    case "secondary":
      // 次按钮：流光白底 + 淡边，hover 时边框与文字转鸢尾蓝
      return "border border-line/80 bg-surface text-ink-soft hover:border-brand-500/50 hover:text-brand-500";
    case "ghost":
      // 幽灵按钮：无边框，hover 只有浅色背景
      return "bg-transparent text-ink-muted hover:bg-ink/5 hover:text-ink";
    case "danger":
      return "bg-danger text-white shadow-sm hover:bg-danger/90 active:scale-[0.98]";
    default:
      return "";
  }
});

const sizeClass = computed(() => {
  switch (props.size) {
    case "sm":
      return "h-8 px-3 text-[13px]";
    case "lg":
      return "h-11 px-6 text-[15px]";
    case "md":
    default:
      return "h-9 px-4 text-[14px]";
  }
});
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    class="inline-flex select-none items-center justify-center gap-2 rounded-md font-medium transition-all duration-150 sereno-ease focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/50 disabled:pointer-events-none disabled:opacity-45"
    :class="[variantClass, sizeClass, block ? 'w-full' : '']"
  >
    <AppSpinner v-if="loading" :size="14" />
    <slot />
  </button>
</template>
