<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { Check, ChevronDown } from "@lucide/vue";

/**
 * 自研下拉选择。
 *
 * 原生 `<select>` 在 WebView2 里会弹出系统菜单，字号、圆角、悬停态都跟界面
 * 完全脱节。这里用按钮 + 浮层自己实现，外观与 AppInput / AppButton 同一套 token。
 *
 * 浮层通过 `Teleport` 挂到 body 并用 fixed 定位：模态框内部通常是
 * `overflow-y-auto`，内联浮层会被裁掉。
 */
export interface SelectOption {
  value: string | number;
  label: string;
}

const props = withDefaults(
  defineProps<{
    modelValue?: string | number;
    options: SelectOption[];
    placeholder?: string;
    disabled?: boolean;
    size?: "sm" | "md";
  }>(),
  {
    modelValue: "",
    placeholder: "请选择",
    disabled: false,
    size: "md",
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string | number): void;
  (e: "change", value: string | number): void;
}>();

const open = ref(false);
const activeIndex = ref(-1);
const triggerRef = ref<HTMLButtonElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
const panelStyle = ref({
  left: "0px",
  top: "0px",
  width: "0px",
  transform: "none",
});

const selected = computed(() =>
  props.options.find((option) => option.value === props.modelValue) ?? null,
);

const sizeClass = computed(() =>
  props.size === "sm" ? "h-8 px-2.5 text-caption" : "h-9 px-3 text-body",
);

function positionPanel(): void {
  const rect = triggerRef.value?.getBoundingClientRect();
  if (!rect) return;
  const width = Math.max(rect.width, 160);
  const below = window.innerHeight - rect.bottom;
  const openUp = below < 220 && rect.top > 220;

  panelStyle.value = {
    left: `${Math.min(rect.left, window.innerWidth - width - 8)}px`,
    top: openUp ? `${rect.top - 4}px` : `${rect.bottom + 4}px`,
    width: `${width}px`,
    transform: openUp ? "translateY(-100%)" : "none",
  };
}

function toggle(): void {
  if (props.disabled) return;
  if (open.value) {
    open.value = false;
    return;
  }
  activeIndex.value = props.options.findIndex((option) => option.value === props.modelValue);
  open.value = true;
  positionPanel();
  void nextTick(() => panelRef.value?.focus());
}

function close(): void {
  open.value = false;
}

function choose(option: SelectOption): void {
  emit("update:modelValue", option.value);
  emit("change", option.value);
  close();
  triggerRef.value?.focus();
}

function move(step: number): void {
  if (props.options.length === 0) return;
  activeIndex.value = (activeIndex.value + step + props.options.length) % props.options.length;
  void nextTick(() => {
    panelRef.value
      ?.querySelector(`[data-index="${activeIndex.value}"]`)
      ?.scrollIntoView({ block: "nearest" });
  });
}

function onKeydown(event: KeyboardEvent): void {
  if (props.disabled) return;

  if (!open.value) {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggle();
    }
    return;
  }

  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      move(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      move(-1);
      break;
    case "Enter": {
      event.preventDefault();
      const option = props.options[activeIndex.value];
      if (option) choose(option);
      break;
    }
    case "Escape":
      event.preventDefault();
      close();
      triggerRef.value?.focus();
      break;
  }
}

function onPointerDown(event: MouseEvent): void {
  const target = event.target as Node;
  if (triggerRef.value?.contains(target) || panelRef.value?.contains(target)) return;
  close();
}

/** 页面滚动时收起浮层；浮层内部滚动不算。 */
function onScroll(event: Event): void {
  const target = event.target as Node | null;
  if (target && panelRef.value?.contains(target)) return;
  close();
}

watch(open, (value) => {
  if (value) {
    document.addEventListener("mousedown", onPointerDown);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", onScroll, true);
  } else {
    document.removeEventListener("mousedown", onPointerDown);
    window.removeEventListener("resize", close);
    window.removeEventListener("scroll", onScroll, true);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener("mousedown", onPointerDown);
  window.removeEventListener("resize", close);
  window.removeEventListener("scroll", onScroll, true);
});
</script>

<template>
  <button
    ref="triggerRef"
    type="button"
    :disabled="disabled"
    class="flex w-full select-none items-center justify-between gap-2 rounded-md border border-line/80 bg-surface text-left text-ink transition-all duration-200 focus-visible:border-brand-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/20 disabled:cursor-not-allowed disabled:bg-surface-muted disabled:text-ink-muted"
    :class="[
      sizeClass,
      open ? 'border-brand-500 ring-2 ring-brand-500/20' : 'hover:border-brand-500/40',
    ]"
    :aria-expanded="open"
    aria-haspopup="listbox"
    @click="toggle"
    @keydown="onKeydown"
  >
    <span class="truncate" :class="selected ? '' : 'text-ink-muted/70'">
      {{ selected?.label ?? placeholder }}
    </span>
    <ChevronDown
      :size="14"
      class="shrink-0 text-ink-muted transition-transform duration-200"
      :class="open ? 'rotate-180 text-brand-500' : ''"
    />
  </button>

  <Teleport to="body">
    <Transition name="select-pop">
      <div
        v-if="open"
        ref="panelRef"
        class="select-panel fixed z-[70] max-h-65 overflow-y-auto rounded-lg border border-line/70 bg-surface/95 p-1 shadow-[0_2px_6px_-1px_rgba(16,24,40,0.06),0_16px_40px_-12px_rgba(16,24,40,0.24)] backdrop-blur-xl focus:outline-none"
        :style="panelStyle"
        role="listbox"
        tabindex="-1"
        @keydown="onKeydown"
      >
        <div v-if="options.length === 0" class="px-3 py-2 text-caption text-ink-muted">
          没有可选项
        </div>
        <button
          v-for="(option, index) in options"
          :key="option.value"
          type="button"
          :data-index="index"
          class="flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-caption transition-colors"
          :class="
            index === activeIndex
              ? 'bg-brand-500/10 text-brand-600'
              : option.value === modelValue
                ? 'text-ink'
                : 'text-ink-soft hover:bg-ink/5'
          "
          @click="choose(option)"
          @mouseenter="activeIndex = index"
        >
          <span class="truncate">{{ option.label }}</span>
          <Check v-if="option.value === modelValue" :size="13" class="shrink-0 text-brand-500" />
        </button>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.select-pop-enter-active,
.select-pop-leave-active {
  transition:
    opacity 140ms ease,
    transform 160ms cubic-bezier(0.22, 1, 0.36, 1);
}

.select-pop-enter-from,
.select-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}
</style>
