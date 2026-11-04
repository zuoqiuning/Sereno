<script setup lang="ts">
import type { Component } from "vue";

import AppButton from "@/components/common/AppButton.vue";

/**
 * 统一空状态。
 *
 * 之前各处手写「一个大图标 + 一句开发中 / 没有数据」：图标大小从 28 到 40、
 * 标题字号从 12 到 14、留白从 py-6 到 h-48，看起来每处都是临时拼的。
 * 空状态是用户**最常看到**的界面之一（新用户第一次进每个页面都会看到），
 * 必须和正常内容一样被认真对待。
 */
withDefaults(
  defineProps<{
    icon?: Component;
    title?: string;
    description?: string;
    /** 给出可执行动作时，空状态才不是死路 */
    actionLabel?: string;
  }>(),
  { icon: undefined, title: "", description: "", actionLabel: "" },
);

defineEmits<{ action: [] }>();
</script>

<template>
  <div class="empty-state">
    <div v-if="icon" class="empty-state__icon">
      <component :is="icon" :size="28" :stroke-width="1.6" />
    </div>
    <div class="empty-state__text">
      <p class="empty-state__title">{{ title }}</p>
      <p v-if="description" class="empty-state__description">{{ description }}</p>
    </div>
    <AppButton v-if="actionLabel" size="sm" @click="$emit('action')">
      {{ actionLabel }}
    </AppButton>
  </div>
</template>

<style scoped>
.empty-state {
  display: flex;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  padding: 48px 24px;
  text-align: center;
}

/* 图标块：淡品牌底 + 一圈极淡的光环。
   空状态往往是新用户看到的第一屏，它不能是「一个灰色线框 + 一行字」——
   那是「功能没做完」的观感，而不是「这里暂时没有内容」。 */
.empty-state__icon {
  display: flex;
  height: 64px;
  width: 64px;
  align-items: center;
  justify-content: center;
  border-radius: 20px;
  background: var(--color-surface-muted);
  color: color-mix(in srgb, var(--color-ink-muted) 78%, transparent);
}

.empty-state__text {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.empty-state__title {
  font-size: var(--text-h2);
  font-weight: 600;
  color: var(--color-ink);
}

.empty-state__description {
  max-width: 320px;
  font-size: var(--text-caption);
  line-height: 1.55;
  color: var(--color-ink-muted);
}
</style>
