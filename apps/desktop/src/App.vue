<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterView } from "vue-router";

import ResizeHandles from "@/components/layout/ResizeHandles.vue";
import TitleBar from "@/components/layout/TitleBar.vue";
import ConfirmHost from "@/components/common/ConfirmHost.vue";
import CommandPalette from "@/components/layout/CommandPalette.vue";
import DropOverlay from "@/components/common/DropOverlay.vue";
import DropImportModal from "@/components/instances/DropImportModal.vue";
import ToastStack from "@/components/common/ToastStack.vue";
import UpdateCard from "@/components/common/UpdateCard.vue";
import { transitionName } from "@/composables/useNavDirection";
import { useKeyboardShortcuts } from "@/composables/useKeyboardShortcuts";
import { clearDroppedFiles, droppedPaths, useFileDrop } from "@/composables/useFileDrop";
import { useUpdateCheck } from "@/composables/useUpdateCheck";
import { useUpdateStore } from "@/stores/update";
import { registerAutoRestart } from "@/composables/autoRestart";

const updateStore = useUpdateStore();

// 全局快捷键：Ctrl/Cmd+1~6 切页面、Ctrl/Cmd+K 命令面板、Ctrl/Cmd+, 打开设置
useKeyboardShortcuts();

// 全局拖放：把 .jar / .zip 从桌面拖进窗口即可导入实例
useFileDrop();

// 崩溃自动重启（设置里可开关，单实例最多 3 次）——必须在应用级监听，
// 游戏退出事件在任何页面都可能到达
registerAutoRestart();

/**
 * 更新提示接线。
 *
 * Rust 侧在启动画面阶段就在后台检查更新了（**不等结果**），
 * 有新版了通过 `update-available` 事件通知这里。
 * 收到后写进全局 store，由 `UpdateCard` 渲染。
 */
useUpdateCheck((payload) => {
  updateStore.show({
    latestVersion: payload.latestVersion,
    currentVersion: payload.currentVersion,
    notes: payload.notes,
  });
});

/** 有待处理的文件时自动打开导入对话框。 */
const dropModalOpen = computed(() => droppedPaths.value.length > 0);

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

    <!-- 全局更新卡片：有新版本时出现，直到用户处理或关掉 -->
    <UpdateCard />

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
          <!-- 方向由主导航顺序决定：向右切页从右侧推入，8px 位移足够表达方向 -->
          <Transition :name="transitionName()" mode="out-in" @before-enter="resetScroll">
            <component :is="Component" :key="route.path" />
          </Transition>
        </RouterView>
      </div>
    </main>

    <ResizeHandles />

    <!-- 全局通知：顶部居中，按队列进出，退场时其余卡片平滑补位 -->
    <ToastStack />

    <!-- 全局确认框：业务代码 await askConfirm(...) 即可，不阻塞渲染 -->
    <ConfirmHost />

    <!-- 命令面板：Ctrl/Cmd+K 唤出，一条路径覆盖切页面与跳设置项 -->
    <CommandPalette />

    <!-- 拖放导入：悬停时给提示，落下后问装到哪个实例 -->
    <DropOverlay />
    <DropImportModal :open="dropModalOpen" :paths="droppedPaths" @close="clearDroppedFiles" />
  </div>
</template>
