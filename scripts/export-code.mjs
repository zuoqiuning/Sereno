#!/usr/bin/env node
/**
 * 把项目里**自己写的代码**导出成一份 Markdown 文档。
 *
 * 同时会检查 `PCL参考代码` 目录，如果存在，则将其 C# 代码单独导出为
 * `PCL参考代码导出.md`，不与主项目代码混淆。
 *
 * ## 用法
 *
 * 双击 `scripts/导出代码.bat`，或命令行：
 *
 * ```powershell
 * node scripts/export-code.mjs
 * ```
 */

import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { consola } from "consola";

const log = consola.withTag("export");

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** 主项目输出名 */
const MAIN_OUTPUT_NAME = "Sereno项目代码导出.md";
/** PCL 参考代码输出名 */
const PCL_OUTPUT_NAME = "PCL参考代码导出.md";

/** PCL 参考代码的文件夹名 */
const PCL_DIR_NAME = "PCL参考代码";

// ==========================================
// 规则定义
// ==========================================

/** 主项目要跳过的目录 */
const MAIN_SKIP_DIRS = new Set([
  "node_modules",
  "target",
  "dist",
  "gen",
  "icons",
  ".git",
  ".workbuddy",
  PCL_DIR_NAME, // 主项目导出时跳过 PCL
]);

/** PCL 参考代码要跳过的目录（排除 C# 编译产物） */
const PCL_SKIP_DIRS = new Set([
  "node_modules",
  "bin",
  "obj",
  ".vs",
  ".git",
  ".workbuddy",
]);

const MAIN_SKIP_FILES = new Set([
  "Cargo.lock",
  "pnpm-lock.yaml",
  ".gitkeep",
  "pnpm-workspace.yaml.orig",
]);

/** 主项目支持的扩展名 */
const MAIN_CODE_EXTENSIONS = new Set([
  ".rs", ".ts", ".tsx", ".vue", ".js", ".mjs", ".cjs",
  ".css", ".scss", ".sass", ".less",
  ".html", ".json", ".jsonc", ".toml", ".yaml", ".yml", ".xml",
  ".bat", ".cmd", ".ps1", ".sh", ".py",
]);

/** PCL 支持的扩展名（补充 C# 相关） */
const PCL_CODE_EXTENSIONS = new Set([
  ".cs", ".xaml", ".csproj", ".sln", ".resx", ".config",
  ".json", ".xml", ".txt", ".md", ".ini", ".yml", ".yaml",
]);

/** 无扩展名文件 */
const CODE_FILENAMES = new Set([
  ".gitignore",
  ".gitattributes",
  ".npmrc",
  ".nvmrc",
  ".env.example",
]);

/** 主项目语言映射 */
const MAIN_LANGUAGE = {
  ".rs": "rust", ".ts": "typescript", ".tsx": "tsx", ".vue": "vue",
  ".js": "javascript", ".mjs": "javascript", ".cjs": "javascript",
  ".css": "css", ".scss": "scss", ".html": "html",
  ".json": "json", ".jsonc": "jsonc", ".toml": "toml", ".yaml": "yaml", ".yml": "yaml",
  ".xml": "xml", ".bat": "batch", ".cmd": "batch", ".ps1": "powershell", ".sh": "bash", ".py": "python",
};

/** PCL 语言映射 */
const PCL_LANGUAGE = {
  ".cs": "csharp", ".xaml": "xml", ".csproj": "xml", ".sln": "text",
  ".resx": "xml", ".config": "xml", ".json": "json", ".xml": "xml",
  ".md": "markdown", ".yml": "yaml", ".yaml": "yaml", ".ini": "ini",
};

// ==========================================
// 工具函数
// ==========================================

/** 判断主项目文件 */
function isMainCodeFile(name) {
  if (MAIN_SKIP_FILES.has(name)) return false;
  if (CODE_FILENAMES.has(name)) return true;
  return MAIN_CODE_EXTENSIONS.has(extname(name).toLowerCase());
}

/** 判断 PCL 文件 */
function isPclCodeFile(name) {
  if (CODE_FILENAMES.has(name)) return true;
  return PCL_CODE_EXTENSIONS.has(extname(name).toLowerCase());
}

/** 计算围栏长度 */
function fenceFor(content) {
  let longest = 0;
  for (const match of content.matchAll(/`+/g)) {
    longest = Math.max(longest, match[0].length);
  }
  return "`".repeat(Math.max(3, longest + 1));
}

/** 相对路径 */
function relPath(absolute, base) {
  return relative(base, absolute).split("\\").join("/");
}

/** 时间戳 */
function timestamp() {
  const now = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return (
    `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())} ` +
    `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`
  );
}

/**
 * 通用遍历收集函数。
 */
function collect(dir, skipDirs, isCodeFileFn, baseDir, groups = []) {
  if (!existsSync(dir)) return groups;
  
  const entries = readdirSync(dir, { withFileTypes: true });
  const subDirs = [];
  const files = [];

  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name, "zh-CN"))) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!skipDirs.has(entry.name)) subDirs.push(full);
      continue;
    }
    if (entry.isFile() && isCodeFileFn(entry.name)) {
      files.push(full);
    }
  }

  groups.push({ dir, files, baseDir });
  for (const sub of subDirs) {
    collect(sub, skipDirs, isCodeFileFn, baseDir, groups);
  }
  return groups;
}

