import { Boxes, Download, ListTodo, Rocket, Settings } from "@lucide/vue";
import type { Component } from "vue";

export interface NavItem {
  to: string;
  label: string;
  icon: Component;
}

export const allNavItems: NavItem[] = [
  { to: "/", label: "启动", icon: Rocket },
  { to: "/instances", label: "实例", icon: Boxes },
  { to: "/downloads", label: "下载", icon: Download },
  { to: "/tasks", label: "任务", icon: ListTodo },
  { to: "/settings", label: "设置", icon: Settings },
];
