/**
 * 统一类型出口。
 *
 * `bindings.ts` 已经包含全部类型，本文件只做 `export * from '../bindings'`，
 * 让业务代码统一从 `@/types` 引用。
 *
 * **任何手写类型都是契约腐化的开始**——缺类型就回去补 Rust 的 `specta::Type`，
 * 不在前端打补丁。
 */
export * from "../bindings";
