#!/usr/bin/env node
/**
 * 构建期获取 EasyTier 内核，使其随安装包分发（用户首次联机零下载）。
 *
 * 流程：下载官方 release zip → SHA-256 校验（必须匹配下方硬编码摘要）→
 * 解压后把运行时必需文件（kernel.rs::BUNDLED_FILES 同一份清单）写入
 * apps/desktop/src-tauri/resources/easytier/。
 * Tauri 打包时把它们映射进安装包资源；运行时 kernel.rs 会**逐文件再次**
 * 校验哈希，双保险防供应链篡改。
 *
 * 用法：node scripts/fetch-easytier.mjs
 * 产物不入库（.gitignore 已排除）——40MB 二进制不进 git。
 *
 * ⚠️ 升级内核版本时：同步修改 crates/sereno-core/src/multiplayer/kernel.rs
 *    的 KERNEL_VERSION / KERNEL_SHA256 / BUNDLED_FILES，以及本文件的
 *    KERNEL_VERSION / KERNEL_SHA256 / KEEP_FILES（两处硬编码，注释互相指认）。
 */

import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readdirSync, rmSync, writeFile } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

// ---- 与 crates/sereno-core/src/multiplayer/kernel.rs 保持一致 ----
const KERNEL_VERSION = "2.7.0";
const KERNEL_SHA256 = {
  "easytier-windows-x86_64":
    "790fd9be347b00753570ac2b65f874a591ede9427603fe34e74a86b2b084847f",
  "easytier-linux-x86_64":
    "442238912065537c280869089217303e3d2813e9c5d2a6b14deaf25667065aa2",
  "easytier-macos-x86_64":
    "4a52b977eba3c29fdf392748318992be766dbab1d144d7f5cb69bfea572e237b",
  "easytier-macos-aarch64":
    "ff0ad04fd6fcc462021a3b3161ed82270dd9e7927221ecb22009f0df697bdf52",
};
// 运行时必需文件（web 面板不打包，见 kernel.rs::BUNDLED_FILES）
const KEEP_FILES = [
  "easytier-core.exe",
  "wintun.dll",
  "Packet.dll",
  "WinDivert64.sys",
  "easytier-cli.exe",
];
// ------------------------------------------------------------------

const PLATFORM = {
  "win32-x64": "easytier-windows-x86_64",
  "linux-x64": "easytier-linux-x86_64",
  "darwin-x64": "easytier-macos-x86_64",
  "darwin-arm64": "easytier-macos-aarch64",
};

const assetKey = PLATFORM[`${process.platform}-${process.arch}`];
if (!assetKey) {
  console.error(`[fetch-easytier] 不支持的平台: ${process.platform}-${process.arch}`);
  process.exit(1);
}

const expected = KERNEL_SHA256[assetKey];
const url = `https://github.com/EasyTier/EasyTier/releases/download/v${KERNEL_VERSION}/${assetKey}-v${KERNEL_VERSION}.zip`;
const mirrors = [`https://ghfast.top/${url}`, `https://gh-proxy.com/${url}`, url];

const outDir = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "apps/desktop/src-tauri/resources/easytier",
);

async function download(url) {
  const resp = await fetch(url, { redirect: "follow" });
  if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
  const buf = Buffer.from(await resp.arrayBuffer());
  if (buf.length < 1024 * 1024) throw new Error(`体积异常: ${buf.length}B`);
  return buf;
}

let bytes = null;
for (const candidate of mirrors) {
  try {
    console.log(`[fetch-easytier] 下载 ${candidate}`);
    bytes = await download(candidate);
    break;
  } catch (error) {
    console.warn(`[fetch-easytier] 失败: ${error.message}，尝试下一源`);
  }
}
if (!bytes) {
  console.error("[fetch-easytier] 所有下载源均失败");
  process.exit(1);
}

const actual = createHash("sha256").update(bytes).digest("hex");
if (actual !== expected) {
  console.error(`[fetch-easytier] SHA-256 不匹配！拒绝使用。\n  期望: ${expected}\n  实际: ${actual}`);
  process.exit(1);
}

// 校验通过 → 解压 → 只保留运行时必需文件
mkdirSync(outDir, { recursive: true });
const tmpZip = path.join(outDir, "kernel.zip");
writeFile(tmpZip, bytes, () => undefined);

const tmpExtract = path.join(outDir, "_extract");
rmSync(tmpExtract, { recursive: true, force: true });
mkdirSync(tmpExtract, { recursive: true });

// Windows/macOS/Linux 通用解压：优先 tar（zip 支持随平台而异），
// Windows 上用官方 tar（libarchive 后端，支持 zip）。
execFileSync("tar", ["-xf", tmpZip, "-C", tmpExtract]);

function findFile(dir, name, depth = 0) {
  if (depth > 4) return null;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const hit = findFile(full, name, depth + 1);
      if (hit) return hit;
    } else if (entry.name === name) {
      return full;
    }
  }
  return null;
}

for (const name of KEEP_FILES) {
  const found = findFile(tmpExtract, name);
  if (!found) {
    console.error(`[fetch-easytier] 内核包中缺少 ${name}`);
    process.exit(1);
  }
  copyFileSync(found, path.join(outDir, name));
}
rmSync(tmpExtract, { recursive: true, force: true });
rmSync(tmpZip, { force: true });
console.log(
  `[fetch-easytier] 完成：${KEEP_FILES.length} 个文件已就位（${(bytes.length / 1048576).toFixed(1)} MB，校验通过）`,
);
