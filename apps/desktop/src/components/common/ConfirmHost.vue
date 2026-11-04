<script setup lang="ts">
import { computed } from "vue";
import { AlertTriangle, HelpCircle } from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppModal from "@/components/common/AppModal.vue";
import { settleConfirm, useConfirmState } from "@/composables/useConfirm";

/**
 * 全局确认框宿主：挂在 `App.vue` 里一次即可。
 *
 * 业务代码只调用 `askConfirm(...)`，不关心弹窗怎么渲染——
 * 这样既避免了原生 `confirm()` 阻塞界面，也不用每个页面各写一份确认逻辑。
 */
const state = useConfirmState();

const confirmVariant = computed(() => (state.danger ? "danger" : "primary"));

function close(): void {
  settleConfirm(false);
}
</script>

<template>
  <AppModal :open="state.open" width="sm" @close="close">
    <template #header>
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg"
        :class="state.danger ? 'bg-danger/10 text-danger' : 'bg-brand-500/10 text-brand-500'"
      >
        <AlertTriangle v-if="state.danger" :size="18" />
        <HelpCircle v-else :size="18" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-h2 font-semibold leading-5 text-ink">{{ state.title }}</h2>
      </div>
    </template>

    <p v-if="state.message" class="whitespace-pre-line text-body leading-6 text-ink-soft">
      {{ state.message }}
    </p>

    <template #footer>
      <div class="flex justify-end gap-2">
        <AppButton variant="secondary" @click="close">{{ state.cancelText }}</AppButton>
        <AppButton :variant="confirmVariant" @click="settleConfirm(true)">
          {{ state.confirmText }}
        </AppButton>
      </div>
    </template>
  </AppModal>
</template>
