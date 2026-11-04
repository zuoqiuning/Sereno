<script setup lang="ts">
/**
 * 设置项骨架：左标签（标题 + 说明）+ 右控件。
 *
 * 设置页与实例设置弹窗里，每一行都是「一个配置项」，之前却是各写各的：
 * 有的用 `flex justify-between`，有的用 `flex flex-col gap-1`，
 * 说明文字时而 11px 时而 12px，控件时而贴右时而占满整行。
 * 统一成这个模板后，所有设置项的节奏才一致。
 *
 * - `orientation="inline"`（默认）：左标签、右控件，用于开关 / 下拉 / 按钮；
 * - `orientation="stack"`：标签在上、控件占满一行，用于输入框 / 滑杆 / 路径。
 */
withDefaults(
  defineProps<{
    title: string;
    description?: string;
    orientation?: "inline" | "stack";
  }>(),
  { description: "", orientation: "inline" },
);
</script>

<template>
  <div class="settings-row" :class="`settings-row--${orientation}`">
    <div class="settings-row__label">
      <span class="settings-row__title">{{ title }}</span>
      <span v-if="description" class="settings-row__description">{{ description }}</span>
    </div>
    <div class="settings-row__control">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.settings-row {
  display: flex;
  gap: 16px;
  padding: 12px 0;
}

.settings-row--inline {
  align-items: center;
  justify-content: space-between;
}

.settings-row--stack {
  flex-direction: column;
  gap: 8px;
}

.settings-row + .settings-row,
:slotted(*) + .settings-row {
  border-top: 1px solid color-mix(in srgb, var(--color-line-soft) 90%, transparent);
}

.settings-row__label {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.settings-row__title {
  font-size: var(--text-body);
  font-weight: 500;
  color: var(--color-ink);
}

.settings-row__description {
  font-size: var(--text-micro);
  line-height: 1.5;
  color: var(--color-ink-muted);
}

.settings-row__control {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: 8px;
}

.settings-row--stack .settings-row__control {
  width: 100%;
  align-items: stretch;
}
</style>
