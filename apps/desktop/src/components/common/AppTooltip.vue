<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId } from "vue";

/**
 * 悬停气泡。
 *
 * 深色 HUD 气泡，350ms 进入延迟、离场即隐，聚焦时同样显示。
 *
 * **界面禁止用原生 `title` 属性当 tooltip**——原生气泡样式不可控，
 * 且在无边框窗口里表现很差。
 */
type Placement = "top" | "bottom" | "left" | "right";

const props = withDefaults(
  defineProps<{
    content: string;
    placement?: Placement;
    /** 进入延迟（毫秒） */
    delay?: number;
  }>(),
  {
    placement: "top",
    delay: 350,
  },
);

const visible = ref(false);
const tipId = useId();

let timer: ReturnType<typeof setTimeout> | null = null;

function show(): void {
  if (timer !== null) {
    clearTimeout(timer);
  }
  timer = setTimeout(() => {
    visible.value = true;
  }, props.delay);
}

function hide(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  visible.value = false;
}

onBeforeUnmount(() => {
  if (timer !== null) {
    clearTimeout(timer);
  }
});

const placementClass = computed(() => {
  switch (props.placement) {
    case "bottom":
      return "top-full left-1/2 mt-2 -translate-x-1/2";
    case "left":
      return "right-full top-1/2 mr-2 -translate-y-1/2";
    case "right":
      return "left-full top-1/2 ml-2 -translate-y-1/2";
    case "top":
    default:
      return "bottom-full left-1/2 mb-2 -translate-x-1/2";
  }
});
</script>

<template>
  <span
    class="relative inline-flex"
    :aria-describedby="visible ? tipId : undefined"
    @mouseenter="show"
    @mouseleave="hide"
    @focusin="show"
    @focusout="hide"
  >
    <slot />

    <Transition
      enter-active-class="transition-opacity duration-150"
      enter-from-class="opacity-0"
      leave-active-class="transition-opacity duration-100"
      leave-to-class="opacity-0"
    >
      <span
        v-if="visible"
        :id="tipId"
        role="tooltip"
        class="pointer-events-none absolute z-50 whitespace-nowrap rounded-xs bg-ink px-2.5 py-1.5 text-micro text-canvas shadow-lg"
        :class="placementClass"
      >
        {{ content }}
      </span>
    </Transition>
  </span>
</template>
