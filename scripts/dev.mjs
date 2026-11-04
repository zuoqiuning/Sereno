#!/usr/bin/env node
/**
 * 一键启动：端口治理 → 前端服务器 → 桌面窗口。
 *
 * ## 为什么需要这个脚本
 *
 * 裸 `tauri dev` 有两个痛点：
 *
 * 1. **端口被占用时白屏**。`vite.config.ts` 设了 `strictPort: true`，端口被占会
 *    直接报错退出，而不是偷偷顺延到 5174——一旦顺延，`tauri.conf.json` 的
 *    `devUrl` 仍然指向 5173，Tauri 就会一直停在「等待前端服务器」。
 *    本脚本**固定端口 5173**：发现占用就杀掉占用进程，**绝不后推端口**。
 *
 * 2. **看不清发生了什么**。脚本用 consola 输出带颜色、带图标的时间线，
 *    端口清理、进程退出、前端就绪、窗口启动都有明确节点。
 *
 * ## 用法
 *
 * ```powershell
 * pnpm dev              # 一键启动（含端口治理）
 * pnpm dev:raw          # 跳过端口清理，直接 tauri dev
 * node scripts/dev.mjs --skip-port-cleanup
 * node scripts/dev.mjs --port 5173
 * node scripts/dev.mjs --force   # 即使是本项目的另一个实例，也强制清理
 * ```
 *
 * ## 端口治理的两条规则
 *
 * 1. **外人占用 → 杀掉**。端口被别的进程占着（别的开发服务器、遗留的
 *    vite……）就结束它，**绝不后推端口**——后推会让 Tauri 一直停在
 *    「等待前端服务器」。
 * 2. **自己人占用 → 不杀**。同一台机器上并行跑两个 `pnpm dev` 时，
 *    后一个会杀掉前一个的 vite，前一个的 tauri 随即报
 *    `beforeDevCommand terminated with a non-zero status code` 而退出——
 *    两败俱伤，还留下一堆看不懂的报错。因此启动时写一份
 *    **同源实例标记**：标记还在且进程活着，就说明是本项目的另一个实例，
 *    此时**拒绝启动并给出明确提示**，而不是把它杀掉。
 *    确实要强制接管时用 `--force`。
 *
 * 端口在四处出现，必须同时改：
 *   1. `apps/desktop/vite.config.ts` 的 `server.port`
 *   2. `apps/desktop/src-tauri/tauri.conf.json` 的 `build.devUrl`
 *   3. 本文件的 `DEFAULT_PORT`
 *   4. `apps/desktop/vite.config.ts` 的 `server.host`（必须写 127.0.0.1）
 */

import { spawn, execFile } from "node:child_process";
import { readFileSync, unlinkSync, writeFileSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { consola } from "consola";

/** 固定端口。**不因为被占用就后推**——后推会导致 Tauri 白屏。 */
const DEFAULT_PORT = 5173;

/** 端口释放等待上限（毫秒）。 */
const PORT_RELEASE_TIMEOUT = 5000;

/** 轮询间隔（毫秒）。 */
const POLL_INTERVAL = 200;

/** 等待清理后仍被占用时的最终确认次数。 */
const KILL_RETRY = 3;

const isWindows = process.platform === "win32";

/** 仓库根目录（本文件在 `scripts/` 下，根目录是它的上一级）。 */
const PROJECT_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** 同源实例标记的有效期（毫秒）。超过这么久没清理过的标记一律视为失效。 */
const MARKER_TTL = 24 * 60 * 60 * 1000;

const log = consola.withTag("sereno");

/** 解析命令行参数。 */
function parseArgs(argv) {
  const options = {
    skipPortCleanup: false,
    port: DEFAULT_PORT,
    raw: false,
    force: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--skip-port-cleanup" || arg === "--no-port-cleanup") {
      options.skipPortCleanup = true;
    } else if (arg === "--raw") {
      options.raw = true;
    } else if (arg === "--force") {
      options.force = true;
    } else if (arg === "--port" && argv[i + 1]) {
      options.port = Number.parseInt(argv[i + 1], 10);
      i += 1;
    } else if (arg.startsWith("--port=")) {
      options.port = Number.parseInt(arg.slice("--port=".length), 10);
    }
  }

  return options;
}

/* =========================================================
   同源实例标记
   ========================================================= */

/** 标记文件路径（按端口区分，不同端口互不干扰）。 */
function markerPath(port) {
  return join(tmpdir(), `sereno-dev-${port}.json`);
}

