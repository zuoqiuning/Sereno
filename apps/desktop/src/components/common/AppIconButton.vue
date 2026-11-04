<script setup lang="ts">
/**
 * 图标按钮：**页面里所有纯图标按钮都用它**。
 *
 * 之前导航项、卡片操作、页码、开关处的按钮各写一套 class，
 * 高度从 28 到 36、圆角从 4 到 8、hover 底从 4% 到 8%，看起来「差不多但不一样」。
 * 这里统一三档尺寸（28 / 32 / 36）与三种形态，且 `label` 必填——它同时是
 * `aria-label` 与悬停提示，避免出现「一个没有任何说明的方块」。
 */
withDefaults(
  defineProps<{
    /** 必填：既做无障碍标签，也做悬停提示 */
    label: string;
    size?: "sm" | "md";
    variant?: "ghost" | "surface" | "danger";
    disabled?: boolean;
    /** 弹出菜单用：aria-haspopup */
    hasPopup?: boolean;
    /** 弹出菜单用：aria-expanded */
    expanded?: boolean;
  }>(),
  {
    size: "md",
    variant: "ghost",
    disabled: false,
    hasPopup: false,
    expanded: false,
  },
);
</script>

<template>
  <button
    type="button"
    :aria-label="label"
    :title="label"
    :aria-haspopup="hasPopup ? 'menu' : undefined"
    :aria-expanded="hasPopup ? expanded : undefined"
    :disabled="disabled"
    class="icon-button"
    :class="[`icon-button--${size}`, `icon-button--${variant}`]"
  >
    <slot />
  </button>
</template>

<style scoped>
.icon-button {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-md);
  color: var(--color-ink-muted);
  transition:
    background-color var(--duration-fast) var(--ease-out-soft),
    color var(--duration-fast) var(--ease-out-soft),
    border-color var(--duration-fast) var(--ease-out-soft),
    transform var(--duration-fast) var(--ease-out-soft);
}

.icon-button:active:not(:disabled) {
  transform: scale(var(--scale-press));
  transition-duration: 80ms;
}

.icon-button:focus-visible {
  outline: 2px solid var(--color-brand-500);
  outline-offset: 2px;
}

.icon-button:disabled {
  cursor: not-allowed;
  opacity: 0.4;
}

.icon-button--sm {
  height: 28px;
  width: 28px;
}

.icon-button--md {
  height: 32px;
  width: 32px;
}

.icon-button--ghost {
  background: transparent;
}

.icon-button--ghost:hover:not(:disabled) {
  background: var(--alpha-ink-6);
  color: var(--color-ink);
}

.icon-button--surface {
  border: 1px solid color-mix(in srgb, var(--color-line) 80%, transparent);
  background: var(--color-surface);
  color: var(--color-ink-soft);
}

.icon-button--surface:hover:not(:disabled) {
  border-color: var(--alpha-brand-20);
  color: var(--color-brand-600);
}

.icon-button--danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--color-danger) 10%, transparent);
  color: var(--color-danger);
}
</style>
