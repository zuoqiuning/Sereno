import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// 端口在四处出现，必须同时改：
//   1. 这里的 server.port
//   2. src-tauri/tauri.conf.json 的 build.devUrl
//   3. scripts/dev.mjs 的 PORT
//   4. vite.config.ts 的 server.host（必须写 127.0.0.1，不能写 localhost）
const PORT = 5173;

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Tauri 使用固定端口加载开发页面；strictPort 保证端口被占时直接报错，
  // 而不是偷偷顺延到 5174 —— 一旦顺延，tauri.conf.json 的 devUrl 仍指向
  // 5173，Tauri 会白屏。
  server: {
    host: "127.0.0.1",
    port: PORT,
    strictPort: true,
    // 关闭 sourcemap 映射的额外开销，dev 下保留 HMR 即可
    hmr: true,
    watch: {
      // target/ 与 dist/ 由 Rust / 构建产物产生，监听它们只会白烧 CPU
      ignored: ["**/src-tauri/**", "**/dist/**", "**/target/**"],
    },
  },
  // 预览与生产构建
  build: {
    target: "esnext",
    sourcemap: false,
    chunkSizeWarningLimit: 1500,
    // 启动画面是独立窗口，需要单独产出 splash.html
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        splash: fileURLToPath(new URL("./splash.html", import.meta.url)),
      },
    },
  },
  clearScreen: false,
});