/** 读取同源实例标记；标记不存在或已失效则返回 null。 */
function readMarker(port) {
  try {
    const data = JSON.parse(readFileSync(markerPath(port), "utf8"));
    if (typeof data.pid !== "number" || data.pid <= 0) {
      return null;
    }
    if (typeof data.startedAt !== "number" || Date.now() - data.startedAt > MARKER_TTL) {
      return null;
    }
    return data;
  } catch {
    return null;
  }
}

/** 写入同源实例标记。 */
function writeMarker(port) {
  try {
    writeFileSync(
      markerPath(port),
      JSON.stringify({ pid: process.pid, projectRoot: PROJECT_ROOT, startedAt: Date.now() }),
      "utf8",
    );
  } catch {
    /* 标记写不了不影响启动，只是失去「自己人保护」 */
  }
}

/** 清除同源实例标记。 */
function clearMarker(port) {
  try {
    unlinkSync(markerPath(port));
  } catch {
    /* 已经没了 */
  }
}

/** 进程是否还活着（信号 0 只做存在性检查）。 */
function isProcessAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    // EPERM = 进程存在但当前用户无权发信号
    return error && error.code === "EPERM";
  }
}

/**
 * 端口是否被「本项目的另一个开发实例」占着。
 *
 * 命中条件：标记文件里的进程还活着，且标记指向同一个仓库根。
 */
function findSiblingInstance(port) {
  const marker = readMarker(port);
  if (!marker) {
    return null;
  }
  if (marker.projectRoot !== PROJECT_ROOT) {
    return null;
  }
  if (!isProcessAlive(marker.pid)) {
    return null;
  }
  return marker;
}

/**
 * 查找监听指定端口的进程 PID。
 *
 * Windows 用 `netstat -ano -p tcp`；其他平台用 `lsof -nP -iTCP:<port> -sTCP:LISTEN`。
 */
async function findListeningPids(port) {
  if (isWindows) {
    return new Promise((resolve) => {
      execFile(
        "netstat",
        ["-ano", "-p", "tcp"],
        { windowsHide: true, maxBuffer: 8 * 1024 * 1024 },
        (error, stdout) => {
          if (error) {
            resolve([]);
            return;
          }
          const pids = new Set();
          for (const line of stdout.split(/\r?\n/)) {
            // TCP  <local>  <foreign>  LISTENING  <pid>
            const match = line.match(/^\s*TCP\s+(\S+)\s+\S+\s+LISTENING\s+(\d+)\s*$/i);
            if (!match) {
              continue;
            }
            const local = match[1];
            const localPort = Number.parseInt(local.slice(local.lastIndexOf(":") + 1), 10);
            if (localPort === port) {
              pids.add(Number.parseInt(match[2], 10));
            }
          }
          resolve([...pids]);
        },
      );
    });
  }

  return new Promise((resolve) => {
    execFile(
      "lsof",
      ["-nP", `-iTCP:${port}`, "-sTCP:LISTEN", "-t"],
      { maxBuffer: 1024 * 1024 },
      (error, stdout) => {
        if (error) {
          resolve([]);
          return;
        }
        const pids = stdout
          .split(/\r?\n/)
          .map((line) => Number.parseInt(line.trim(), 10))
          .filter((pid) => Number.isInteger(pid) && pid > 0);
        resolve([...new Set(pids)]);
      },
    );
  });
}

/** 判断端口是否可绑定。 */
function isPortFree(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => {
      server.close(() => resolve(true));
    });
    server.listen(port, "127.0.0.1");
  });
}

/** 结束进程树。 */
function killProcessTree(pid) {
  return new Promise((resolve) => {
    if (isWindows) {
      // /T 结束整棵进程树（tauri dev → vite），/F 强制结束
      execFile("taskkill", ["/PID", String(pid), "/T", "/F"], { windowsHide: true }, () => {
        resolve();
      });
      return;
    }
    try {
      process.kill(-pid, "SIGKILL");
    } catch {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
        /* 进程可能已经退出 */
      }
    }
    resolve();
  });
}

