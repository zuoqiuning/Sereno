# Sereno

> [!WARNING]
> **本项目仅供个人学习、研究与非商业交流使用，严禁任何形式的商业售卖、套壳私服盈利、植入广告或捆绑商业插件。**
> Sereno 是一款完全免费、无广告、无遥测、不联机验证的单机启动器。
> 欢迎随意参考与借鉴本项目的代码与架构设计；若能注明「参考自 Sereno」，不胜感激。

![version](https://img.shields.io/badge/version-0.1.0-0F64B5?style=flat-square)
![platform](https://img.shields.io/badge/platform-Windows-4C6EF5?style=flat-square)
![license](https://img.shields.io/badge/license-PolyForm%20NC%201.0.0-0F64B5?style=flat-square)
![rust](https://img.shields.io/badge/rust-1.99-orange?style=flat-square)
![tauri](https://img.shields.io/badge/tauri-2.12-24C8D8?style=flat-square)
![vue](https://img.shields.io/badge/vue-3.5-4FC08D?style=flat-square)

**一个安静、克制的 Minecraft 启动器。**
**A quiet, minimalist Minecraft launcher.**

Sereno 只做启动器该做的事：把游戏装好、把 Mod 管好、出问题时告诉你哪里坏了。
界面尽量少放东西，能用一步完成的就不放两步。

<!-- markdownlint-disable MD033 -->

| | |
| :--- | :--- |
| [功能特性](#功能特性--features) | [截图](#截图--screenshots) |
| [快速上手](#快速上手--getting-started) | [启用微软正版登录](#启用微软正版登录--enabling-microsoft-login) |
| [支持作者](#支持作者--support) | [开源协议](#开源协议--license) |
| [特别鸣谢](#特别鸣谢--acknowledgements) | |

---

## 功能特性 / Features

- 原版游戏与 Fabric / Quilt / Forge / NeoForge / OptiFine 的安装与启动
- 微软正版登录与离线档案，令牌保存在系统凭据管理器
  **注意：微软正版登录需要你自行在 Azure 注册应用**，见
  [启用微软正版登录](#启用微软正版登录--enabling-microsoft-login)
- 第三方登录（authlib-injective / Yggdrasil）：支持 LittleSkin、Ely.by 等兼容的皮肤站
- Mod、整合包、资源包、光影的搜索与下载，聚合 Modrinth 与 CurseForge
- **CurseForge 整合包安装**：解析 `manifest.json`，逐文件校验 SHA-1
- **从其它启动器导入实例**：认得 PCL / HMCL / MultiMC / Prism 的目录结构，
  把 Mod、配置与存档一并搬过来
- 实例健康检查：Java、内存、磁盘、文件完整性、Mod 依赖与最近崩溃一次说清
- 世界备份、日志查看、崩溃自动诊断与诊断包导出
- 下载限速、镜像优先、断点续传与增量校验
- **启动器自身的更新**：更新包经 minisign 数字签名校验，验签不过绝不安装

<details>
<summary><b>English</b></summary>

- Install and launch Vanilla, Fabric, Quilt, Forge, NeoForge, and OptiFine.
- Microsoft OAuth 2.0 login (Device Code Flow) and offline accounts; tokens are stored in the system keychain.
  **Note:** Microsoft login needs your own Azure app registration — see
  [Enabling Microsoft login](#启用微软正版登录--enabling-microsoft-login).
- Third-party login via authlib-injective / Yggdrasil (LittleSkin, Ely.by, and any compatible server).
- Search and download mods, modpacks, resource packs, and shaders from Modrinth and CurseForge.
- **Install CurseForge modpacks** — `manifest.json` is resolved through the CF API, with per-file SHA-1 verification.
- **Import existing instances** from PCL, HMCL, MultiMC, Prism, or the official launcher — mods, configs and saves come along.
- Instance health checks: Java, memory, disk space, file integrity, mod dependencies, and recent crash diagnostics.
- World backups, log viewer, crash diagnostics, and diagnostic package export.
- Download speed limiting, mirror preference, resumable downloads, and incremental verification.
- **In-app updates** — update bundles are signed with minisign and verified with a public key embedded
  in the app. A bundle that fails verification is never installed.

</details>

---

## 截图 / Screenshots

![Sereno 启动界面 / Home screen](assets/screenshots/home.png)

最近游玩的实例会被放到舞台中央，点一下就能进入游戏。
The last-played instance takes the stage — one click to launch.

---

## 快速上手 / Getting Started

`0.1.0`，仍在持续开发中，暂未发布安装包。
`0.1.0`, still under active development. No installer released yet.

遇到问题或有功能建议，欢迎在 [Issues](https://github.com/zuoqiuning/Sereno/issues) 里提出。

---

## 启用微软正版登录 / Enabling Microsoft Login

离线档案开箱即用；**微软正版登录不随包附带客户端 ID**——用别人的 Azure 应用等于
把你的 Minecraft 账号经由第三方回调地址走一遍。请自行注册一个免费应用：

Offline accounts work out of the box. Microsoft login does **not** ship with a
client ID — using someone else's Azure application would route your Minecraft
account through a third party's redirect URI. Register your own free Azure app
and point Sereno at it:

1. Azure 门户 → **应用注册** → **新注册**
   / Azure Portal → **App registrations** → **New registration**
2. 名称随意；**支持的账户类型**选「个人 Microsoft 账户」
   / Name it anything; set **Supported account types** to *Personal Microsoft accounts*
3. 添加 **Web** 重定向 URI：`https://login.live.com/oauth20_desktop.srf`
   / Add a **Web** redirect URI: `https://login.live.com/oauth20_desktop.srf`
4. 复制**应用程序（客户端）ID**
   / Copy the **Application (client) ID**
5. 用该值启动 Sereno：

```
set SERENO_MS_CLIENT_ID=<你的客户端 ID>
sereno.exe
```

未配置时，界面会明确提示「登录尚未开放」，而不是在登录流程中途失败。

Until then the launcher says "login is not configured yet" up front, instead of
failing halfway through the sign-in flow.

---

## 支持作者 / Support

如果 Sereno 帮你省下了一点折腾的时间，欢迎请我喝杯奶茶。
赞助不会解锁任何额外功能，只是让我更有底气把它一直维护下去。

爱发电主页：https://ifdian.net/a/Sereno

---

## 开源协议 / License

**PolyForm Noncommercial License 1.0.0** —— 你可以自由地学习、修改，
并在**任何非商业目的**下使用与分发；但**严禁任何形式的商业使用**
（包括但不限于套壳售卖、绑定私服充值、投放商业广告、捆绑付费插件）。
详见 [LICENSE](LICENSE)。

> 如果你再分发本项目的任何部分，必须保留协议文本（或其链接），
> 且不得以任何方式出售、捆绑或变现。
> 从非官方渠道发布、附带广告或付费功能的版本是对这些条款的违反，
> 而不叫「二次开发」。

Sereno 本身完全免费、无广告、无遥测、不联机验证，也不会因为你没登录任何账号而影响使用。
本软件与 Mojang Studios、Microsoft、网易公司无关。

This project is open-source under the [PolyForm Noncommercial License 1.0.0](LICENSE): study it, modify it,
and use it for any non-commercial purpose — but **commercial use of any kind is not permitted**
(including selling a repackaged build, monetized private servers, ads, or bundled paid plugins).

If you redistribute any part of this project, you must keep the license text (or its URL) included,
and you may not sell, bundle or monetize it in any way. A copy published from an unofficial channel
with ads or paid features attached is a violation of these terms, not a "fork".

If you find Sereno's code or mechanisms helpful and use them in your project, we would really appreciate it
if you could mention "Feature X inspired by `https://github.com/zuoqiuning/Sereno`" in your documentation.
This is completely optional and not a legal requirement, but it means a lot to us. Thanks!

Sereno is free, ad-free, telemetry-free and never phones home. It is not affiliated with
Mojang Studios, Microsoft or NetEase.

---

## 特别鸣谢 / Acknowledgements

- **[MCIM](https://www.mcimirror.top)** —— 面向国内用户的 Modrinth 与
  CurseForge 元数据及文件镜像。正是因为它，Sereno 才能在**不配置任何 API Key**
  的前提下访问 CurseForge。
  [赞助 MCIM](https://www.mcimirror.top/sponsor)
- **[BMCLAPI](https://bmclapi2.bangbang93.com/)** —— 游戏本体、加载器与
  Maven 依赖的国内加速
- **[PCL2](https://github.com/LTCat/PCL)** —— 加载器离线安装、镜像优先与
  实例资源管理的思路参考
- **[minecraft-svg-vue](https://github.com/zuoqiuning/minecraft-svg-vue)** ——
  界面上的 Minecraft 原版像素图标

---

Made by 丘屿知宁
