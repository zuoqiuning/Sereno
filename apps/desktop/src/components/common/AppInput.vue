<script setup lang="ts">
/**
 * 单行文本输入框。
 *
 * - **focus**：边框转鸢尾蓝 + 一圈 20% 透明度的同色光晕（`ring`）。
 *   焦点状态必须比常态明显一档，否则用户不知道光标在哪。
 * - **placeholder** 用 `text-ink-muted/60`，比正文弱但仍可读。
 * - **disabled**：底色换成 `surface-muted` 并降低文字对比，
 *   和 `disabled` 的原生光标样式一起说明「现在不能输入」。
 *
 * 支持 `v-model`（`modelValue` + `update:modelValue`）。
 */
withDefaults(
  defineProps<{
    /** 绑定值 */
    modelValue?: string;
    /** 占位提示 */
    placeholder?: string;
    /** 是否禁用 */
    disabled?: boolean;
    /** 原生 input 类型 */
    type?: "text" | "password" | "search" | "number" | "url";
  }>(),
  {
    modelValue: "",
    placeholder: "",
    disabled: false,
    type: "text",
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  /** 回车触发（搜索框常用） */
  (e: "submit"): void;
}>();

function onInput(event: Event): void {
  emit("update:modelValue", (event.target as HTMLInputElement).value);
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Enter") {
    emit("submit");
  }
}
</script>

<template>
  <input
    :type="type"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    class="h-9 w-full rounded-md border border-line/80 bg-surface px-3 text-[14px] text-ink transition-all duration-200 placeholder:text-ink-muted/60 focus:border-brand-500 focus:outline-none focus:ring-2 focus:ring-brand-500/20 disabled:cursor-not-allowed disabled:bg-surface-muted disabled:text-ink-muted"
    @input="onInput"
    @keydown="onKeydown"
  />
</template>
