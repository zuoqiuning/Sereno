/**
 * 把 Tailwind 的任意值写法换成规范写法。
 *
 * ## 为什么需要
 *
 * Tailwind v4 的 lint 会对这些写法提 `suggestCanonicalClasses` 警告：
 *
 * - `w-[168px]` → `w-42`（spacing scale 是 4px 一档，168 / 4 = 42）
 * - `z-[60]` → `z-60`
 * - `bg-ink/[0.03]` → `bg-ink/3`（v4 支持直接写百分比）
 * - `break-words` → `wrap-break-word`（v4 改了断词类的名字）
 *
 * 规范写法的好处不只是消警告：它们走 spacing scale 而不是任意值，
 * 主题的 `--spacing` 一改就跟着变。
 *
 * ## 哪些不转
 *
 * - 除不尽 4 的（`w-[150px]` → 37.5，不是整数档）
 * - 不是 px 单位的（`text-[13px]` 的字号有自己的 scale，另算）
 * - 负值与小数（`w-[1.5px]` 这类没有等价写法）
 */
import fs from "fs";
import path from "path";

const root = process.argv[2] ?? "apps/desktop/src";

/** spacing scale 的基准单位（px）。Tailwind v4 默认 0.25rem = 4px。 */
const SPACING = 4;

/** 走 spacing scale 的工具前缀。 */
const SPACING_PREFIXES = [
  "w", "min-w", "max-w",
  "h", "min-h", "max-h",
  "p", "px", "py", "pt", "pr", "pb", "pl",
  "m", "mx", "my", "mt", "mr", "mb", "ml",
  "gap", "gap-x", "gap-y",
  "top", "right", "bottom", "left",
  "inset", "inset-x", "inset-y",
  "z",
  "size",
  "basis",
  "scroll-m", "scroll-mx", "scroll-my",
  "scroll-p", "scroll-px", "scroll-py",
  "translate-x", "translate-y",
];

/** 收集所有 .vue 文件。 */
function collect(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...collect(full));
    else if (entry.name.endsWith(".vue")) out.push(full);
  }
  return out;
}

let changedFiles = 0;
let changedClasses = 0;

for (const file of collect(root)) {
  const original = fs.readFileSync(file, "utf8");
  let text = original;

  // 1. `w-[168px]` → `w-42`
  text = text.replace(
    new RegExp(
      `\\b(${SPACING_PREFIXES.join("|")})-\\[(\\d+)px\\]`,
      "g",
    ),
    (match, prefix, px) => {
      const steps = Number(px) / SPACING;
      if (!Number.isInteger(steps) || steps === 0) return match;
      changedClasses += 1;
      return `${prefix}-${steps}`;
    },
  );

  // 2. `bg-ink/[0.03]` → `bg-ink/3`
  //    只处理 0–1 之间、乘 100 后是整数的小数。
  text = text.replace(
    /\b((?:bg|text|border|ring|shadow|decoration|outline|fill|stroke|from|via|to|accent|caret|divide)-[a-z-]+(?:-\d+)?)\/\[(0?\.\d+)\]/g,
    (match, prefix, alpha) => {
      const percent = Math.round(Number(alpha) * 100);
      if (!Number.isInteger(percent) || percent === 0) return match;
      changedClasses += 1;
      return `${prefix}/${percent}`;
    },
  );

  // 3. `break-words` → `wrap-break-word`（v4 的规范名）
  text = text.replace(/\bbreak-words\b/g, () => {
    changedClasses += 1;
    return "wrap-break-word";
  });

  if (text !== original) {
    fs.writeFileSync(file, text, "utf8");
    changedFiles += 1;
    console.log(`  ${file}`);
  }
}

console.log(
  `\n转换完成：${changedClasses} 处类名，${changedFiles} 个文件`,
);
