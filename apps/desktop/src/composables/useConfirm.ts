import { reactive } from "vue";

/**
 * 全局确认框。
 *
 * ## 为什么不用 `window.confirm`
 *
 * WebView2 里的原生 `confirm()` 会**阻塞渲染线程**（界面在等待期间完全冻结），
 * 样式也不受控，与整套玻璃拟态视觉完全割裂。更糟的是它无法本地化按钮文案，
 * 也不能表达「危险操作」的语义。
 *
 * 这里改成「Promise 化的全局单例」：任何模块 `await askConfirm(...)` 即可，
 * 不需要在每个组件里挂一个弹窗实例；真正的渲染由 `ConfirmHost.vue` 统一负责。
 */

export interface ConfirmOptions {
  title: string;
  /** 补充说明，支持换行。 */
  message?: string;
  confirmText?: string;
  cancelText?: string;
  /** 危险操作：确认按钮用 destructive 配色。 */
  danger?: boolean;
}

interface ConfirmState extends Required<Omit<ConfirmOptions, "message">> {
  open: boolean;
  message: string;
  resolve: ((value: boolean) => void) | null;
}

const state = reactive<ConfirmState>({
  open: false,
  title: "",
  message: "",
  confirmText: "确定",
  cancelText: "取消",
  danger: false,
  resolve: null,
});

/** 弹出确认框，返回用户是否确认。 */
export function askConfirm(options: ConfirmOptions | string): Promise<boolean> {
  // 上一个还没结算就被新请求覆盖时，按「取消」结算，避免 Promise 永远挂着
  state.resolve?.(false);

  const normalized: ConfirmOptions = typeof options === "string" ? { title: options } : options;

  state.title = normalized.title;
  state.message = normalized.message ?? "";
  state.confirmText = normalized.confirmText ?? "确定";
  state.cancelText = normalized.cancelText ?? "取消";
  state.danger = normalized.danger ?? false;
  state.open = true;

  return new Promise<boolean>((resolve) => {
    state.resolve = resolve;
  });
}

/** 由 `ConfirmHost` 调用：结算并关闭。 */
export function settleConfirm(value: boolean): void {
  const resolve = state.resolve;
  state.resolve = null;
  state.open = false;
  resolve?.(value);
}

/** 供宿主组件读取状态。 */
export function useConfirmState(): ConfirmState {
  return state;
}
