<script setup lang="ts">
import { computed } from "vue";

/**
 * 滑块。
 *
 * ## 为什么不用原生 `<input type="range">` 的默认样式
 *
 * Chromium 默认滑块是「一根 8px 的灰色粗条 + 一个 12px 的蓝色方块」，
 * 与周边的圆角、阴影、字重完全不是一套语言，放在设置页里像嵌了个网页控件。
 *
 * 这里只重绘两端，交互仍交给原生 input：键盘、拖拽、无障碍全部免费。
 * - 轨道 4px，已填充部分用品牌色，未填充用 10% 中性灰；
 * - 拖动点 18px，白底 + 2px 品牌描边 + 双层阴影，悬停轻微放大；
 * - 用 `--fill` 变量传递百分比，避免为每个刻度写一堆背景渐变。
 */
const props = withDefaults(
  defineProps<{
    modelValue: number;
    min: number;
    max: number;
    step?: number;
    disabled?: boolean;
    /** 无障碍标签（没有可见文字时必须给） */
    label: string;
  }>(),
  { step: 1, disabled: false },
);

const emit = defineEmits<{
  "update:modelValue": [value: number];
  /** 拖动结束（松手）时触发，适合做「持久化保存」 */
  change: [value: number];
}>();

const fill = computed(() => {
  const span = props.max - props.min;
  if (span <= 0) return "0%";
  const ratio = (props.modelValue - props.min) / span;
  return `${Math.min(100, Math.max(0, ratio * 100))}%`;
});

function onInput(event: Event): void {
  emit("update:modelValue", Number((event.target as HTMLInputElement).value));
}

function onChange(event: Event): void {
  emit("change", Number((event.target as HTMLInputElement).value));
}
</script>

<template>
  <input
    type="range"
    class="slider"
    :style="{ '--fill': fill }"
    :min="min"
    :max="max"
    :step="step"
    :value="modelValue"
    :disabled="disabled"
    :aria-label="label"
    @input="onInput"
    @change="onChange"
  />
</template>

<style scoped>
.slider {
  -webkit-appearance: none;
  appearance: none;
  width: 100%;
  height: 20px;
  margin: 0;
  background: transparent;
  cursor: pointer;
}

.slider:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

/* 轨道：已填充段品牌色，未填充段 10% 中性灰 */
.slider::-webkit-slider-runnable-track {
  height: 4px;
  border-radius: 999px;
  background: linear-gradient(
    to right,
    var(--color-brand-500) 0 var(--fill),
    color-mix(in srgb, var(--color-ink) 10%, transparent) var(--fill) 100%
  );
}

.slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  height: 18px;
  width: 18px;
  /* (轨道 4px - 拖动点 18px) / 2 = -7px，手动居中 */
  margin-top: -7px;
  border: 2px solid var(--color-brand-500);
  border-radius: 999px;
  background: #ffffff;
  box-shadow:
    0 1px 2px rgb(16 24 40 / 0.16),
    0 3px 8px -2px rgb(16 24 40 / 0.18);
  transition:
    transform var(--duration-fast) var(--ease-out-soft),
    box-shadow var(--duration-fast) var(--ease-out-soft);
}

.slider:hover::-webkit-slider-thumb {
  transform: scale(1.12);
  box-shadow:
    0 1px 3px rgb(16 24 40 / 0.18),
    0 5px 12px -3px rgb(16 24 40 / 0.22);
}

.slider:active::-webkit-slider-thumb {
  transform: scale(1.04);
}

.slider:focus-visible {
  outline: none;
}

.slider:focus-visible::-webkit-slider-thumb {
  box-shadow:
    0 0 0 3px color-mix(in srgb, var(--color-brand-500) 22%, transparent),
    0 2px 6px rgb(16 24 40 / 0.18);
}

@media (prefers-reduced-motion: reduce) {
  .slider::-webkit-slider-thumb {
    transition: none;
  }
}
</style>
