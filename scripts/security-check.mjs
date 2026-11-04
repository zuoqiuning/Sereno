#!/usr/bin/env node
/**
 * 发布前安全自检。
 *
 * ## 为什么需要它
 *
 * 「防止别人在 Sereno 里塞广告 / 投毒」这件事，靠协议和口头声明是不够的——
 * 协议只对愿意遵守的人有效。真正能落地的做法是把安全约束**写成可执行的检查**，
 * 让每一次提交都必须通过，这样约束不会随时间腐化。
 *
 * 这个脚本覆盖四类约束：
 *
 * 1. **授权层**：LICENSE 必须是 PolyForm Noncommercial，且带 PolyForm 要求的
 *    `Required Notice:`；README 顶部有非商业声明——防止有人无意间把协议改回去。
 * 2. **注入面**：前端不得出现 `eval` / `new Function` / `v-html` / 远程脚本与样式。
 *    这几条是「远程广告 / 远程脚本」能进到界面里的唯一通道。
 * 3. **权限面**：Tauri capabilities 不得出现 shell / fs / http 等高危权限；
 *    CSP 必须锁死 `script-src 'self'`、`object-src 'none'`、`frame-ancestors 'none'`；
 *    前端不得直连外网（`connect-src` 只允许 IPC）。
 * 4. **构建面**：release 必须 strip 符号、用 LTO；前端发布产物不得带 sourcemap、
 *    不得残留 `console`；依赖不得有 `postinstall` 之类的生命周期脚本。
 *
 * 用法：`node scripts/security-check.mjs`
 * 退出码：0 全部通过；1 有失败项。
 */

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

const SKIP_DIRS = new Set([
  "node_modules",
  "dist",
  "target",
  ".git",
  ".workbuddy",
  "PCL参考代码",
]);

const results = [];
let failed = 0;

function readText(rel) {
  return readFileSync(join(ROOT, rel), "utf8");
}

function walk(dir, out = [], exts = null) {
  if (!existsSync(dir)) return out;
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry)) continue;
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      walk(full, out, exts);
    } else if (!exts || exts.includes(entry.slice(entry.lastIndexOf(".")))) {
      out.push(full);
    }
  }
  return out;
}

function check(group, desc, fn) {
  let ok = false;
  let detail = "";
  try {
    const result = fn();
    if (result === true) {
      ok = true;
    } else if (result && typeof result === "object") {
      ok = Boolean(result.ok);
      detail = result.detail ?? "";
    }
  } catch (err) {
    ok = false;
    detail = err instanceof Error ? err.message : String(err);
  }
  results.push({ group, desc, ok, detail });
  if (!ok) failed += 1;
}

/** 在一组文件里找匹配，返回前若干条命中摘要。 */
function grepFiles(files, pattern) {
  const hits = [];
  const re = pattern instanceof RegExp ? pattern : new RegExp(pattern);
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      if (re.test(lines[i])) {
        hits.push(`${relative(ROOT, file)}:${i + 1}`);
        if (hits.length >= 6) return hits;
      }
    }
  }
  return hits;
}

// ---------------------------------------------------------------- 收集文件
const vueFiles = walk(join(ROOT, "apps/desktop/src"), [], [".vue", ".ts"]);
const rustFiles = walk(join(ROOT, "crates"), [], [".rs"]);
const shellFiles = walk(join(ROOT, "apps/desktop/src-tauri/src"), [], [".rs"]);

// ---------------------------------------------------------------- 1. 授权层
const licenseText = existsSync(join(ROOT, "LICENSE")) ? readText("LICENSE") : "";
const readmeText = existsSync(join(ROOT, "README.md")) ? readText("README.md") : "";

check("授权", "LICENSE 使用 PolyForm Noncommercial 1.0.0", () =>
  licenseText.includes("PolyForm Noncommercial License 1.0.0")
    ? true
    : { ok: false, detail: "LICENSE 不是 PolyForm Noncommercial 1.0.0" },
);

