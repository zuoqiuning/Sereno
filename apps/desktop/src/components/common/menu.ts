import type { Component } from "vue";

/**
 * 下拉菜单项。
 *
 * 放在 `.ts` 而不是 `.vue` 里：`<script setup>` 不允许出现 ES 导出，
 * 从 SFC 里导出接口会让 Vite 编译失败（while 类型检查不会报）。
 */
export interface MenuItem {
  id: string;
  label: string;
  icon?: Component;
  /** 危险操作（删除类），用红色文字 */
  danger?: boolean;
  disabled?: boolean;
  /** 在这一项之前画一条分隔线，用来分组 */
  separatorBefore?: boolean;
}
