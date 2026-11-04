<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { X } from "@lucide/vue";

import AppIconButton from "@/components/common/AppIconButton.vue";

/**
 * 统一模态框外壳。
 *
 * ## 结构
 *
 * 只负责「遮罩 / 面板 / 固定头部 / 固定工具栏 / 可滚动主体 / 固定底部」，
 * 内容全部由插槽决定。关闭行为（Esc、右上角按钮）集中在这里。
 *
 * ## 键盘与焦点（桌面应用的基本要求）
 *
 * - `role="dialog"` + `aria-modal`，读屏软件才知道「焦点被困在弹窗里」；
 * - 打开时把焦点移到第一个可聚焦元素，**Tab 在弹窗内循环**；
 * - 关闭时把焦点还给触发它的元素；
 * - 打开期间锁定 `body` 滚动（用计数，支持「确认框叠在弹窗上」）。
 *
 * ## 尺寸
 *
 * - `width` 控制面板最大宽度，详情类用 `lg`，表单类用 `sm`；
 * - `height="fixed"` 给出稳定高度，列表加载前后不会跳动。
 */
type ModalWidth = "sm" | "md" | "lg" | "xl";

const props = withDefaults(
  defineProps<{
    width?: ModalWidth;
    height?: "auto" | "fixed";
    padded?: boolean;
    /** 是否显示。默认 `true`，兼容「一直渲染」的用法。 */
    open?: boolean;
    /** 无障碍标题（优先用 header 里的 h2 文本，这里作为兜底） */
    ariaLabel?: string;
  }>(),
  {
    width: "md",
    height: "auto",
    padded: true,
    open: true,
    ariaLabel: "对话框",
  },
);

const emit = defineEmits<{
  (e: "close"): void;
}>();

const widthClass: Record<ModalWidth, string> = {
  sm: "max-w-[480px]",
  md: "max-w-[680px]",
  lg: "max-w-[900px]",
  xl: "max-w-[1080px]",
};

const panelRef = ref<HTMLElement | null>(null);
let previouslyFocused: HTMLElement | null = null;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** 同时打开的弹窗数量：确认框会叠在业务弹窗之上，滚动锁要用计数。 */
let openCount = 0;

/** 本实例是否真的锁过滚动。避免「关闭状态的弹窗」误减别人的计数。 */
let locked = false;

function lockScroll(): void {
  if (locked) return;
  locked = true;
  openCount += 1;
  if (openCount === 1) document.body.style.overflow = "hidden";
}

function unlockScroll(): void {
  if (!locked) return;
  locked = false;
  openCount = Math.max(0, openCount - 1);
  if (openCount === 0) document.body.style.overflow = "";
}

watch(
  () => props.open,
  async (open) => {
    if (open) {
      previouslyFocused =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      lockScroll();
      await nextTick();
      const first = panelRef.value?.querySelector<HTMLElement>(FOCUSABLE);
      (first ?? panelRef.value)?.focus();
    } else {
      unlockScroll();
      // 关闭后把焦点还给触发者，键盘用户不会「迷失在页面顶部」
      previouslyFocused?.focus?.();
      previouslyFocused = null;
    }
  },
  { immediate: true },
);

/** Tab 焦点陷阱：在面板首尾之间循环。 */
function onKeydown(event: KeyboardEvent): void {
  if (event.key !== "Tab" || !panelRef.value) return;

  const focusable = Array.from(panelRef.value.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (element) => element.offsetParent !== null || element === document.activeElement,
  );
  if (focusable.length === 0) {
    event.preventDefault();
    panelRef.value.focus();
    return;
  }

  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;

  if (event.shiftKey && (active === first || active === panelRef.value)) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && active === last) {
    event.preventDefault();
    first.focus();
  }
}

/** Esc 关闭。输入框里的 Esc 交给组件自己处理（例如清空搜索）。 */
function onKeydownCapture(event: KeyboardEvent): void {
  if (event.key !== "Escape" || !props.open) return;
  const target = event.target as HTMLElement | null;
  if (target && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;
  event.stopPropagation();
  emit("close");
}

onBeforeUnmount(() => {
  if (props.open) unlockScroll();
});

const panelClass = computed(() => [
  widthClass[props.width],
  props.height === "fixed" ? "h-[82vh] max-h-[720px]" : "max-h-[86vh]",
]);
</script>

<template>
  <Teleport to="body">
    <Transition name="sereno-modal" appear>
      <div
        v-show="open"
        class="pointer-events-auto fixed inset-0 z-50 flex items-center justify-center bg-ink/45 p-4 backdrop-blur-sm"
        @keydown.capture="onKeydownCapture"
        @keydown="onKeydown"
      >
        <div
          ref="panelRef"
          class="sereno-glass-strong sereno-modal-panel flex w-full flex-col overflow-hidden rounded-xl outline-none"
          :class="panelClass"
          role="dialog"
          aria-modal="true"
          :aria-label="ariaLabel"
          tabindex="-1"
        >
          <header class="flex shrink-0 items-center gap-3 border-b border-line/50 px-5 py-3">
            <slot name="header" />
            <AppIconButton label="关闭" size="sm" @click="emit('close')">
              <X :size="16" />
            </AppIconButton>
          </header>

          <div v-if="$slots.toolbar" class="shrink-0 border-b border-line/40 px-5 py-2">
            <slot name="toolbar" />
          </div>

          <div
            class="min-h-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-gutter:stable]"
            :class="padded ? 'px-5 py-4' : ''"
          >
            <slot />
          </div>

          <footer v-if="$slots.footer" class="shrink-0 border-t border-line/50 px-5 py-3">
            <slot name="footer" />
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
