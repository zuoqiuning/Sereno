import js from "@eslint/js";
import tseslint from "typescript-eslint";
import pluginVue from "eslint-plugin-vue";
import vueParser from "vue-eslint-parser";
import prettierConfig from "eslint-config-prettier";

/**
 * 图标与 UI 组件库的禁止清单。
 *
 * 图标只有 `@lucide/vue` 一个来源；UI 组件库一律自研于 `components/common/`。
 */
const RESTRICTED_LIBRARIES = [
  { name: "lucide-vue-next", message: "已弃用，请使用 @lucide/vue" },
  { name: "@heroicons/vue", message: "图标只允许来自 @lucide/vue" },
  { name: "@iconify/vue", message: "图标只允许来自 @lucide/vue" },
  { name: "primevue", message: "不引入 UI 组件库，组件自研于 components/common" },
  { name: "vuetify", message: "不引入 UI 组件库，组件自研于 components/common" },
  { name: "element-plus", message: "不引入 UI 组件库，组件自研于 components/common" },
  { name: "naive-ui", message: "不引入 UI 组件库，组件自研于 components/common" },
];

export default tseslint.config(
  {
    ignores: ["dist/**", "node_modules/**", "src-tauri/**", "src/bindings.ts"],
  },

  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs["flat/recommended"],

  {
    files: ["**/*.vue"],
    languageOptions: {
      parser: vueParser,
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: [".vue"],
        sourceType: "module",
      },
    },
    rules: {
      "vue/multi-word-component-names": "off",
      "vue/max-attributes-per-line": "off",
      "vue/singleline-html-element-content-newline": "off",
      "vue/html-self-closing": "off",
      "vue/html-indent": "off",
      "vue/html-closing-bracket-newline": "off",
      "vue/attributes-order": "off",
    },
  },

  {
    files: ["**/*.{ts,vue}"],
    rules: {
      // TypeScript 已经在编译期检查了未定义标识符与类型，
      // ESLint 再跑一遍 no-undef 只会对浏览器全局产生误报
      "no-undef": "off",
      "no-restricted-imports": ["error", { paths: RESTRICTED_LIBRARIES }],
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/consistent-type-imports": [
        "error",
        { prefer: "type-imports", fixStyle: "separate-type-imports" },
      ],
    },
  },

  /*
   * IPC 边界：除 `src/api/` 外，禁止直接 import / 调用 `@tauri-apps/api/core`。
   *
   * `src/api/` 是**唯一允许接触 bindings 的地方**（见《IPC 契约与类型生成》）。
   * 业务代码绕过这一层，错误处理、日志、类型收窄就全部失控。
   *
   * 注意选择器：`import { invoke } from "@tauri-apps/api/core"` 之后调用的是
   * **裸 `invoke(...)`**，没有 `core.` 前缀——早先写成
   * `CallExpression[callee.object.name='core']` 的规则从未命中过。
   */
  {
    files: ["src/**/*.{ts,vue}"],
    ignores: ["src/api/**"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          paths: [
            ...RESTRICTED_LIBRARIES,
            {
              name: "@tauri-apps/api/core",
              message: "不要直接 import @tauri-apps/api/core，请在 src/api/ 中封装后使用",
            },
          ],
        },
      ],
      "no-restricted-syntax": [
        "error",
        {
          selector: "CallExpression[callee.name='invoke']",
          message: "不要直接调用 invoke()，请在 src/api/ 中封装后使用（见《IPC 契约与类型生成》）",
        },
      ],
    },
  },

  {
    // api 层放行「直接调 invoke」，但图标/UI 库的限制依然生效
    files: ["src/api/**"],
    rules: {
      "no-restricted-syntax": "off",
    },
  },

  {
    files: ["scripts/**/*.mjs", "*.config.{js,ts}"],
    rules: {
      "@typescript-eslint/no-require-imports": "off",
    },
  },

  prettierConfig,
);