/** 等待端口真正释放。 */
async function waitForPortRelease(port, timeout) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await isPortFree(port)) {
      return true;
    }
    await sleep(POLL_INTERVAL);
  }
  return isPortFree(port);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * 结束本项目遗留的应用进程。
 *
 * 只杀掉占用端口的 vite **是不够的**：`cargo` 与 `sereno-desktop.exe`
 * 挂在 `tauri dev` 下面，是 vite 的**旁系**而不是子进程，
 * `taskkill /T` 结束 vite 的进程树时带不走它们。
 * 结果就是「端口清干净了，旧窗口还开着」——两个应用窗口叠在一起。
 *
 * 定位方式：**可执行文件路径**落在本项目 `target/` 下的那个 exe，
 * 因此不会误伤别的项目的同名程序。
 */
async function killLeftoverApps() {
  if (!isWindows) {
    return [];
  }

  // PowerShell 单引号里反斜杠不是转义符，原样传入即可
  const ps =
    `$root = '${PROJECT_ROOT}\\target\\';` +
    `Get-CimInstance Win32_Process -Filter "Name='sereno-desktop.exe'" |` +
    ` Where-Object { $_.ExecutablePath -like "$root*" } |` +
    ` Select-Object -ExpandProperty ProcessId`;

  const pids = await new Promise((resolve) => {
    execFile(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-Command", ps],
      { windowsHide: true, maxBuffer: 1024 * 1024 },
      (error, stdout) => {
        if (error) {
          resolve([]);
          return;
        }
        resolve(
          stdout
            .split(/\r?\n/)
            .map((line) => Number.parseInt(line.trim(), 10))
            .filter((pid) => Number.isInteger(pid) && pid > 0),
        );
      },
    );
  });

  for (const pid of pids) {
    await killProcessTree(pid);
    log.info(`已结束遗留的应用进程 PID ${pid}`);
  }

  return pids;
}

/**
 * 端口治理：发现占用就清掉，**不后推端口**。
 *
 * 但**本项目的另一个开发实例**例外——杀掉它会两败俱伤，
 * 因此此时拒绝启动并给出明确提示（`--force` 可强制接管）。
 */
async function ensurePortAvailable(port, { force = false } = {}) {
  const pids = await findListeningPids(port);

  if (pids.length === 0) {
    log.success(`端口 ${port} 空闲`);
    return true;
  }

  if (!force) {
    const sibling = findSiblingInstance(port);
    if (sibling) {
      const since = new Date(sibling.startedAt).toLocaleTimeString("zh-CN");
      log.error(
        `端口 ${port} 已被本项目的另一个开发实例占用（PID ${sibling.pid}，${since} 启动）。\n` +
          `        两个实例同时跑会互相残杀：后一个会杀掉前一个的前端服务器，\n` +
          `        前一个随即报错退出。请先在那个实例的终端按 Ctrl+C 停掉它。\n` +
          `        确实要强制接管：node scripts/dev.mjs --force`,
      );
      return false;
    }
  }

  log.warn(`端口 ${port} 被占用（PID: ${pids.join(", ")}），正在清理…`);

  for (const pid of pids) {
    await killProcessTree(pid);
    log.info(`已结束占用进程 PID ${pid}`);
  }

  for (let attempt = 1; attempt <= KILL_RETRY; attempt += 1) {
    if (await waitForPortRelease(port, PORT_RELEASE_TIMEOUT)) {
      log.success(`端口 ${port} 已释放`);
      // 端口释放后，把没跟着 vite 一起死的遗留应用进程也收掉，
      // 否则会出现「端口清了但旧窗口还开着」。
      await killLeftoverApps();
      return true;
    }
    log.warn(`端口 ${port} 仍被占用，第 ${attempt} 次重试清理…`);

    const remaining = await findListeningPids(port);
    if (remaining.length === 0) {
      continue;
    }
    for (const pid of remaining) {
      await killProcessTree(pid);
    }
  }

  const stillBusy = await findListeningPids(port);
  if (stillBusy.length > 0) {
    log.error(
      `无法释放端口 ${port}（PID: ${stillBusy.join(", ")}）。` +
        `请手动结束这些进程，或用 --port 指定其他端口（注意 tauri.conf.json 的 devUrl 需同步）。`,
    );
    return false;
  }

  log.success(`端口 ${port} 已释放`);
  return true;
}