check("授权", "LICENSE 带 PolyForm 要求的 Required Notice", () =>
  licenseText.includes("Required Notice:")
    ? true
    : { ok: false, detail: "缺少 Required Notice:" },
);

check("授权", "README 顶部有非商业使用声明", () => {
  const head = readmeText.slice(0, 1200);
  return head.includes("非商业") || head.includes("commercial")
    ? true
    : { ok: false, detail: "README 前 1200 字符内没有非商业声明" };
});

check("授权", "package.json 未声明 MIT", () => {
  const pkg = JSON.parse(readText("apps/desktop/package.json"));
  return pkg.license !== "MIT"
    ? true
    : { ok: false, detail: `license 仍是 ${pkg.license}` };
});

check("授权", "安装向导展示许可文件", () => {
  // PolyForm Noncommercial 不是 OSI 认可的协议，
  // 用户必须在装之前就看到「禁止商业使用」这一条。
  const conf = JSON.parse(readText("apps/desktop/src-tauri/tauri.conf.json"));
  const rel = conf.bundle?.licenseFile;
  if (!rel) return { ok: false, detail: "bundle.licenseFile 未配置" };
  return existsSync(join(ROOT, "apps/desktop/src-tauri", rel))
    ? true
    : { ok: false, detail: `找不到 ${rel}` };
});

