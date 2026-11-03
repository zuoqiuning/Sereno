import { reactive, ref } from "vue";

/**
 * 模态框的显示状态。
 *
 * 组件必须**活到出场动画播完**才会被卸载，否则淡出根本来不及播放，
 * 关闭会变成「啪」地消失。因此这里把「弹什么内容」和「现在显示吗」拆成两份状态：
 *
 * - `item`：弹窗内容（对象 / 详情），关闭后延时清空；
 * - `visible`：是否可见，关闭时立刻置 false 以触发过渡。
 *
 * 用法：
 *
 * ```ts
 * const backup = useModalState<InstanceMetadata>();
 * backup.open(instance);        // 打开
 * backup.close();               // 关闭（先淡出，再卸载）
 * ```
 */
export function useModalState<T>(exitDelay = 200) {
  const item = ref<T | null>(null);
  const visible = ref(false);

  function open(value: T): void {
    item.value = value;
    visible.value = true;
  }

  function close(): void {
    if (!visible.value) return;
    visible.value = false;
    window.setTimeout(() => {
      // 期间又被打开的话，就不要清空内容。
      if (!visible.value) item.value = null;
    }, exitDelay);
  }

  // 用 reactive 包一层：模板里 `state.item` 会自动拆包成值，
  // 不用在每个调用点写 `.value`。
  return reactive({ item, visible, open, close });
}
