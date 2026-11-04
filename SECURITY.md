# Security Policy / 安全策略

## 支持的版本

| 版本 | 状态 |
| :--- | :--- |
| `0.1.x` | ✅ 接收安全报告与修复 |

## 报告安全问题

发现漏洞请通过 GitHub 仓库的 **Security → Report a vulnerability** 私下报告，
不要开公开 Issue。修复后会在 CHANGELOG 中致谢（除非你希望匿名）。

---

## 分发信任：怎么确认你手上的 Sereno 是官方版

Sereno 完全免费、无广告、无遥测。正因为没有变现渠道，市面上出现的
「Sereno 高级版 / Sereno Pro / Sereno 公益服客户端」**全部是二次修改版**，
其中一部分塞了广告或劫持了账号登录。

请只从官方渠道获取，并在安装前核对一次：

### 1. 核对 SHA-256

每个 Release 都附有 `SHA256SUMS.txt`。在 PowerShell / CMD 里执行：

```powershell
certutil -hashfile .\Sereno_0.1.0_x64-setup.exe SHA256
```

把输出与 `SHA256SUMS.txt` 中同名那一行的哈希逐字符比对。
**完全一致**才是官方原件。

发布者也可以自行校验：

```bash
node scripts/gen-checksums.mjs --verify
```

### 2. 看代码签名

官方安装包带 Authenticode 签名。在文件属性 → 「数字签名」里，
发布者应为 `zuoqiuning`（或项目后续公告的发布者）。

> 未签名或签名与发布者不符的文件，请立即删除，并到 Issues 反馈。

### 3. 识别二次修改版

- 界面上出现了官方版本里**没有**的功能（会员、加速、皮肤站、公益服入口）；
- 出现了需要额外下载的「组件」「驱动」「登录器」；
- 启动后要求你交出正版账号密码给第三方域名；
- 进程列表里出现了项目里没有的额外可执行文件。

官方版本**永远不会**要求你把正版账号交给除 `microsoft.com` /
`microsoftonline.com` / `live.com` 之外的任何域名——
这条规则在代码里是硬编码白名单，并有自动化检查守着（`scripts/security-check.mjs`）。

---

## 授权红线

本项目采用 **PolyForm Noncommercial License 1.0.0**：

- ✅ 允许：个人学习、研究、修改、二次开发、非商业分发；
- ❌ 禁止：任何形式的商业售卖、套壳私服盈利、投放商业广告、捆绑付费插件、
  将本项目用于任何以盈利为目的的分发。

如果你在做二次开发并因此收到律师函或平台投诉，请先确认自己的用法是否越过了
非商业边界——把代码还给原作者是最省事的做法。

完整的协议文本见 [LICENSE](LICENSE)。

---

## 我们**不会**做的事

这几条是 Sereno 的产品底线，任何版本都不会改变：

- ❌ 不做任何遥测或用户行为上报；
- ❌ 不做强制联网验证、不做登录后才能使用；
- ❌ 不在界面上投放广告、不接第三方推广 SDK；
- ❌ 不读取与启动游戏无关的文件。

因此，请不要相信任何声称「Sereno 官方需要登录/激活/授权」的说法——
那一定是假的。

## 开源贡献的安全门槛

仓库的 CI 会在每次提交与 PR 上运行：

```bash
pnpm check:security     # 授权声明 / 注入面 / Tauri 权限 / CSP / 产物洁净度
cargo clippy -- -D warnings
pnpm typecheck && pnpm lint && pnpm test
```

其中 `check:security` 会检查注入面（`eval` / `v-html` / 远程脚本）、
Tauri 权限白名单、CSP 是否被放宽、以及发布产物里是否残留 sourcemap 与 `console`。
**新增上述能力时，请一并更新脚本里的白名单，而不是绕过它。**
