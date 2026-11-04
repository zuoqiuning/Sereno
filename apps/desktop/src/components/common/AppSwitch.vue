<script setup lang="ts">
/**
 * 开关。
 *
 * ## 为什么单独封装
 *
 * 之前设置页里 4 个开关各自复制了一段 `peer` + `after:` 的 Tailwind 类，
 * 只要改一次视觉就要改四处。更关键的是：原生 checkbox 的 thumb 是**匀速**移动，
 * 看起来像滑块而不是开关。
 *
 * 这里给 thumb 一点**轻微过冲**（`--ease-overshoot`，视觉上约 0.5px），
 * 这是整个界面里唯一允许「弹性」的地方——因为开关是物理控件。
 */
withDefaults(
  defineProps<{
    modelValue: boolean;
    disabled?: boolean;
    /** 无障碍标签（没有可见文字时必须给） */
    label?: string;
  }>(),
  { disabled: false, label: "开关" },
);

const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();
</script>

<template>
  <button
    type="button"
    role="switch"
    :aria-checked="modelValue"
    :aria-label="label"
    :disabled="disabled"
    class="switch"
    :class="{ 'is-on': modelValue }"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span class="switch__thumb" />
  </button>
</template>

<style scoped>
.switch {
  position: relative;
  display: inline-flex;
  height: 24px;
  width: 44px;
  flex-shrink: 0;
  align-items: center;
  padding: 2px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--color-ink) 14%, transparent);
  box-shadow: inset 0 1px 2px rgb(16 24 40 / 0.08);
  transition:
    background-color var(--duration-base) var(--ease-out-soft),
    box-shadow var(--duration-base) var(--ease-out-soft);
}

.switch:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.switch:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--color-brand-500) 55%, transparent);
  outline-offset: 2px;
}

.switch.is-on {
  background: var(--color-brand-500);
  box-shadow: inset 0 1px 2px rgb(0 0 0 / 0.12);
}

.switch__thumb {
  display: block;
  height: 20px;
  width: 20px;
  border-radius: 999px;
  background: #ffffff;
  box-shadow:
    0 1px 2px rgb(16 24 40 / 0.18),
    0 2px 6px rgb(16 24 40 / 0.12);
  transform: translateX(0);
  transition: transform 200ms var(--ease-overshoot);
}

.switch.is-on .switch__thumb {
  transform: translateX(20px);
}

@media (prefers-reduced-motion: reduce) {
  .switch__thumb {
    transition-duration: 0.01ms;
  }
}
</style>
