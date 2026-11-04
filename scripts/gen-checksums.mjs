#!/usr/bin/env node
/**
 * 发布产物的校验和清单。
 *
 * ## 为什么需要
 *
 * 协议只能约束「愿意守规矩的人」，混淆只能抬高二次分发的成本——
 * 真正能让用户分辨「官方版」与「倒卖版」的，是**分发链**：
 * 官方渠道提供固定的 SHA-256，用户自己核对一次就知道手上这份是不是原版。
 *
 * 这个脚本做两件事：
 *
 * - `node scripts/gen-checksums.mjs`  —— 对打包产物生成 `SHA256SUMS.txt`，
 *   连同 git 提交号一起写进清单，发布时随包附上；
 * - `node scripts/gen-checksums.mjs --verify` —— 校验清单里的每个文件，
 *   任何一个对不上就以非零退出码失败。
 *
 * 配合 Authenticode 代码签名（流程见仓库根目录的 SECURITY.md），
 * 就能覆盖「用户被塞了广告版」这个最常见的场景：
 * 签名会因二进制被改而失效，校验和会与清单不符——两个信号指向同一个结论。
 */
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const BUNDLE_DIR = join(ROOT, "apps/desktop/src-tauri/target/release/bundle");
const MANIFEST = join(BUNDLE_DIR, "SHA256SUMS.txt");

/** 需要纳入清单的发布物类型。 */
const ARTIFACT_EXT = new Set([".exe", ".msi", ".zip", ".msix", ".appx"]);

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

function gitCommit() {
  try {
    return execFileSync("git", ["rev-parse", "--short", "HEAD"], {
      cwd: ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return "unknown";
  }
}

function gitDescribe() {
  try {
    return execFileSync("git", ["describe", "--tags", "--always", "--dirty"], {
      cwd: ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return "unknown";
  }
}

const artifacts = walk(BUNDLE_DIR).filter(
  (f) => ARTIFACT_EXT.has(f.slice(f.lastIndexOf("."))) && !f.endsWith("SHA256SUMS.txt"),
);

if (artifacts.length === 0) {
  console.error(
    `没有找到发布产物。\n` +
      `请先执行：pnpm build  （产物目录：${relative(ROOT, BUNDLE_DIR)}）`,
  );
  process.exit(2);
}

if (process.argv.includes("--verify")) {
  if (!existsSync(MANIFEST)) {
    console.error(`找不到清单：${relative(ROOT, MANIFEST)}`);
    process.exit(2);
  }
  const lines = readFileSync(MANIFEST, "utf8")
    .split("\n")
    .filter((l) => l.trim() && !l.startsWith("#"));

  let bad = 0;
  for (const line of lines) {
    const [hash, name] = line.trim().split(/\s+/, 2);
    const file = join(BUNDLE_DIR, name);
    if (!existsSync(file)) {
      console.error(`✗ ${name}：文件缺失`);
      bad += 1;
      continue;
    }
    const actual = sha256(file);
    if (actual !== hash) {
      console.error(`✗ ${name}：校验和不匹配\n    期望 ${hash}\n    实际 ${actual}`);
      bad += 1;
    } else {
      console.log(`✓ ${name}`);
    }
  }

  if (bad > 0) {
    console.error(`\n${bad} 个文件未通过校验——产物已被修改，不要分发它。`);
    process.exit(1);
  }
  console.log(`\n全部 ${lines.length} 个文件校验通过。`);
  process.exit(0);
}

// 生成清单
const header = [
  "# Sereno 发布产物校验清单",
  "#",
  `# 版本: ${gitDescribe()}`,
  `# 提交: ${gitCommit()}`,
  "#",
  "# 用户校验方法（Windows）：",
  "#   certutil -hashfile <下载的文件> SHA256",
  "# 把输出的哈希与下面同名的这一行比对，完全一致才是官方原件。",
  "#",
  "# 发布者请勿手工编辑本文件；由 node scripts/gen-checksums.mjs 生成。",
  "",
].join("\n");

const body = artifacts
  .map((file) => `${sha256(file)}  ${relative(BUNDLE_DIR, file).split("\\").join("/")}`)
  .join("\n");

writeFileSync(MANIFEST, `${header}${body}\n`, "utf8");

console.log(`已生成 ${relative(ROOT, MANIFEST)}：`);
for (const file of artifacts) {
  console.log(`  ${sha256(file)}  ${relative(BUNDLE_DIR, file)}`);
}
console.log(`\n提交: ${gitCommit()}`);
console.log("请把 SHA256SUMS.txt 与安装包一起发布，并在 Release 页贴出同样内容。");