/**
 * 将收集到的 groups 渲染为 Markdown 字符串并写入文件。
 */
function buildMarkdown(outputPath, title, groups, languageMap, baseDir) {
  const sections = [];
  let totalFiles = 0;
  let totalLines = 0;

  for (const group of groups) {
    const entries = [];
    for (const file of group.files) {
      const raw = readFileSync(file, "utf8");
      const content = raw.replace(/\r\n/g, "\n");
      const lineCount = content.length === 0 ? 0 : content.split("\n").length;
      totalFiles += 1;
      totalLines += lineCount;
      entries.push({
        path: relPath(file, baseDir),
        content,
        lineCount,
        lang: languageMap[extname(file).toLowerCase()] ?? "",
      });
    }
    if (entries.length > 0) {
      sections.push({ dir: relPath(group.dir, baseDir) || ".", entries });
    }
  }

  if (totalFiles === 0) {
    log.warn(`[${title}] 没有找到任何可导出的代码文件。`);
    return;
  }

  const out = [];
  out.push(`# ${title}`);
  out.push("");
  out.push("> 本文件由 `scripts/export-code.mjs` 自动生成，**请勿手工编辑**。");
  out.push("> 每次运行会覆盖同名文件。");
  out.push("");
  out.push(`| 项 | 值 |`);
  out.push(`| :--- | :--- |`);
  out.push(`| 生成时间 | ${timestamp()} |`);
  out.push(`| 导出文件数 | ${totalFiles} |`);
  out.push(`| 导出总行数 | ${totalLines} |`);
  out.push("");
  out.push("---");
  out.push("");
  out.push("## 文件清单（完整路径）");
  out.push("");

  let index = 0;
  for (const section of sections) {
    const dirTitle = section.dir === "." ? "（根目录）" : section.dir;
    out.push(`### 📁 \`${dirTitle}\``);
    out.push("");
    out.push("| # | 路径 | 行数 |");
    out.push("| ---: | :--- | ---: |");
    for (const entry of section.entries) {
      index += 1;
      out.push(`| ${index} | \`${entry.path}\` | ${entry.lineCount} |`);
    }
    out.push("");
  }

  out.push("---");
  out.push("");
  out.push("## 代码正文");
  out.push("");

  for (const section of sections) {
    const dirTitle = section.dir === "." ? "（根目录）" : section.dir;
    out.push(`### 📁 \`${dirTitle}\``);
    out.push("");
    for (const entry of section.entries) {
      out.push(`#### \`${entry.path}\``);
      out.push("");
      const fence = fenceFor(entry.content);
      out.push(`${fence}${entry.lang}`);
      out.push(entry.content.replace(/\n+$/, ""));
      out.push(fence);
      out.push("");
    }
  }

  writeFileSync(outputPath, `${out.join("\n")}\n`, "utf8");
  const size = statSync(outputPath).size;

  log.success(`已生成 ${outputPath}`);
  log.info(`  ├─ 导出 ${totalFiles} 个文件 / ${totalLines} 行 / ${(size / 1024).toFixed(1)} KB`);
  log.info(`  └─ 路径：${outputPath}`);
}

// ==========================================
// 主流程
// ==========================================

function main() {
  log.start("导出代码中...");

  // 1. 导出主项目代码
  const mainGroups = collect(ROOT, MAIN_SKIP_DIRS, isMainCodeFile, ROOT);
  buildMarkdown(join(ROOT, MAIN_OUTPUT_NAME), "Sereno 项目代码导出", mainGroups, MAIN_LANGUAGE, ROOT);

  // 2. 尝试导出 PCL 参考代码
  const pclDir = join(ROOT, PCL_DIR_NAME);
  if (existsSync(pclDir)) {
    log.info(`检测到 PCL 参考代码目录，准备单独导出...`);
    const pclGroups = collect(pclDir, PCL_SKIP_DIRS, isPclCodeFile, pclDir);
    buildMarkdown(join(ROOT, PCL_OUTPUT_NAME), "PCL 参考代码导出", pclGroups, PCL_LANGUAGE, pclDir);
  } else {
    log.info(`未找到 \`${PCL_DIR_NAME}\` 目录，跳过单独导出。`);
  }
}

main();