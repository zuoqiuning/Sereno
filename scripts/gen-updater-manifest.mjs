#!/usr/bin/env node
/**
 * 生成 Tauri 更新清单 `latest.json`。
 *
 * ## 这个文件是什么
 *
 * `tauri-plugin-updater` 的静态 JSON 服务模式下，客户端会去读清单，
 * 拿到「最新版本号 + 下载地址 + 签名」，然后下载产物、**用内置公钥验签**，
 * 验签不过就拒绝安装。
 *
 * 清单本身**没有签名**（它走 HTTPS），所以：
 *
 * - 我们开启了 `requireSignedVersion`，签名里记录的版本号必须与清单声明的一致，
 *   否则客户端拒绝——这挡住了「伪造高版本号 + 指向旧包」的降级攻击；
 * - 清单里的 `signature` 字段是**内容**（不是路径），客户端会拿它验签产物。
 *
 * ## 为什么 URL 要重写
 *
 * Tauri 生成的清单默认指向构建产物在 GitHub Release 上的地址。
 * 国内访问 GitHub 下载很慢，因此发布到腾讯云 COS 时必须把 URL 换成 COS 域名。
 *
 * 用法：
 *
 * ```bash
 * node scripts/gen-updater-manifest.mjs --base-url https://xxx.cos.ap-beijing.myqcloud.com
 * node scripts/gen-updater-manifest.mjs --base-url https://github.com/o/r/releases/download/v0.2.0
 * ```
 *
 * 不带 `--base-url` 时用 GitHub Release 的标准地址。
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

/**
 * 产物目录的候选位置。
 *
 * Cargo workspace 把编译产物统一放在仓库根的 `target/`，
 * 但 **Tauri CLI 是按 crate 目录定位 bundle 输出位置**的，
 * 所以打包产物落在 `apps/desktop/src-tauri/target/release/bundle`。
 *
 * 两个位置都查一遍，命中哪个用哪个——CI 与本地都可能遇到其中一种。
 */
const BUNDLE_CANDIDATES = [
  join(ROOT, "apps/desktop/src-tauri/target/release/bundle"),
  join(ROOT, "target/release/bundle"),
];

const MANIFEST = join(BUNDLE_CANDIDATES[0], "latest.json");

/** 仓库名与默认的 Release 下载前缀。 */
const REPO = "zuoqiuning/Sereno";

/** 腾讯云 COS 桶（与 tauri.conf.json 的 endpoints 保持一致）。 */
const COS_BASE = "https://sereno-updates-1316381240.cos.ap-beijing.myqcloud.com";

function fail(message) {
  console.error(`[gen-updater-manifest] ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { baseUrl: null, version: null, notes: "", out: null };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--base-url" && argv[i + 1]) {
      args.baseUrl = argv[i + 1];
      i += 1;
    } else if (argv[i] === "--version" && argv[i + 1]) {
      args.version = argv[i + 1];
      i += 1;
    } else if (argv[i] === "--notes" && argv[i + 1]) {
      args.notes = argv[i + 1];
      i += 1;
    } else if (argv[i] === "--out" && argv[i + 1]) {
      args.out = argv[i + 1];
      i += 1;
    }
  }
  return args;
}

/** 读取包版本。 */
function readVersion() {
  const raw = readFileSync(join(ROOT, "version.json"), "utf8");
  return JSON.parse(raw).version;
}

/** 找到真正有产物的 bundle 目录。 */
function findBundleDir() {
  for (const dir of BUNDLE_CANDIDATES) {
    if (!existsSync(dir)) continue;
    // 目录存在但没有安装包，说明是空壳（比如上一次构建失败留下的）
    const hasArtifact = readdirSync(dir).some((entry) =>
      /^(nsis|msi)$/i.test(entry),
    );
    if (hasArtifact) return dir;
  }
  return null;
}

/** 收集 bundle 目录下的安装包与对应签名。 */
function collectArtifacts() {
  const bundleDir = findBundleDir();
  if (!bundleDir) {
    fail(
      "找不到打包产物。请先执行 `pnpm build`。\n" +
        `  已查找：\n${BUNDLE_CANDIDATES.map((d) => `    ${d}`).join("\n")}`,
    );
  }

  const found = [];
  for (const entry of readdirSync(bundleDir)) {
    const dir = join(bundleDir, entry);
    if (!statSync(dir).isDirectory()) continue;

    for (const file of readdirSync(dir)) {
      // 只认安装包本体：.exe / .msi
      if (!/\.(exe|msi)$/i.test(file)) continue;

      const artifact = join(dir, file);
      const signature = `${artifact}.sig`;
      if (!existsSync(signature)) {
        fail(
          `安装包缺少签名文件：${file}\n` +
            `  期望路径：${signature}\n` +
            `  签名由构建时的 TAURI_SIGNING_PRIVATE_KEY 生成，没有它客户端不会安装。`,
        );
      }

      found.push({ file, artifact, signature, installerType: entry });
    }
  }

  if (found.length === 0) {
    fail(`产物目录里没有安装包：${bundleDir}`);
  }
  return found;
}

/**
 * 平台键。
 *
 * Tauri 的清单用 `OS-ARCH` 形式，Windows 上是 `windows-x86_64`。
 * 带安装器类型的后缀（`windows-x86_64-nsis`）优先级更高，
 * 插件会先找带类型的，找不到再退回不带类型的。
 */
function platformKey(target) {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  return `windows-${arch}`;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version ?? readVersion();
  const baseUrl = (args.baseUrl ?? `${COS_BASE}`).replace(/\/+$/, "");

  // 输出路径：默认写到 bundle 目录，也可以用 --out 指定。
  // CI 里要生成两份（COS 与 GitHub 各一份，URL 不同），所以必须能分开写。
  // 两份分别落到不同目录，文件名都叫 `latest.json`——
  // GitHub 端点的 URL 是固定的 `.../releases/latest/download/latest.json`，
  // 资产名必须一致；而 COS 上传时也要叫 `latest.json`。
  // 绝对路径直接用，相对路径相对仓库根解析。
  const outPath = args.out
    ? isAbsolute(args.out)
      ? args.out
      : join(ROOT, args.out)
    : MANIFEST;
  mkdirSync(dirname(outPath), { recursive: true });

  const artifacts = collectArtifacts();

  const platforms = {};
  for (const item of artifacts) {
    const signature = readFileSync(item.signature, "utf8").trim();
    if (!signature) {
      fail(`签名文件是空的：${item.signature}`);
    }

    const key = platformKey();
    // 同平台有多个安装包时（nsis + msi），NSIS 优先——
    // 它是我们 `bundle.targets` 里配置的那个。
    if (platforms[key] && item.installerType !== "nsis") continue;

    platforms[key] = {
      signature,
      url: `${baseUrl}/${item.file}`,
    };
  }

  const manifest = {
    version,
    notes: args.notes,
    pub_date: new Date().toISOString(),
    platforms,
  };

  writeFileSync(outPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

  console.log(`[gen-updater-manifest] 已生成 ${outPath}`);
  console.log(`  version: ${version}`);
  console.log(`  base:    ${baseUrl}`);
  for (const [key, value] of Object.entries(platforms)) {
    console.log(`  ${key}: ${value.url}`);
  }
}

main();
