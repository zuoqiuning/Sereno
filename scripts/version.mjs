#!/usr/bin/env node
/**
 * 版本号同步：**唯一事实源是仓库根的 `version.json`**。
 *
 * | 消费方       | 如何拿到版本                                        |
 * | :----------- | :-------------------------------------------------- |
 * | UI           | `crates/sereno-core/build.rs` 注入 `SERENO_APP_VERSION`  |
 * | 安装包/卸载项 | `tauri.conf.json` 的 `version` 指向 `version.json`    |
 * | Cargo        | `scripts/version.mjs` 同步 `[workspace.package]`     |
 * | package.json | `scripts/version.mjs` 同步（根 + app）                |
 *
 * ```powershell
 * pnpm version:check     # 五项校验
 * pnpm version:set 0.2.0 # 写入新版本并同步全部消费方
 * ```
 *
 * **不要手改 `package.json` / `Cargo.toml` 的版本字段**：
 * 它们是生成物，手改会被 `version:check` 判为不一致。
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { consola } from "consola";

const log = consola.withTag("version");

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const PATHS = {
  version: join(ROOT, "version.json"),
  rootPackage: join(ROOT, "package.json"),
  appPackage: join(ROOT, "apps/desktop/package.json"),
  cargo: join(ROOT, "Cargo.toml"),
  tauriConf: join(ROOT, "apps/desktop/src-tauri/tauri.conf.json"),
};

/** `tauri.conf.json` 中 version 字段应当指向的相对路径。 */
const TAURI_VERSION_PATH = "../../../version.json";

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, data) {
  writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`, "utf8");
}

/** 读取当前版本号（唯一事实源）。 */
function readSourceVersion() {
  const data = readJson(PATHS.version);
  if (typeof data.version !== "string" || data.version.length === 0) {
    throw new Error("version.json 缺少合法的 version 字段");
  }
  return data.version;
}

/** 从 Cargo.toml 里抠出 `[workspace.package]` 的 version。 */
function readCargoVersion() {
  const raw = readFileSync(PATHS.cargo, "utf8");
  const inWorkspacePackage = raw.split("[workspace.package]")[1] ?? "";
  const match = inWorkspacePackage.match(/^\s*version\s*=\s*"([^"]+)"/m);
  return match ? match[1] : null;
}

/**
 * 校验五项一致性。
 *
 * @returns {string[]} 不一致项的描述列表，空数组表示全部一致
 */
export function checkVersions() {
  const problems = [];
  const source = readSourceVersion();

  // 1. 源
  if (!source) {
    problems.push("version.json: 缺少 version");
  }

  // 2. 根 package.json
  const rootPkg = readJson(PATHS.rootPackage);
  if (rootPkg.version !== source) {
    problems.push(`package.json: ${rootPkg.version ?? "(无)"} ≠ ${source}`);
  }

  // 3. app package.json
  const appPkg = readJson(PATHS.appPackage);
  if (appPkg.version !== source) {
    problems.push(`apps/desktop/package.json: ${appPkg.version ?? "(无)"} ≠ ${source}`);
  }

  // 4. Cargo.toml
  const cargoVersion = readCargoVersion();
  if (cargoVersion !== source) {
    problems.push(`Cargo.toml: ${cargoVersion ?? "(无)"} ≠ ${source}`);
  }

  // 5. tauri.conf.json 的 version 必须是**指向 version.json 的路径**，不能是字面量
  const tauriConf = readJson(PATHS.tauriConf);
  if (tauriConf.version !== TAURI_VERSION_PATH) {
    problems.push(
      `tauri.conf.json: version 应为 "${TAURI_VERSION_PATH}"（指向 version.json 的路径），实际为 "${tauriConf.version}"`,
    );
  }

  return problems;
}

/** 把版本号同步到全部消费方。 */
export function setVersion(next) {
  if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(next)) {
    throw new Error(`版本号格式非法: ${next}（应为 x.y.z）`);
  }

  writeJson(PATHS.version, { version: next });

  const rootPkg = readJson(PATHS.rootPackage);
  rootPkg.version = next;
  writeJson(PATHS.rootPackage, rootPkg);

  const appPkg = readJson(PATHS.appPackage);
  appPkg.version = next;
  writeJson(PATHS.appPackage, appPkg);

  const cargoRaw = readFileSync(PATHS.cargo, "utf8");
  const parts = cargoRaw.split("[workspace.package]");
  if (parts.length !== 2) {
    throw new Error("Cargo.toml 中找不到 [workspace.package] 段");
  }
  const [, rest] = parts;
  // 注意：不能用 `\s*`——它会连上一行行尾的 CRLF 一起吞掉，
  // 把 rust-version 和 version 拼成一行，直接写坏 Cargo.toml。
  const replaced = rest.replace(
    /^[ \t]*version[ \t]*=[ \t]*"[^"]*"/m,
    `version = "${next}"`,
  );
  writeFileSync(PATHS.cargo, parts[0] + "[workspace.package]" + replaced, "utf8");

  const tauriConf = readJson(PATHS.tauriConf);
  tauriConf.version = TAURI_VERSION_PATH;
  writeJson(PATHS.tauriConf, tauriConf);

  return next;
}

function main() {
  const args = process.argv.slice(2);
  const isCheck = args.includes("--check");
  const target = args.find((arg) => !arg.startsWith("--"));

  if (isCheck || !target) {
    const problems = checkVersions();
    if (problems.length === 0) {
      log.success(`版本一致：${readSourceVersion()}`);
      return;
    }
    log.error("版本不一致：");
    for (const problem of problems) {
      log.error(`  · ${problem}`);
    }
    process.exitCode = 1;
    return;
  }

  const applied = setVersion(target);
  log.success(`版本已同步到 ${applied}`);
}

// 仅在「作为脚本直接执行」时运行 main()。
// 用 pathToFileURL 比较，Windows 上 import.meta.url 是 file:///D:/...（三斜杠），
// 手拼字符串会漏掉一个斜杠而永远不匹配。
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
