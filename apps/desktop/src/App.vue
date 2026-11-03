<script setup lang="ts">
import { ref } from "vue";
import { RouterView } from "vue-router";

import ResizeHandles from "@/components/layout/ResizeHandles.vue";
import TitleBar from "@/components/layout/TitleBar.vue";

const scrollRoot = ref<HTMLElement | null>(null);

/**
 * 新页面开始入场时把外层滚动位置归零。
 * 不能监听路由变化立刻归零——那会在旧页面淡出期间把它跳到顶部，
 * 看起来就是切换时的“顿一下”。
 */
function resetScroll(): void {
  if (scrollRoot.value) scrollRoot.value.scrollTop = 0;
}
</script>

<template>
  <div class="sereno-window relative flex h-screen flex-col overflow-hidden bg-canvas">
    <!-- 悬浮工具条 -->
    <TitleBar />

    <!--
      必须用 flex-1 min-h-0，并且去掉 h-full 这种依赖父级百分比高度的写法。
      因为 .sereno-scroll 已经是 flex: 1 1 auto，子组件直接在里面 h-full
      有时在 WebView2 下解析不出高度，会导致内部 flex-col 布局的
      overflow-y-auto 失效，连带左侧栏和顶部 Tabs 被挤成 0 高。
      改用 block 包裹并显式提供 100% 尺寸。
    -->
    <main ref="scrollRoot" class="sereno-scroll relative flex flex-col pt-12">
      <div class="h-full w-full flex-1">
        <RouterView v-slot="{ Component, route }">
          <Transition name="page" mode="out-in" @before-enter="resetScroll">
            <component :is="Component" :key="route.path" />
          </Transition>
        </RouterView>
      </div>
    </main>

    <ResizeHandles />
  </div>
</template>
