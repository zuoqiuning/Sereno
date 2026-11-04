<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import { MoreHorizontal } from "@lucide/vue";

import AppIconButton from "@/components/common/AppIconButton.vue";
import type { MenuItem } from "./menu";

/**
 * 通用「更多操作」菜单。
 *
 * ## 为什么必须 Teleport
 *
 * 第一版把浮层写成卡片内的 `absolute`。卡片是 `overflow-hidden` 的，
 * 菜单一展开就被卡片边缘裁掉；`z-40` 也只在自己卡片的堆叠上下文里有效，
 * 于是出现实机截图里的那一幕——菜单糊在卡片上、被下一张卡片压住、文字互相重叠。
 *
 * 现在与 `AppSelect` 采用同一套方案：`Teleport` 到 body + `fixed` 定位 +
 * 高于一切业务层的 `z-index`，并给足不透明底色与投影。菜单永远是「浮在最上面的一层」。
 *
 * 定位：默认右对齐触发按钮；下方空间不足且上方更宽裕时自动向上展开，
 * 同时把最大高度收进可视区域，避免菜单底部跑出窗口。
 */
const MENU_WIDTH = 224;
const GAP = 6;

const props = withDefaults(
  defineProps<{
    items: MenuItem[];
    /** 无障碍标签，也是悬停提示 */
    label?: string;
    /** 浮层对齐方向 */
    align?: "left" | "right";
    disabled?: boolean;
  }>(),
  { label: "更多操作", align: "right", disabled: false },
);

const emit = defineEmits<{ select: [id: string] }>();

const open = ref(false);
const rootRef = ref<HTMLElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
/** 是否向上展开：决定入场动画的方向 */
const openUp = ref(false);
const anchorStyle = ref({ left: "0px", top: "0px", transform: "none" });
const panelMaxHeight = ref("360px");

function positionPanel(): void {
  const rect = rootRef.value?.getBoundingClientRect();
  if (!rect) return;

  const width = MENU_WIDTH;
  const spaceBelow = window.innerHeight - rect.bottom - GAP;
  const spaceAbove = rect.top - GAP;
  // 下方放不下 260px、且上方更宽敞时改为向上展开
  const up = spaceBelow < 260 && spaceAbove > spaceBelow;
  openUp.value = up;
  const left =
    props.align === "right"
      ? Math.max(8, Math.min(rect.right - width, window.innerWidth - width - 8))
      : Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));

  anchorStyle.value = {
    left: `${left}px`,
    top: up ? `${rect.top - GAP}px` : `${rect.bottom + GAP}px`,
    transform: up ? "translateY(-100%)" : "none",
  };
  panelMaxHeight.value = `${Math.max(160, Math.min(360, up ? spaceAbove : spaceBelow))}px`;
}

function toggle(): void {
  if (props.disabled) return;
  if (open.value) {
    open.value = false;
    return;
  }
  open.value = true;
  positionPanel();
  void nextTick(() => panelRef.value?.focus());
}

function close(): void {
  open.value = false;
}

function pick(item: MenuItem): void {
  if (item.disabled) return;
  close();
  emit("select", item.id);
}

function onPointerDown(event: PointerEvent): void {
  const target = event.target as Node;
  if (rootRef.value?.contains(target) || panelRef.value?.contains(target)) return;
  close();
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && open.value) close();
}

/** 页面滚动时收起浮层；浮层自身滚动不算。 */
function onScroll(event: Event): void {
  const target = event.target as Node | null;
  if (target && panelRef.value?.contains(target)) return;
  close();
}

watch(open, (value) => {
  if (value) {
    document.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeydown);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", onScroll, true);
  } else {
    document.removeEventListener("pointerdown", onPointerDown);
    window.removeEventListener("keydown", onKeydown);
    window.removeEventListener("resize", close);
    window.removeEventListener("scroll", onScroll, true);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onPointerDown);
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", close);
  window.removeEventListener("scroll", onScroll, true);
});
</script>

<template>
  <div ref="rootRef" class="inline-flex shrink-0">
    <AppIconButton
      :label="label"
      :disabled="disabled"
      :has-popup="true"
      :expanded="open"
      @click="toggle"
    >
      <MoreHorizontal :size="16" />
    </AppIconButton>
  </div>

  <Teleport to="body">
    <div v-if="open" class="fixed z-[80] w-56" :style="anchorStyle">
      <div
        ref="panelRef"
        role="menu"
        tabindex="-1"
        class="menu-panel overflow-y-auto p-1 focus:outline-none"
        :class="openUp ? 'menu-panel--up' : 'menu-panel--down'"
        :style="{ maxHeight: panelMaxHeight }"
        @keydown="onKeydown"
      >
        <template v-for="item in items" :key="item.id">
          <div v-if="item.separatorBefore" class="my-1 h-px bg-line/70" role="separator" />
          <button
            role="menuitem"
            type="button"
            class="menu-item"
            :class="item.danger ? 'is-danger' : ''"
            :disabled="item.disabled"
            @click="pick(item)"
          >
            <component :is="item.icon" v-if="item.icon" :size="14" class="shrink-0" />
            <span class="truncate">{{ item.label }}</span>
          </button>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.menu-panel {
  border: 1px solid color-mix(in srgb, var(--color-line) 78%, transparent);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  box-shadow:
    0 2px 6px -1px rgb(16 24 40 / 0.06),
    0 14px 36px -10px rgb(16 24 40 / 0.22);
}

/* 入场方向跟随展开方向：向下展开从上方 4px 落下，向上展开反之 */
.menu-panel--down {
  animation: menu-in-down var(--duration-fast) var(--ease-out-soft);
}

.menu-panel--up {
  animation: menu-in-up var(--duration-fast) var(--ease-out-soft);
}

@keyframes menu-in-down {
  from {
    opacity: 0;
    transform: translateY(-4px) scale(0.985);
  }
}

@keyframes menu-in-up {
  from {
    opacity: 0;
    transform: translateY(4px) scale(0.985);
  }
}

@media (prefers-reduced-motion: reduce) {
  .menu-panel--down,
  .menu-panel--up {
    animation: none;
  }
}

.menu-item {
  display: flex;
  width: 100%;
  height: 32px;
  align-items: center;
  gap: 10px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  font-size: var(--text-body);
  color: var(--color-ink-soft);
  text-align: left;
  transition:
    background-color var(--duration-fast) var(--ease-out-soft),
    color var(--duration-fast) var(--ease-out-soft);
}

.menu-item:hover:not(:disabled) {
  background: var(--alpha-ink-6);
  color: var(--color-ink);
}

.menu-item:focus-visible {
  outline: 2px solid var(--color-brand-500);
  outline-offset: -2px;
}

.menu-item.is-danger {
  color: var(--color-danger);
}

.menu-item.is-danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--color-danger) 9%, transparent);
  color: var(--color-danger);
}

.menu-item:disabled {
  cursor: not-allowed;
  opacity: 0.4;
}
</style>
