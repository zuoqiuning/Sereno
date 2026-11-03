<script setup lang="ts">
import { computed, useId } from "vue";

/**
 * 品牌加载态：SVG 图标 + 一道沿笔画扫过的柔光。
 *
 * 用作页面的“正在加载”占位，替代整块骨架屏。`label` 可选，
 * 不传时只保留图标，避免每个加载场景都堆一句说明文字。
 */
withDefaults(
  defineProps<{
    /** 图标边长（px） */
    size?: number;
    /** 可选说明文字 */
    label?: string;
  }>(),
  {
    size: 72,
    label: "",
  },
);

const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
const cutId = computed(() => `sereno-cut-${uid}`);
const logoMaskId = computed(() => `sereno-logo-mask-${uid}`);
const shineId = computed(() => `sereno-shine-${uid}`);
</script>

<template>
  <div
    class="flex flex-col items-center justify-center gap-3"
    role="status"
    aria-live="polite"
    :aria-label="label || '加载中'"
  >
    <svg :width="size" :height="size" viewBox="0 0 100 100" role="img" aria-label="Sereno">
      <defs>
        <linearGradient :id="shineId" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#fff" stop-opacity="0" />
          <stop offset="0.5" stop-color="#fff" stop-opacity="0.55" />
          <stop offset="1" stop-color="#fff" stop-opacity="0" />
        </linearGradient>

        <mask :id="cutId" maskUnits="userSpaceOnUse" x="0" y="0" width="100" height="100">
          <rect width="100" height="100" fill="#fff" />
          <rect x="36" y="45.5" width="28" height="5" fill="#000" transform="rotate(65 50 48)" />
        </mask>

        <mask :id="logoMaskId" maskUnits="userSpaceOnUse" x="0" y="0" width="100" height="100">
          <rect width="100" height="100" fill="#000" />
          <path
            d="M 72 24 L 38 24 A 12 12 0 0 0 38 48 L 62 48 A 12 12 0 0 1 62 72 L 28 72"
            fill="none"
            stroke="#fff"
            stroke-width="14"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <rect x="36" y="45.5" width="28" height="5" fill="#000" transform="rotate(65 50 48)" />
        </mask>
      </defs>

      <path
        d="M 72 24 L 38 24 A 12 12 0 0 0 38 48 L 62 48 A 12 12 0 0 1 62 72 L 28 72"
        fill="none"
        stroke="#0F64B5"
        stroke-width="14"
        stroke-linecap="round"
        stroke-linejoin="round"
        :mask="`url(#${cutId})`"
      />

      <g :mask="`url(#${logoMaskId})`">
        <rect
          class="app-loading-shine"
          x="-30"
          y="0"
          width="30"
          height="100"
          :fill="`url(#${shineId})`"
        >
          <animateTransform
            attributeName="transform"
            type="translate"
            values="-30 0; 130 0; 130 0"
            keyTimes="0; 0.45; 1"
            dur="1.8s"
            repeatCount="indefinite"
            calcMode="spline"
            keySplines="0.4 0 0.2 1; 0 0 1 0"
          />
        </rect>
      </g>
    </svg>

    <span v-if="label" class="text-[12px] text-ink-muted">{{ label }}</span>
  </div>
</template>

<style scoped>
@media (prefers-reduced-motion: reduce) {
  .app-loading-shine {
    display: none;
  }
}
</style>
