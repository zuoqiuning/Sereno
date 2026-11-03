<script setup lang="ts">
import { watch } from "vue";
import { X } from "@lucide/vue";

/**
 * 统一模态框外壳。
 *
 * 只负责「遮罩 / 面板 / 固定头部 / 固定工具栏 / 可滚动主体 / 固定底部」，
 * 具体内容全部由插槽决定。关闭行为（Esc、点遮罩、右上角按钮）集中在这里，
 * 各业务弹窗不再重复写一遍。
 *
 * 尺寸：
 * - `width` 控制面板最大宽度，详情类用 `lg`，表单类用 `sm`；
 * - `height="fixed"` 给出稳定高度，列表加载前后不会跳动；
 * - `padded=false` 适合自己管理内边距的页面（例如自带工具栏的详情页）。
 *
 * ## 显隐与动画
 *
 * `open` 决定弹窗是否显示。用 `open`（而不是父组件的 `v-if`）才能播放**出场动画**：
 * 父组件直接 `v-if` 卸载组件的话，组件连同 DOM 一起消失，过渡根本没有机会执行。
 * 因此调用方应写成 `<AppModal :open="visible" @close="visible = false">`。
 */
type ModalWidth = "sm" | "md" | "lg" | "xl";

const props = withDefaults(
  defineProps<{
    width?: ModalWidth;
    height?: "auto" | "fixed";
    padded?: boolean;
    /** 是否显示。默认 `true`，兼容「一直渲染」的用法。 */
    open?: boolean;
  }>(),
  {
    width: "md",
    height: "auto",
    padded: true,
    open: true,
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

// 关闭行为只保留右上角叉号与弹窗内部的「取消」按钮：
// 点遮罩不再关闭，避免拖动选择文本或误触时丢掉正在填写的内容。

// 关闭时把焦点还给页面，避免键盘操作「卡」在已经消失的弹窗里。
watch(
  () => props.open,
  (open) => {
    if (!open && document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  },
);
</script>

<template>
  <Teleport to="body">
    <Transition name="sereno-modal" appear>
      <div
        v-show="open"
        class="pointer-events-auto fixed inset-0 z-50 flex items-center justify-center bg-ink/45 p-4 backdrop-blur-sm"
      >
      <div
        class="sereno-glass-strong sereno-modal-panel flex w-full flex-col overflow-hidden rounded-xl"
        :class="[
          widthClass[props.width],
          props.height === 'fixed' ? 'h-[82vh] max-h-[720px]' : 'max-h-[86vh]',
        ]"
      >
        <header class="flex shrink-0 items-center gap-3 border-b border-line/50 px-5 py-3">
          <slot name="header" />
          <button
            type="button"
            aria-label="关闭"
            class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-ink/6 hover:text-ink"
            @click="emit('close')"
          >
            <X :size="16" />
          </button>
        </header>

        <div v-if="$slots.toolbar" class="shrink-0 border-b border-line/40 px-5 py-2.5">
          <slot name="toolbar" />
        </div>

        <div
          class="min-h-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-gutter:stable]"
          :class="props.padded ? 'px-5 py-4' : ''"
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