// ---------------------------------------------------------------- 2. 注入面
check("注入面", "前端不含 eval(", () => {
  const hits = grepFiles(vueFiles, /\beval\s*\(/);
  return hits.length === 0 ? true : { ok: false, detail: hits.join(", ") };
});

check("注入面", "前端不含 new Function(", () => {
  const hits = grepFiles(vueFiles, /new\s+Function\s*\(/);
  return hits.length === 0 ? true : { ok: false, detail: hits.join(", ") };
});

check("注入面", "前端不含 v-html / innerHTML 注入", () => {
  const hits = grepFiles(vueFiles, /v-html|\.innerHTML\s*=/);
  return hits.length === 0 ? true : { ok: false, detail: hits.join(", ") };
});

check("注入面", "前端不加载远程脚本", () => {
  const hits = grepFiles(vueFiles, /<script[^>]+src\s*=\s*["']https?:/i);
  return hits.length === 0 ? true : { ok: false, detail: hits.join(", ") };
});

check("注入面", "前端不加载远程样式表", () => {
  const hits = grepFiles(vueFiles, /<link[^>]+href\s*=\s*["']https?:/i);
  return hits.length === 0 ? true : { ok: false, detail: hits.join(", ") };
});

check("注入面", "远程图片一律带 referrerpolicy=no-referrer", () => {
  const files = vueFiles.filter((f) => f.endsWith(".vue"));
  const offenders = [];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    const re = /<img\b[^>]*>/g;
    for (const match of text.matchAll(re)) {
      const tag = match[0];
      if (/:src\s*=\s*["']https?:/i.test(tag) && !/referrerpolicy/i.test(tag)) {
        offenders.push(`${relative(ROOT, file)}`);
      }
    }
  }
  return offenders.length === 0
    ? true
    : { ok: false, detail: offenders.slice(0, 6).join(", ") };
});

// ---------------------------------------------------------------- 3. 权限面
const tauriConf = JSON.parse(readText("apps/desktop/src-tauri/tauri.conf.json"));
const capabilities = JSON.parse(readText("apps/desktop/src-tauri/capabilities/main.json"));

check("权限", "capabilities 不含 shell / fs / http / process 权限", () => {
  const danger = capabilities.permissions.filter((p) =>
    /^(shell|fs|http|process|os|upload|clipboard-manager):/.test(p),
  );
  return danger.length === 0
    ? true
    : { ok: false, detail: danger.join(", ") };
});

check("权限", "未引入 tauri-plugin-shell", () => {
  const cargo = readText("apps/desktop/src-tauri/Cargo.toml");
  return cargo.includes("tauri-plugin-shell")
    ? { ok: false, detail: "Cargo.toml 里有 tauri-plugin-shell" }
    : true;
});

check("权限", "未启用 withGlobalTauri", () =>
  tauriConf.app.withGlobalTauri === false
    ? true
    : { ok: false, detail: "withGlobalTauri 不是 false" },
);

const csp = tauriConf.app.security?.csp ?? "";

check("权限", "CSP 的 script-src 锁死为 'self'", () => {
  const m = /script-src([^;]*)/.exec(csp);
  const value = m?.[1]?.trim() ?? "";
  if (!value) return { ok: false, detail: "CSP 里没有 script-src" };
  if (/unsafe-eval|unsafe-inline|https?:|\*/.test(value)) {
    return { ok: false, detail: `script-src 含危险源: ${value}` };
  }
  return value.includes("'self'")
    ? true
    : { ok: false, detail: `script-src 不是 'self': ${value}` };
});

check("权限", "CSP 含 object-src 'none'", () =>
  /object-src\s+'none'/.test(csp) ? true : { ok: false, detail: csp },
);

check("权限", "CSP 含 frame-ancestors 'none'", () =>
  /frame-ancestors\s+'none'/.test(csp) ? true : { ok: false, detail: csp },
);

check("权限", "CSP 含 frame-src 'none'", () =>
  /frame-src\s+'none'/.test(csp) ? true : { ok: false, detail: csp },
);

check("权限", "CSP 的 connect-src 不允许直连外网", () => {
  const m = /connect-src([^;]*)/.exec(csp);
  const value = m?.[1]?.trim() ?? "";
  if (!value) return { ok: false, detail: "CSP 里没有 connect-src" };
  // Tauri 的 IPC 通道（ipc: 与 Windows 上的 http://ipc.localhost）是必需的，
  // 其余任何 http(s) / 通配符都意味着前端可以直接联网。
  const stripped = value
    .replace(/ipc:/g, "")
    .replace(/http:\/\/ipc\.localhost/g, "")
    .trim();
  return /https?:|\*/.test(stripped)
    ? { ok: false, detail: `connect-src 含外网源: ${value}` }
    : true;
});

check("权限", "鸣谢 / 仓库链接是硬编码字面量", () => {
  const links = readText("apps/desktop/src-tauri/src/commands/links.rs");
  // 不允许用 format! 拼 URL——拼接意味着某一段可能来自外部数据
  if (/format!\([^)]*https?:/.test(links)) {
    return { ok: false, detail: "links.rs 里出现了拼接出来的 URL" };
  }
  const urls = [...links.matchAll(/https?:\/\/[^"']+/g)].map((m) => m[0]);
  return urls.length > 0
    ? true
    : { ok: false, detail: "links.rs 里没有找到硬编码 URL" };
});

check("权限", "微软登录页有域名白名单校验", () => {
  const account = readText("apps/desktop/src-tauri/src/commands/account.rs");
  const guarded = account.includes("ALLOWED") && account.includes("starts_with");
  if (!guarded) {
    return { ok: false, detail: "account.rs 里没有发现 ALLOWED / starts_with 校验" };
  }
  // 白名单里只允许微软域名
  const domains = [...account.matchAll(/"(https:\/\/[^"]+)"/g)].map((m) => m[1]);
  const bad = domains.filter(
    (d) =>
      !/^https:\/\/(www\.|login\.)?(microsoft\.com|microsoftonline\.com|live\.com)\//.test(d),
  );
  return bad.length === 0 ? true : { ok: false, detail: bad.join(", ") };
});

check("权限", "资源项目页只用固定域前缀拼接", () => {
  const resource = readText("apps/desktop/src-tauri/src/commands/resource.rs");
  // 只看「打开项目页」那一段：最后一个 opener::open 之前的 URL 构造
  const idx = resource.lastIndexOf("opener::open");
  if (idx < 0) return { ok: false, detail: "resource.rs 里没有 opener::open" };
  const window = resource.slice(Math.max(0, idx - 900), idx);
  const urls = [...window.matchAll(/https:\/\/[^"')[\s]+/g)].map((m) => m[0]);
  const bad = urls.filter((u) => !/^https:\/\/(www\.)?(modrinth\.com|curseforge\.com)\//.test(u));
  return bad.length === 0 ? true : { ok: false, detail: bad.join(", ") };
});

check("权限", "Rust 侧不拼接 shell 命令", () => {
  const hits = grepFiles(rustFiles, /Command::new\("sh"|Command::new\("cmd"|\.arg\("-c"\)/);
  return hits.length === 0 ? true : { ok: false, detail: hits.join(", ") };
});

check("权限", "更新端点只信任官方域名", () => {
  const conf = readText("apps/desktop/src-tauri/tauri.conf.json");
  // 只取 updater 段里的端点——别的 URL（如 $schema）不参与更新。
  const section = conf.match(/"updater"\s*:\s*\{[\s\S]*?\n\s{4}\}/);
  if (!section) {
    return { ok: false, detail: "tauri.conf.json 里没有 plugins.updater 段" };
  }
  const endpoints = [...section[0].matchAll(/https:\/\/([^/"\s]+)/g)].map((m) => m[1]);
  const allowed = new Set([
    // 腾讯云 COS（国内下载源）
    "sereno-updates-1316381240.cos.ap-beijing.myqcloud.com",
    // GitHub Release（兜底源）
    "github.com",
  ]);
  const bad = endpoints.filter((d) => !allowed.has(d));
  return bad.length === 0
    ? true
    : { ok: false, detail: `更新端点出现了非白名单域名: ${bad.join(", ")}` };
});

check("权限", "更新通道启用了 minisign 签名校验", () => {
  const conf = readText("apps/desktop/src-tauri/tauri.conf.json");
  // pubkey 必须存在且非空
  const pubkey = conf.match(/"pubkey"\s*:\s*"([^"]+)"/);
  if (!pubkey || !pubkey[1].trim()) {
    return { ok: false, detail: "tauri.conf.json 里没有配置 updater.pubkey" };
  }
  // 必须开启版本一致性校验（防降级攻击）
  if (!/"requireSignedVersion"\s*:\s*true/.test(conf)) {
    return {
      ok: false,
      detail: "没有开启 requireSignedVersion——攻击者可用旧包的合法签名把用户降级",
    };
  }
  // 不允许关闭传输安全
  if (/dangerousInsecureTransportProtocol"\s*:\s*true/.test(conf)) {
    return { ok: false, detail: "开启了 dangerousInsecureTransportProtocol" };
  }
  return true;
});

check("权限", "更新私钥不在仓库里", () => {
  // 私钥必须是「环境变量引用」或「文件路径」，不能是密钥本体。
  // 扫全部受版本控制的文本文件，命中即视为泄漏。
  const scanned = [
    ...vueFiles,
    ...rustFiles,
    ...shellFiles,
    ...walk(join(ROOT, "scripts"), [], [".mjs", ".js"]),
    ...walk(join(ROOT, ".github/workflows"), [], [".yml", ".yaml"]),
  ];
  const hits = grepFiles(scanned, /TAURI_SIGNING_PRIVATE_KEY\s*=\s*[A-Za-z0-9+/=]{40,}/);
  return hits.length === 0
    ? true
    : { ok: false, detail: `疑似私钥泄漏在: ${hits.join(", ")}` };
});

check("权限", "更新安装包必须通过签名校验", () => {
  // 构建时必须产出 .sig，且清单脚本必须校验它存在
  const script = readText("scripts/gen-updater-manifest.mjs");
  if (!script.includes(".sig")) {
    return { ok: false, detail: "清单脚本没有引用 .sig 签名文件" };
  }
  // 命令层不能自己下载——下载与验签都由插件完成
  const cmd = readText("apps/desktop/src-tauri/src/commands/update.rs");
  if (/reqwest|download_url|https:\/\//.test(cmd)) {
    return {
      ok: false,
      detail: "命令层自己发起下载或拼了 URL——必须走插件的验签通道",
    };
  }
  return true;
});

check("权限", "更新下载地址不由前端传入", () => {
  const cmd = readText("apps/desktop/src-tauri/src/commands/update.rs");
  // 命令层不能接收 url 参数
  if (/url\s*:\s*String/.test(cmd)) {
    return { ok: false, detail: "命令层出现了 url 参数——前端可以构造任意下载地址" };
  }
  // 前端 API 层同样不能有 url 参数
  const api = readText("apps/desktop/src/api/update.ts");
  if (/url\s*:\s*string/i.test(api)) {
    return { ok: false, detail: "前端 API 层出现了 url 参数" };
  }
  return true;
});

check("权限", "更新产物必须签名后才发布", () => {
  const workflow = readText(".github/workflows/release.yml");
  if (!workflow.includes("TAURI_SIGNING_PRIVATE_KEY")) {
    return { ok: false, detail: "发布流程没有注入签名私钥" };
  }
  // 必须检查 .sig 产物存在
  if (!workflow.includes(".sig")) {
    return { ok: false, detail: "发布流程没有检查签名产物" };
  }
  return true;
});

check("权限", "第三方登录令牌只进系统密钥库", () => {
  const authlib = readText("crates/sereno-core/src/account/authlib.rs");
  // 令牌不得写进 settings.json
  if (/settings\.account\.[a-z_]*token/.test(authlib)) {
    return { ok: false, detail: "令牌被写进了设置文件" };
  }
  if (!authlib.includes("secure_store") && !authlib.includes("SecureStore")) {
    return { ok: false, detail: "authlib.rs 里没有用到系统密钥库" };
  }
  return true;
});

check("权限", "第三方登录的密码不落盘", () => {
  const authlib = readText("crates/sereno-core/src/account/authlib.rs");
  const cmd = readText("apps/desktop/src-tauri/src/commands/account.rs");
  // 命令层不能把密码存进设置
  if (/next\.[a-z_.]*password\s*=/.test(cmd) && !/password:\s*String/.test(cmd)) {
    return { ok: false, detail: "命令层疑似把密码写进了设置" };
  }
  // 认证函数里密码只作为参数出现，不参与持久化
  const persistCount = (authlib.match(/store\.set\(/g) || []).length;
  const storeCalls = [...authlib.matchAll(/store\.set\(([^)]*)\)/g)].map((m) => m[1]);
  const storesPassword = storeCalls.some((call) => call.includes("password"));
  if (storesPassword) {
    return { ok: false, detail: "密码被写进了凭据存储" };
  }
  if (persistCount === 0) {
    return { ok: false, detail: "authlib.rs 里没有找到凭据存储调用" };
  }
  return true;
});

check("权限", "第三方登录的 API Root 有格式校验", () => {
  const authlib = readText("crates/sereno-core/src/account/authlib.rs");
  if (!authlib.includes("validate_api_root")) {
    return { ok: false, detail: "authlib.rs 里没有 API Root 校验" };
  }
  // 校验函数必须拒绝非 http(s) 协议
  const fn = authlib.slice(
    authlib.indexOf("pub fn validate_api_root"),
    authlib.indexOf("pub fn validate_api_root") + 900,
  );
  if (!fn.includes("https://") || !fn.includes("http://")) {
    return { ok: false, detail: "API Root 校验没有限定协议" };
  }
  return true;
});

check("注入面", "注入器下载后必须校验哈希", () => {
  const authlib = readText("crates/sereno-core/src/account/authlib.rs");
  if (!authlib.includes("sha256")) {
    return { ok: false, detail: "注入器下载没有校验 SHA-256" };
  }
  // 校验失败必须返回错误，不能继续
  if (!authlib.includes("ChecksumMismatch")) {
    return { ok: false, detail: "注入器校验失败时没有返回错误" };
  }
  return true;
});

// ---------------------------------------------------------------- 4. 构建面
const cargoToml = readText("Cargo.toml");

check("构建", "release profile 去掉了符号表", () => {
  const m = /\[profile\.release\]([\s\S]*?)(?=\n\[|$)/.exec(cargoToml);
  const section = m?.[1] ?? "";
  return /strip\s*=/.test(section)
    ? true
    : { ok: false, detail: "release profile 里没有 strip" };
});

check("构建", "release profile 启用了 LTO", () => {
  const m = /\[profile\.release\]([\s\S]*?)(?=\n\[|$)/.exec(cargoToml);
  const section = m?.[1] ?? "";
  return /lto\s*=/.test(section)
    ? true
    : { ok: false, detail: "release profile 里没有 lto" };
});

const viteConfig = readText("apps/desktop/vite.config.ts");

check("构建", "前端发布产物不产出 sourcemap", () =>
  /sourcemap:\s*false/.test(viteConfig)
    ? true
    : { ok: false, detail: "vite.config.ts 里没有 sourcemap: false" },
);

check("构建", "前端使用 terser 混淆", () =>
  /minify:\s*["']terser["']/.test(viteConfig)
    ? true
    : { ok: false, detail: "vite.config.ts 里 minify 不是 terser" },
);

check("构建", "依赖没有安装期生命周期脚本", () => {
  for (const rel of ["package.json", "apps/desktop/package.json"]) {
    const pkg = JSON.parse(readText(rel));
    for (const key of ["preinstall", "install", "postinstall", "prepare"]) {
      if (pkg.scripts?.[key]) {
        return { ok: false, detail: `${rel} 含 ${key} 脚本` };
      }
    }
  }
  return true;
});

// ---------------------------------------------------------------- 5. 产物
const distDir = join(ROOT, "apps/desktop/dist");
if (existsSync(distDir)) {
  const distFiles = walk(distDir, [], [".js", ".css", ".map", ".html"]);
  const maps = distFiles.filter((f) => f.endsWith(".map"));
  check("产物", "产物中没有 sourcemap 文件", () =>
    maps.length === 0
      ? true
      : { ok: false, detail: maps.map((f) => relative(ROOT, f)).join(", ") },
  );

  const jsFiles = distFiles.filter((f) => f.endsWith(".js"));
  const consoleHits = grepFiles(jsFiles, /console\.(log|warn|error|info|debug)\(/);
  check("产物", "产物中没有残留 console 调用", () =>
    consoleHits.length === 0
      ? true
      : { ok: false, detail: consoleHits.join(", ") },
  );

  const mapRefs = grepFiles(jsFiles, /sourceMappingURL/);
  check("产物", "产物中没有 sourceMappingURL 引用", () =>
    mapRefs.length === 0
      ? true
      : { ok: false, detail: mapRefs.join(", ") },
  );
}

// ---------------------------------------------------------------- 输出
const useColor = process.stdout.isTTY && !process.env.NO_COLOR;
const paint = (code, text) => (useColor ? `\u001b[${code}m${text}\u001b[0m` : text);

let lastGroup = "";
for (const item of results) {
  if (item.group !== lastGroup) {
    lastGroup = item.group;
    console.log(`\n${paint("1", `[${item.group}]`)}`);
  }
  const mark = item.ok ? paint("32", "  ✓") : paint("31", "  ✗");
  console.log(`${mark} ${item.desc}`);
  if (!item.ok && item.detail) {
    console.log(`      ${paint("90", item.detail)}`);
  }
}

console.log(
  `\n${failed === 0 ? paint("32", "全部通过") : paint("31", `${failed} 项未通过`)}：` +
    `${results.length - failed}/${results.length}`,
);

process.exit(failed === 0 ? 0 : 1);