/**
 * 启动 `pnpm --filter @sereno/desktop tauri dev`。
 *
 * ## 为什么优先用 `npm_execpath`
 *
 * `pnpm dev` / `npm run dev` 启动时，包管理器会把自己的**入口文件绝对路径**
 * 写进 `npm_execpath`。直接执行它，比在 `cmd.exe` 里找一个 `pnpm.cmd` 可靠得多：
 *
 * - 后者依赖 PATH 里恰好有 pnpm 的全局目录。从别的终端、IDE 任务或
 *   自动化脚本直接调起 `node scripts/dev.mjs` 时，PATH 常常不含它，
 *   于是报「'pnpm' 不是内部或外部命令」；
 * - 前者不受 PATH 影响，也不受 PATHEXT / 代码页影响。
 *
 * 兜底仍然保留 `cmd.exe /c pnpm ...`：直接用 node 跑本脚本时
 * （没有 npm_execpath）只能这么走。
 * **不要用 `shell: true`**——Node 22+ 对它有 DEP0190 弃用警告，
 * 而且参数转义会变得不可控。
 */
function startDevServer(port) {
  const pnpmArgs = ["--filter", "@sereno/desktop", "tauri", "dev"];
  const execPath = process.env.npm_execpath;

  let command;
  let args;

  if (execPath && /\.exe$/i.test(execPath)) {
    // pnpm.exe：直接执行，不需要 cmd 包装
    command = execPath;
    args = pnpmArgs;
  } else if (execPath && /\.(c|m)?js$/i.test(execPath)) {
    // pnpm.cjs / pnpm.js：用当前 node 执行
    command = process.execPath;
    args = [execPath, ...pnpmArgs];
  } else {
    // 兜底：cmd.exe /c pnpm ...
    command = isWindows ? process.env.ComSpec || "cmd.exe" : "pnpm";
    args = isWindows ? ["/d", "/s", "/c", `pnpm ${pnpmArgs.join(" ")}`] : pnpmArgs;
  }

  log.start("启动桌面应用（前端由 tauri 的 beforeDevCommand 拉起）…");
  log.info(`开发服务器: http://127.0.0.1:${port}`);
  log.info(`包管理器入口: ${command}`);

  const child = spawn(command, args, {
    stdio: "inherit",
    windowsHide: false,
    env: {
      ...process.env,
      // 让 Tauri 的 beforeDevCommand 在子 shell 里也能找到正确的可执行文件
      PATH: process.env.PATH || "",
      RUST_LOG: process.env.RUST_LOG ?? "info,sereno_core=debug,sereno_desktop_lib=debug",
      // cargo 网络兜底：crates.io 抽风时 cargo 会一直挂着，既不报错也不动。
      // 实测曾因此卡死 28 分钟。仓库级 .cargo/config.toml 也有同样的设置，
      // 这里再兜一层，保证通过 npm script 启动时同样生效。
      CARGO_HTTP_TIMEOUT: process.env.CARGO_HTTP_TIMEOUT ?? "30",
      CARGO_HTTP_LOW_SPEED_LIMIT: process.env.CARGO_HTTP_LOW_SPEED_LIMIT ?? "1",
      CARGO_NET_RETRY: process.env.CARGO_NET_RETRY ?? "3",
    },
  });

  return child;
}

/** 结束子进程树。 */
async function terminate(child) {
  if (child === null || child.exitCode !== null) {
    return;
  }
  log.start("正在关闭…");
  await killProcessTree(child.pid);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));

  log.info(`Sereno 开发环境启动中（端口 ${options.port}）`);

  if (!options.skipPortCleanup && !options.raw) {
    const ready = await ensurePortAvailable(options.port, { force: options.force });
    if (!ready) {
      process.exitCode = 1;
      return;
    }
  } else {
    log.info("已跳过端口清理");
  }

  // 登记同源实例：并行跑第二个 `pnpm dev` 时靠它识别「自己人」，避免互相残杀
  writeMarker(options.port);
  process.on("exit", () => clearMarker(options.port));

  const child = startDevServer(options.port);

  // Ctrl+C 时优雅结束整棵进程树，避免残留 vite 占用端口
  let shuttingDown = false;
  const onSignal = async () => {
    if (shuttingDown) {
      return;
    }
    shuttingDown = true;
    await terminate(child);
    clearMarker(options.port);
    process.exit(0);
  };

  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);

  child.on("exit", (code, signal) => {
    clearMarker(options.port);
    if (shuttingDown) {
      return;
    }
    if (signal) {
      log.info(`桌面应用已退出（信号 ${signal}）`);
    } else {
      log.info(`桌面应用已退出（代码 ${code ?? 0}）`);
    }
    process.exit(code ?? 0);
  });

  child.on("error", (error) => {
    log.error(`启动失败: ${error.message}`);
    process.exit(1);
  });
}

main().catch((error) => {
  log.error(error instanceof Error ? error.stack : String(error));
  process.exit(1);
});
