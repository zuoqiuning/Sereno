import { Boxes, Download, Ellipsis, ListTodo, Rocket, Settings, Users } from "@lucide/vue";
import type { Component } from "vue";

export interface NavItem {
  to: string;
  label: string;
  icon: Component;
}

export const allNavItems: NavItem[] = [
  { to: "/", label: "启动", icon: Rocket },
  { to: "/instances", label: "实例", icon: Boxes },
  { to: "/multiplayer", label: "联机", icon: Users },
  { to: "/downloads", label: "下载", icon: Download },
  { to: "/tasks", label: "任务", icon: ListTodo },
  { to: "/settings", label: "设置", icon: Settings },
  { to: "/more", label: "更多", icon: Ellipsis },
];
