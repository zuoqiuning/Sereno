<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ChevronLeft, ChevronRight } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";

/**
 * 分页器：页码按钮 + 明确的“跳至 X 页”输入。
 *
 * 页码永远包含首尾页，并在中间自动插入省略号；跳转输入带单位，回车或点「前往」
 * 都能生效。页码多到几千页时也不会退化成一排密密麻麻的小方块。
 */
const props = withDefaults(
  defineProps<{
    page: number;
    pageCount: number;
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{
  (e: "change", page: number): void;
}>();

type PageToken = number | "gap";

const jumpInput = ref(String(props.page));

watch(
  () => props.page,
  (page) => {
    jumpInput.value = String(page);
  },
);

const tokens = computed<PageToken[]>(() => {
  const last = Math.max(1, props.pageCount);
  if (last <= 9) {
    return Array.from({ length: last }, (_, index) => index + 1);
  }

  const pages = new Set<number>([1, last, props.page]);
  if (props.page > 1) pages.add(props.page - 1);
  if (props.page < last) pages.add(props.page + 1);
  if (props.page <= 4) {
    for (let page = 2; page <= 5; page += 1) pages.add(page);
  }
  if (props.page >= last - 3) {
    for (let page = last - 4; page < last; page += 1) pages.add(page);
  }

  const sorted = [...pages].filter((page) => page >= 1 && page <= last).sort((a, b) => a - b);

  const result: PageToken[] = [];
  for (const page of sorted) {
    const previous = result[result.length - 1];
    if (typeof previous === "number" && page - previous > 1) {
      result.push("gap");
    }
    result.push(page);
  }
  return result;
});

function go(target: number): void {
  const last = Math.max(1, props.pageCount);
  const clamped = Math.min(Math.max(1, target), last);
  if (clamped !== props.page) {
    emit("change", clamped);
  } else {
    jumpInput.value = String(props.page);
  }
}

function jump(): void {
  const parsed = Number.parseInt(jumpInput.value, 10);
  if (!Number.isFinite(parsed)) {
    jumpInput.value = String(props.page);
    return;
  }
  go(parsed);
}
</script>

<template>
  <div class="flex flex-wrap items-center gap-1">
    <AppButton variant="ghost" size="sm" :disabled="disabled || page <= 1" @click="go(page - 1)">
      <ChevronLeft :size="14" />
      上一页
    </AppButton>

    <template v-for="(token, index) in tokens" :key="`${token}-${index}`">
      <span v-if="token === 'gap'" class="px-1 text-caption text-ink-muted">…</span>
      <button
        v-else
        type="button"
        :disabled="disabled"
        :aria-current="token === page ? 'page' : undefined"
        :aria-label="`第 ${token} 页`"
        class="h-8 min-w-8 rounded-md px-1.5 text-caption font-medium transition-colors disabled:pointer-events-none disabled:opacity-40"
        :class="
          token === page ? 'bg-brand-500 text-white' : 'text-ink-soft hover:bg-ink/5 hover:text-ink'
        "
        @click="go(token)"
      >
        {{ token }}
      </button>
    </template>

    <AppButton
      variant="ghost"
      size="sm"
      :disabled="disabled || page >= pageCount"
      @click="go(page + 1)"
    >
      下一页
      <ChevronRight :size="14" />
    </AppButton>

    <span class="mx-1 hidden h-4 w-px bg-line/60 sm:block" />

    <div class="flex items-center gap-1.5">
      <span class="text-caption text-ink-muted">跳至</span>
      <input
        v-model="jumpInput"
        type="number"
        min="1"
        :max="pageCount"
        :disabled="disabled"
        class="h-8 w-14 rounded-md border border-line/80 bg-surface px-2 text-center text-caption text-ink outline-none transition-colors focus:border-brand-500 disabled:opacity-40"
        @keydown.enter="jump"
      />
      <span class="text-caption text-ink-muted">页</span>
      <AppButton variant="secondary" size="sm" :disabled="disabled" @click="jump"> 前往 </AppButton>
    </div>
  </div>
</template>
