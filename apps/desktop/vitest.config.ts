import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";

/**
 * Vitest 配置。
 *
 * 与 `vite.config.ts` 分开：生产构建不需要测试相关的配置，
 * 混在一起会让 `vite build` 拖上无用依赖。
 *
 * `environment: "node"` —— 当前测试只覆盖纯函数工具（format / throttle），
 * 不需要 DOM。将来测组件时再换成 `jsdom`（需另装 `jsdom` 依赖）。
 */
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    reporters: "default",
  },
});
