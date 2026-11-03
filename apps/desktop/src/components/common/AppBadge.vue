<script setup lang="ts">
/**
 * 状态标签 / 徽章。
 *
 * 用于 `1.21.1`、`NeoForge`、`已就绪`、`运行中` 这类短状态词。
 *
 * ## 设计约定
 *
 * - **浅底 + 纯色文字**：底色用 `*-soft` 或低透明度主色，文字用对应纯色。
 *   标签面积小、颜色必须克制——大面积铺满会让界面失焦。
 * - **圆角用 `--radius-xs`（4px）**：与设计体系里「徽标与气泡」一致。
 * - **`pulse` 呼吸灯**：只在「还在进行中」的状态上用。
 *   静止的标签用普通圆点即可，全站都在闪会让人焦虑。
 */
type Variant = "default" | "brand" | "success" | "warning" | "danger";
type Size = "sm" | "md";

withDefaults(
  defineProps<{
    /** 配色语义 */
    variant?: Variant;
    /** 描边风格（浅底换成细边框） */
    outline?: boolean;
    /** 是否显示呼吸灯（表示「进行中」） */
    pulse?: boolean;
    /** 尺寸 */
    size?: Size;
  }>(),
  {
    variant: "default",
    outline: false,
    pulse: false,
    size: "sm",
  },
);

/** 浅底配色 */
const colorMap: Record<Variant, string> = {
  default: "bg-ink/5 text-ink-soft",
  brand: "bg-brand-500/10 text-brand-500",
  success: "bg-success-soft text-success",
  warning: "bg-warning-soft text-warning",
  danger: "bg-danger-soft text-danger",
};

/** 描边配色 */
const outlineMap: Record<Variant, string> = {
  default: "border border-line/80 text-ink-soft",
  brand: "border border-brand-500/30 text-brand-500",
  success: "border border-success/30 text-success",
  warning: "border border-warning/30 text-warning",
  danger: "border border-danger/30 text-danger",
};
</script>

<template>
  <span
    class="inline-flex items-center justify-center gap-1.5 rounded-xs font-medium tracking-wide"
    :class="[
      size === 'sm' ? 'px-2 py-0.5 text-[11px]' : 'px-2.5 py-1 text-[12px]',
      outline ? outlineMap[variant] : colorMap[variant],
    ]"
  >
    <span v-if="pulse" class="relative flex h-1.5 w-1.5 shrink-0">
      <span
        class="absolute inline-flex h-full w-full animate-ping rounded-full bg-current opacity-75"
      />
      <span class="relative inline-flex h-1.5 w-1.5 rounded-full bg-current" />
    </span>
    <slot />
  </span>
</template>
