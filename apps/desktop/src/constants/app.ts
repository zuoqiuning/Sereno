/** 应用信息唯一来源：版本号读根目录 version.json（安装包与 Rust 端同样读它）。 */
import version from "../../../../version.json";

export const APP_NAME = "Sereno";
export const APP_VERSION = version.version;
export const APP_AUTHOR = "丘屿知宁";
export const APP_DESCRIPTION = "轻量、干净的 Minecraft 启动器";
export const APP_TECH = "Tauri 2 · Vue 3 · Rust";
