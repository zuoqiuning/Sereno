<script setup lang="ts">
import { X } from "@lucide/vue";

/**
 * 单行文本输入框。
 *
 * - **focus**：边框先变（140ms），外圈光晕后到（200ms）——两个不同的时长
 *   才形成「焦点扩散」的层次；同一个时长会看起来「一起亮起来」，很平。
 * - `clearable`：有内容时右侧出现清除按钮，搜索框不必反复按退格。
 * - `invalid`：危险色描边，用于表单校验失败。
 * - `#prefix` 插槽：左侧图标（搜索、账户名等），自动让出内边距。
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
    /** 有内容时显示清除按钮 */
    clearable?: boolean;
    /** 校验失败态 */
    invalid?: boolean;
  }>(),
  {
    modelValue: "",
    placeholder: "",
    disabled: false,
    type: "text",
    clearable: false,
    invalid: false,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  /** 回车触发（搜索框常用） */
  (e: "submit"): void;
  /** 点击清除按钮 */
  (e: "clear"): void;
}>();

function onInput(event: Event): void {
  emit("update:modelValue", (event.target as HTMLInputElement).value);
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Enter") {
    emit("submit");
  }
}

function clear(): void {
  emit("update:modelValue", "");
  emit("clear");
}
</script>

<template>
  <div class="relative">
    <span
      v-if="$slots.prefix"
      class="pointer-events-none absolute left-2.5 top-1/2 z-10 -translate-y-1/2 text-ink-muted"
    >
      <slot name="prefix" />
    </span>
    <input
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      class="app-input h-9 w-full rounded-md border text-body text-ink placeholder:text-ink-muted/60 disabled:cursor-not-allowed disabled:bg-surface-muted disabled:text-ink-muted"
      :class="[
        /*
          水平内边距必须**始终**有值。
          之前只在「有 prefix」或「可清除」时才加 pl-8 / pr-8，
          其余情况整个输入框没有任何内边距，文字直接贴着边框——这就是截图里的问题。
          这里按条件直接给最终值，避免 px 与 pl/pr 互相覆盖。
        */
        $slots.prefix ? 'pl-8' : 'pl-3',
        clearable && modelValue && !disabled ? 'pr-8' : 'pr-3',
        invalid ? 'is-invalid' : '',
      ]"
      @input="onInput"
      @keydown="onKeydown"
    />
    <button
      v-if="clearable && modelValue && !disabled"
      type="button"
      aria-label="清除"
      class="absolute right-1.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-sm text-ink-muted transition-fast hover:bg-ink/6 hover:text-ink"
      @click="clear"
    >
      <X :size="13" />
    </button>
  </div>
</template>

<style scoped>
.app-input {
  border-color: color-mix(in srgb, var(--color-line) 90%, transparent);
  /* 默认是极浅的灰底（像一张纸），聚焦时才变成纯白——层次靠底色而不是靠边框 */
  background: var(--color-surface-muted);
  outline: none;
  transition:
    border-color var(--duration-fast) var(--ease-out-soft),
    box-shadow var(--duration-base) var(--ease-out-soft),
    background-color var(--duration-fast) var(--ease-out-soft);
}

.app-input:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--color-brand-500) 40%, transparent);
}

.app-input:focus {
  border-color: var(--color-brand-500);
  background: var(--color-surface);
  /* 用 box-shadow 而不是 Tailwind ring：这样才能给「边框」和「光晕」分别设置时长 */
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-brand-500) 14%, transparent);
}

.app-input.is-invalid {
  border-color: var(--color-danger);
}

.app-input.is-invalid:focus {
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-danger) 18%, transparent);
}
</style>
