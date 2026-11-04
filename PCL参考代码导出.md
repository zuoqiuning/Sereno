# PCL 参考代码导出

> 本文件由 `scripts/export-code.mjs` 自动生成，**请勿手工编辑**。
> 每次运行会覆盖同名文件。

| 项 | 值 |
| :--- | :--- |
| 生成时间 | 2026-11-05 00:27:25 |
| 导出文件数 | 10 |
| 导出总行数 | 1302 |

---

## 文件清单（完整路径）

### 📁 `（根目录）`

| # | 路径 | 行数 |
| ---: | :--- | ---: |
| 1 | `_索引.md` | 172 |
| 2 | `机制借鉴与优化建议.md` | 495 |

### 📁 `00-工程配置`

| # | 路径 | 行数 |
| ---: | :--- | ---: |
| 3 | `00-工程配置/PCLCS.csproj` | 70 |
| 4 | `00-工程配置/Plain Craft Launcher 2.sln` | 65 |

### 📁 `03-启动与版本`

| # | 路径 | 行数 |
| ---: | :--- | ---: |
| 5 | `03-启动与版本/Java.cs` | 253 |
| 6 | `03-启动与版本/LaunchUtils.cs` | 31 |

### 📁 `04-资源与本地Mod`

| # | 路径 | 行数 |
| ---: | :--- | ---: |
| 7 | `04-资源与本地Mod/Resource.cs` | 32 |
| 8 | `04-资源与本地Mod/WikiEntry.cs` | 83 |

### 📁 `06-基础工具`

| # | 路径 | 行数 |
| ---: | :--- | ---: |
| 9 | `06-基础工具/Configs.cs` | 20 |
| 10 | `06-基础工具/Constants.cs` | 81 |

---

## 代码正文

### 📁 `（根目录）`

#### `_索引.md`

```markdown
# PCL2 参考代码索引

本目录从 `PCL源码/PCL-main` 中**原样复制**关键功能机制代码，未修改任何一行内容（复制后已逐文件校验 SHA-256，全部一致）。

- 源码版本：PCL2 GitHub 仓库快照（VB.NET + WPF）
- 语言分布：112 个 `.vb`、6 个 `.cs`、63 个 `.xaml`
- 本目录共 48 个源码 / 工程文件，按机制分类存放
- 复制范围：网络/下载、任务调度、加载器与整合包安装、启动与版本、Java、资源与本地 Mod、实例文件夹管理、下载进度状态、桌面窗口框架、设置体系、登录与基础工具
- 未复制：纯 XAML 界面、控件样式、图片/语言资源、更新补丁；`MeloongCore` 子模块在快照中为空目录

> PCL2 的部分机制逻辑写在 WPF 页面代码后置（`Page*.xaml.vb`）里，完全排除页面就无法覆盖安装编排、实例管理、Mod 管理，因此 `05-桌面框架与页面逻辑` 里保留了少量**含机制逻辑**的页面文件，它们不是前端样式代码。

机制取舍与改造建议见 [机制借鉴与优化建议.md](机制借鉴与优化建议.md)：哪些直接照搬、哪些有缺陷、我们怎么做得更好，以及当前项目的踩坑清单与从零开发路线。

---

## 目录与来源对照

### 00-工程配置

| 文件 | 原路径 | 用途 |
|---|---|---|
| `Plain Craft Launcher 2.sln` | 根目录 | 解决方案结构，可看项目划分 |
| `Plain Craft Launcher 2.vbproj` | `Plain Craft Launcher 2/` | 主工程文件，含全部源文件清单 |
| `PCLCS.csproj` | `PCLCS/` | C# 辅助模块工程 |

### 01-网络与下载

| 文件 | 原路径 | 关键机制 |
|---|---|---|
| `ModNet.vb` | `Modules/Base/` | `NetTask` / `NetThread` / `NetFile` / `NetSource`：请求封装、重试、多源回退、线程上限、限速、分段下载、断点续传、进度与校验 |
| `ModDownload.vb` | `Modules/Minecraft/` | 版本清单与加载器版本清单：Mojang / BMCLAPI 双源、Forge / NeoForge / Fabric / OptiFine / LiteLoader 列表、`DlClientFix` 安装计划 |
| `ModValidate.vb` | `Modules/Base/` | 文件 SHA-1 / 大小校验与缺失检测 |
| `ModSecret.vb` | `Modules/` | Microsoft OAuth ClientId、CurseForge API Key、请求签名头（全部从环境变量读取） |

### 02-任务调度与安装器

| 文件 | 原路径 | 关键机制 |
|---|---|---|
| `ModLoader.vb` | `Modules/Base/` | `LoaderBase` / `LoaderTask` / `LoaderCombo` / `LoaderDownload`：PCL 的任务与加载器框架，所有安装/下载/搜索都跑在它上面；含任务栏进度聚合 |
| `ModDownloadLib.vb` | `Pages/PageDownload/` | OptiFine / LiteLoader / Forge / NeoForge 的安装器执行、JSON 合并、tweakClass 注入、客户端下载组合 |
| `ModModpack.vb` | `Modules/Minecraft/` | 整合包格式识别（CurseForge / Modrinth / MMC / HMCL 等）、manifest 解析、override 处理、加载器安装编排 |

### 03-启动与版本

| 文件 | 原路径 | 关键机制 |
|---|---|---|
| `ModLaunch.vb` | `Modules/Minecraft/` | 启动参数构建、占位符替换、JVM/游戏参数、微软/外置/离线登录加载器 |
| `ModMinecraft.vb` | `Modules/Minecraft/` | 版本 JSON 继承与合并、libraries 规则、natives 提取、资源索引、实例路径 |
| `ModJava.vb` | `Modules/Minecraft/` | 按游戏版本计算 Java 版本范围、Java 选择逻辑 |
| `ModWatcher.vb` | `Modules/Minecraft/` | 游戏进程启动、日志读取、窗口检测、退出/崩溃判定 |
| `ModCrash.vb` | `Modules/Minecraft/` | 崩溃日志解析与原因归类（Java、内存、Mixin、Mod 冲突等） |
| `Java.cs` | `PCLCS/` | Java 自动扫描（目录、环境变量、去重、排序） |
| `LaunchUtils.cs` | `PCLCS/Launch/` | 启动相关的 C# 辅助 |

### 04-资源与本地 Mod

| 文件 | 原路径 | 关键机制 |
|---|---|---|
| `ResourceSearcher.vb` | `Modules/Resource/` | Modrinth / CurseForge 搜索请求构造、分类/加载器/版本/分页 |
| `ResourceProject.vb` | `Modules/Resource/` | 项目详情、版本列表、文件信息、依赖关系 |
| `ResourceVersion.vb` | `Modules/Resource/` | 版本/文件元数据、下载地址与依赖解析 |
| `LocalResourceLoaders.vb` | `Modules/Resource/` | 本地 Mod 扫描、联网匹配 Modrinth `version_files`、CurseForge fingerprint 更新检查 |
| `LocalResourceFile.vb` | `Modules/Resource/` | 本地资源文件模型与启用状态（`.jar/.zip/.litemod` vs `.disabled/.old`） |
| `Resource.cs` | `PCLCS/Resource/` | CurseForge `modLoaderType` 与内部加载器枚举映射 |
| `WikiEntry.cs` | `PCLCS/Resource/` | 资源 Wiki 条目模型 |

### 05-桌面框架与页面逻辑

| 文件 | 原路径 | 关键机制 |
|---|---|---|
| `FormMain.xaml.vb` | `FormMain.xaml.vb` | 主窗口生命周期、主题/设置恢复、动画、DPI 记录、缩放绑定 |
| `MyResizer.vb` | `Controls/` | 无边框窗口八方向缩放；拦截 `WM_GETMINMAXINFO` 处理多显示器与工作区 |
| `Application.xaml.vb` | `Application.xaml.vb` | 启动流程、日志、单实例互斥、权限检查、异常兜底 |
| `Settings.vb` | `Pages/PageSetup/` | 全局设置模型、默认值、存储来源、变更通知 |
| `SettingService.vb` | `Pages/PageSetup/` | 设置项与控件绑定、批量刷新 / 重置 / 保存 |
| `PageSetupSystem.xaml.vb` | `Pages/PageSetup/` | 系统设置：下载线程数、限速、更新通道、调试模式 |
| `PageSetupLaunch.xaml.vb` | `Pages/PageSetup/` | 启动设置：内存分配、Java 列表刷新、皮肤 |
| `PageDownloadInstall.xaml.vb` | `Pages/PageDownload/` | 「选版本 → 选加载器 → 下载 → 安装」多步骤编排 |
| `PageDownloadCompDetail.xaml.vb` | `Pages/PageDownload/Resource/` | 资源详情与下载触发逻辑 |
| `PageInstanceMod.xaml.vb` | `Pages/PageInstance/` | 实例 Mod 列表、启用/禁用、批量操作 |
| `PageInstanceSetup.xaml.vb` | `Pages/PageInstance/` | 实例设置与版本隔离 |
| `PageInstanceExport.xaml.vb` | `Pages/PageInstance/` | 实例导出为整合包 |
| `PageInstanceOverall.xaml.vb` | `Pages/PageInstance/` | 实例总览与路径信息 |
| `MyLocalModItem.xaml.vb` | `Pages/PageInstance/` | 单个本地 Mod 的详情、更新与开关 |
| `PageLaunchLeft.xaml.vb` | `Pages/PageLaunch/` | 启动页版本/账户选择与启动触发 |
| `PageSelectLeft.xaml.vb` | `Pages/` | 实例文件夹管理：添加、创建、重命名、删除、刷新 |
| `PageSelectRight.xaml.vb` | `Pages/` | 文件夹内实例列表、启动、删除实例、空状态下载入口 |
| `PageSpeedLeft.xaml.vb` | `Pages/` | 下载进度页：速度、线程数、剩余文件、任务列表、错误详情 |

### 06-基础工具

| 文件 | 原路径 | 关键机制 |
|---|---|---|
| `ModBase.vb` | `Modules/Base/` | 线程/UI 调度（`RunInNewThread`、`RunInUi`）、INI 读写、进程启动、权限检查、模糊搜索 |
| `ModEvent.vb` | `Modules/` | 事件与日志/统计上报机制 |
| `PclLogger.vb` | `Modules/Base/` | 日志落盘与格式化 |
| `DragHelper.vb` | `Modules/ThirdParty/` | Win32 文件拖入窗口（`WM_DROPFILES`），导入 Mod / 整合包 |
| `Configs.cs` | `PCLCS/` | Java 配置项持久化与迁移（`ConfigEntry` / `DynamicConfigEntry`） |
| `Constants.cs` | `PCLCS/` | 资源类型与来源平台枚举（`ResourceTypes` 等） |

---

## 未复制文件清单（73 个，按目录分组）

> 核对基准：`PCL-main` 下共 118 个 `.vb` / `.cs`。本目录复制其中 45 个，另加 3 个工程文件（`.sln` / `.vbproj` / `.csproj`）。
> 原件仍完整保留在 `PCL源码/PCL-main`，需要时可随时补取。

| 分组 | 数量 | 文件 | 未复制原因 |
|---|---|---|---|
| `Controls/` | 33 | `MyButton`、`MyCard`、`MyCheckBox`、`MyComboBox`、`MyListItem`、`MyScrollViewer`、`MyTextBox`、`MyPageRight`、`MyPageLeft`、`MyMsg*`、`MyVirtualizingElement` 等 | WPF 控件、动画与页面生命周期框架，属于前端实现 |
| `My Project/` | 4 | `AssemblyInfo`、`Resources.Designer`、`Settings.Designer`、`MyWpfExtension` | 自动生成、程序集信息与 WPF 扩展 |
| 根目录 | 1 | `GlobalSuppressions.vb` | 编译器警告抑制，无逻辑 |
| `Modules/` | 5 | `ModMain`、`ModAnimation`、`MyBitmap`、`ModMusic`、`ModDevelop` | 消息框/提示、动画、图片、音乐；`ModDevelop` 为空文件 |
| `Pages/PageDownload/` | 8 | `PageDownloadLeft`、`MyResourceItem`、`PageResource`、`PageDownload{Mod,Pack,Shader,DataPack,ResourcePack}` | 资源搜索的 UI 状态与分页；请求构造已在 `ResourceSearcher.vb`，后五个是空壳 partial |
| `Pages/PageInstance/` | 2 | `PageInstanceLeft`、`PageInstanceModDisabled` | 导航与空状态按钮 |
| `Pages/PageLaunch/` | 10 | `PageLogin*`、`MyMsgLogin`、`MySkin`、`PageLaunchRight` | 登录核心加载器已在 `ModLaunch.vb`；其余是登录对话框、皮肤、自定义首页内容 |
| `Pages/PageLink/` | 1 | `PageLinkMain` | EasyTier 联机模块，与启动/下载机制无关 |
| `Pages/PageOther/` | 5 | `PageOther{About,Help,HelpDetail,Left,Test}` | 帮助、关于、测试内容页；`PageOtherTest` 在开源版是占位实现 |
| `Pages/PageSetup/` | 3 | `PageSetupLeft`、`PageSetupLink`、`PageSetupUI` | 导航与背景图/Logo/音乐等外观设置 |
| `Pages/` | 1 | `PageSpeedRight.xaml.vb` | 124 字节的空 partial |

---

## 机制速览（读代码前先看这里）

### 下载引擎

- 核心抽象在 `ModNet.vb`：`NetTask` 表示一个下载任务，内部可拆成多个分段任务；`NetFile` 表示一个待下载文件，持有多个 `NetSource`（官方源 + 镜像源），某个源失败后切换到下一个。
- 并发由 `NetTaskThreadLimit` 控制（对应启动器设置里的下载线程数），线程计数 `NetTaskThreadCount` 用于调度；`NetTaskSpeedLimitLow/High` 控制限速。
- 重试：普通请求最多 3 次（`NetRequestByClientRetry`），429/5xx 有额外等待；文件下载小文件 2 次、大文件走加载器队列重试。
- 校验：`ModValidate.vb` 统一做 SHA-1 与文件大小校验，失败会重新入队。
- 进度：`NetFile.Progress` 聚合到 `LoaderBase`，再汇总到任务栏进度。

### 任务调度

- `ModLoader.vb` 是 PCL 的「加载器框架」：`LoaderBase`（状态机）→ `LoaderTask`（单任务）→ `LoaderCombo`（组合任务）→ `LoaderDownload`（批量下载队列）。
- 所有安装、搜索、扫描、下载都是这条链上的节点，天然支持取消、重试、进度与失败汇总。

### 版本与启动

- `ModMinecraft.vb` 处理 `inheritsFrom` 继承链、libraries 的 rules、natives 解压与资源索引。
- `ModLaunch.vb` 负责最终命令行：`arguments.jvm` / `arguments.game`、规则过滤、占位符替换（`classpath`、`natives_directory`、`library_directory`、`auth_player_name` 等）。
- `ModJava.vb` 根据游戏版本与加载器约束选择 Java；`Java.cs` 负责扫描本机 Java。

### 加载器与整合包

- 加载器列表与下载地址在 `ModDownload.vb`，安装动作在 `ModDownloadLib.vb`：Forge / NeoForge 走官方安装器 + JSON 注入，OptiFine / LiteLoader 走各自的安装器与 tweakClass 注入。
- 整合包在 `ModModpack.vb`：先识别格式，再解析 manifest / modrinth.index.json，最后把「游戏安装 + 加载器安装 + 文件下载」串成 `LoaderCombo`。

### 资源与本地 Mod

- 搜索与详情在 `ResourceSearcher.vb` / `ResourceProject.vb` / `ResourceVersion.vb`，依赖解析也在这里。
- 本地 Mod 扫描在 `LocalResourceLoaders.vb`，更新检查依赖 Modrinth `version_files` 与 CurseForge fingerprint。

### 桌面框架

- PCL2 使用 `WindowStyle=None` + `AllowsTransparency=True` 的无边框窗口，八方向缩放由 `MyResizer.vb` 自己实现，并处理多显示器/DPI 的最大化尺寸。
- 单实例、启动流程、日志和异常兜底在 `Application.xaml.vb`。

---

## 建议阅读顺序

1. `02-任务调度与安装器/ModLoader.vb`（先懂 LoaderBase / Task / Combo）
2. `01-网络与下载/ModNet.vb`（下载引擎）
3. `01-网络与下载/ModDownload.vb` + `ModValidate.vb`（安装计划与校验）
4. `03-启动与版本/ModMinecraft.vb` → `ModLaunch.vb` → `ModJava.vb`
5. `02-任务调度与安装器/ModDownloadLib.vb`（Forge/NeoForge/OptiFine 安装）
6. `02-任务调度与安装器/ModModpack.vb` + `04-资源与本地Mod/*`
7. `05-桌面框架与页面逻辑/*`（窗口框架与安装编排）
```

#### `机制借鉴与优化建议.md`

````markdown
# PCL2 机制借鉴与优化建议

> 分析对象：`PCL参考代码/` 下的 48 个机制文件（PCL2 源码原样副本）。
> 目的：区分「PCL 已经做到最好、我们直接照搬」与「PCL 有缺陷、我们可以做得更好」，作为本项目（Tauri 2 + Rust + Vue）的改造依据。
> 评估维度：正确性、速度、鲁棒性、可维护性、用户体验。

---

## 一、结论速览

| 机制 | PCL 成熟度 | 建议 |
|---|---|---|
| 每文件多源 + 逐源失败回退 | 高 | 直接借鉴 |
| 源优先级 + 启动时测官方延迟 | 高 | 直接借鉴 |
| 文件 SHA-1 / 大小校验、命中即跳过 | 高 | 直接借鉴 |
| 资源索引后台缓存（旧索引先用） | 高 | 直接借鉴，补原子性 |
| Forge / NeoForge 安装器 + JSON 注入 | 高 | 借鉴思路，补事务与进度 |
| 启动参数占位符与 rules 过滤 | 高 | 直接借鉴 |
| Java 版本范围计算 | 高 | 借鉴，补运行时校验 |
| 本地 Mod 哈希缓存（mtime + size 作键） | 高 | 借鉴，修键与清理 |
| 实例列表校验码缓存 | 高 | 借鉴，补 schema 与原子写 |
| 项目详情内存缓存 + 60 秒任务冷却 | 中 | 借鉴，升级为短 TTL 磁盘缓存 |
| LoaderBase / Task / Combo 任务框架思想 | 高 | 借鉴分层，弃用线程模型 |
| 崩溃日志规则分类 | 中 | 借鉴分类，规则外置 |
| CF 429 处理 | 中 | 优化：Retry-After + 抖动 + 熔断 |
| CF / Modrinth 镜像与 CDN 归一化 | 高 | 借鉴，但镜像不做默认 |
| Mod 搜索结果缓存 | 低 | 我们自己做短 TTL 缓存 |
| 设置持久化 | 中 | 改为 JSON + schema 迁移 + 系统密钥库 |
| WPF 无边框窗口 | 中 | 不照搬，继续 Tauri CSD |

---

## 二、可以直接借鉴（PCL 已经做到位）

### 2.1 每文件多源，逐源失败

`NetFile` 持有多个 `NetSource`，某个源失败后切换到下一个，失败次数与异常都记录下来。
见 [ModNet.vb](./01-网络与下载/ModNet.vb#L356-L637)。

**为什么值得照搬**：单源策略在换镜像、CDN 抖动、临时 5xx 时会把整个安装拖死；多源是启动器下载稳定性的底线。本项目已经按文件多源，继续保持。

### 2.2 源优先级与自动测速

PCL 用三个设置控制源顺序，并在启动时测一次 Mojang 清单耗时，小于 4 秒才优先官方源：
[ModDownload.vb](./01-网络与下载/ModDownload.vb#L232-L244)。

**值得照搬的点**：不要写死「官方优先」；用一次轻量探测决定优先级，既照顾国内网络，也不牺牲海外网络。

### 2.3 校验与跳过

`ModValidate.vb` 统一做 SHA-1 与文件大小校验，命中即视为完成。
**值得照搬的点**：校验失败不要静默重下，要进失败清单并带原因；已经正确的文件绝不重复下载。

### 2.4 资源索引后台缓存

先读旧索引保证能启动，后台下载新索引，下次生效：
[ModDownload.vb](./01-网络与下载/ModDownload.vb#L83-L134)。

**值得照搬的点**：索引更新不应该阻塞「现在就能玩」；把「更新」和「可用」解耦。

### 2.5 加载器安装器 + JSON 注入

Forge / NeoForge 不自己复刻 binpatch 处理器，而是运行安装器再合并 JSON：
[ModDownloadLib.vb](./02-任务调度与安装器/ModDownloadLib.vb#L892-L1140)。
OptiFine / LiteLoader 也走各自安装方式，而不是硬拼 classpath。

**值得照搬的点**：加载器安装永远优先用官方安装器，自己复刻处理器只会引入难查的启动 bug。本项目已经采用 installer 方案，方向正确。

### 2.6 启动参数与 Java 版本范围

`arguments.jvm` / `arguments.game` 的 rules 过滤、`inheritsFrom` 继承、占位符替换、按版本计算 Java 范围，都值得直接参考：
[ModLaunch.vb](./03-启动与版本/ModLaunch.vb#L1201-L1594)、
[ModJava.vb](./03-启动与版本/ModJava.vb#L124-L185)。

### 2.7 本地 Mod 哈希缓存键

用「文件名 + 修改时间 + 大小 + 平台标记」做键，缓存 CF MurmurHash2 与 Modrinth SHA-1：
[LocalResourceFile.vb](./04-资源与本地Mod/LocalResourceFile.vb#L188-L260)。

**值得照搬的点**：不要每次都重算几百 MB jar 的哈希；文件一变键就变，天然失效。

### 2.8 实例列表校验码

用「文件夹名列表 + 缓存 schema 版本」的哈希判断是否需要重扫：
[ModMinecraft.vb](./03-启动与版本/ModMinecraft.vb#L1286-L1304)。

**值得照搬的点**：启动页不应该每次全量扫描实例目录。

### 2.9 任务框架的分层思想

`LoaderBase`（状态机）→ `LoaderTask`（单任务）→ `LoaderCombo`（组合）→ `LoaderDownload`（批量队列）：
[ModLoader.vb](./02-任务调度与安装器/ModLoader.vb#L7-L416)。

**值得照搬的点**：安装流程要能分解成可取消、可重试、可汇总进度的节点；不要写一条几百行的大函数。

---

## 三、PCL 的缺陷与我们的更优实现

### 3.1 下载重试与限流

**PCL 做法**：普通请求最多 3 次；429 固定 `Thread.Sleep(10000)`；403/404 直接失败；重试会阻塞线程（[ModNet.vb](./01-网络与下载/ModNet.vb#L44-L86)）。

**问题**：
- 忽略 `Retry-After`，服务端要求等 30 秒时仍只等 10 秒，容易继续吃 429；
- 固定等待没有抖动，多文件并发时同一秒一起重试，形成新的峰值；
- 阻塞线程重试，占用下载线程池；
- 没有「按源熔断」，一个持续失败的镜像会被反复尝试。

**我们的更好方案**：
1. 优先读 `Retry-After`（秒或 HTTP 日期），没有才用指数退避 `0.5s / 1s / 2s + 10% 抖动`。
2. 403/404 不重试，5xx 与网络错误重试；429 单独归类。
3. 每个源维护连续失败计数，连续 3 次失败后熔断 60 秒，期间直接走备用源；成功一次即恢复。
4. 重试用异步定时器，不占并发槽位。
5. 记录每个源的失败率，设置页可以展示「当前源健康度」。

### 3.2 并发模型

**PCL 做法**：`NetTaskThreadLimit` 全局线程数，`NetTaskThreadCount` 计数。

**问题**：全局一个数字，无法区分「元数据请求」和「大文件下载」；也无法防止所有线程同时压向同一个镜像。

**我们的更好方案**：
- 全局并发（默认 12）+ 单 host 并发（默认 6）+ 元数据请求独立小并发（2）。
- 自适应：连续成功时逐步 +1，直到上限；错误率升高时减半。
- 大文件与资源对象分队列，资源对象优先，客户端 jar 最高优先级。

### 3.3 缓存

**PCL 做法**：
- HTTP 响应缓存用 CacheCow，落在 `Cache/Http/`（[ModNet.vb](./01-网络与下载/ModNet.vb#L223-L231)）；
- `LocalMod.json` 缓存本地 Mod 联网信息，6 小时 TTL（[LocalResourceLoaders.vb](./04-资源与本地Mod/LocalResourceLoaders.vb#L105-L120)）；
- `ModHash.ini` 缓存哈希；
- 项目详情只在内存缓存。

**问题**：
- HTTP 缓存没有 TTL 与容量上限，长期使用会膨胀；API 响应被缓存后可能拿到过期数据；
- `ModHash.ini` 用 `ToLongTimeString()` 做键的一部分，受系统区域设置影响，且只精确到秒；
- `LocalMod.json` 是全局单文件，Mod 删除后不会裁剪，条目会越积越多；
- 搜索结果完全不落盘，重启后必然重新请求。

**我们的更好方案**：
1. 元数据 HTTP 缓存：只缓存 GET 200，存 ETag / Last-Modified，TTL 10 分钟，容量上限（如 50MB），LRU 淘汰；带 `Authorization` / `x-api-key` 的请求不进缓存。
2. 搜索结果缓存：键 = `平台|类型|加载器|游戏版本|关键词|排序|页码` 的哈希，TTL 10 分钟；只存列表摘要（id、slug、标题、图标、下载量、更新时间、total），手动刷新可绕过。
3. 项目详情缓存：TTL 6 小时，按 `platform:projectId` 存；版本列表单独缓存 10 分钟。
4. 哈希缓存：键 = `路径|size|mtime(UTC ticks)|算法`，用 SQLite 或带 schema 的 JSON，定期裁剪不存在文件。
5. 所有缓存写临时文件再原子 rename，读取解析失败自动丢弃重下。

### 3.4 加载器安装的事务性

**PCL 做法**：安装器直接写目标目录，失败后靠 `InstanceCache` 清空来触发重扫（[ModDownloadLib.vb](./02-任务调度与安装器/ModDownloadLib.vb#L1802)）。

**问题**：
- 安装器中途失败会留下不完整的版本目录；
- 安装器本身没有结构化进度，用户只能看「安装中」；
- 取消或关闭启动器可能留下锁文件/半成品。

**我们的更好方案**：
1. 安装前备份 `versions/<loader-id>/`，安装完成后校验 JSON、主 jar、完成标记，全部通过才提交；失败删除该版本目录并恢复备份。
2. 安装器运行阶段用「不确定进度 + 阶段文案」：下载安装器 → 预置依赖 → 运行安装器 → 补全资源，每阶段独立可读。
3. 取消时杀死子进程（`kill_on_drop`）并清理 staging；库文件是内容寻址的，可保留，不重复下载。
4. 安装前做磁盘空间预检：`所需大小 * 1.1` 与可用空间比较，不足直接提示。
5. 启动器退出时清理 `.part` 与 staging 残留。

### 3.5 启动与 Java

**PCL 做法**：按版本与加载器计算 Java 范围，从目录/环境变量扫描 Java（[ModJava.vb](./03-启动与版本/ModJava.vb#L124-L185)、[Java.cs](./03-启动与版本/Java.cs#L171-L248)）。

**问题**：
- 依赖路径扫描，可能选到错误架构或错误 vendor 的 Java；
- 没有在启动前实际校验 Java 的版本、架构与可用内存；
- 参数拼接依赖自定义转义，路径带空格/特殊字符时容易出错。

**我们的更好方案**：
1. 启动前跑一次 `java -XshowSettings:properties -version`，解析 `java.version`、`os.arch`、`java.vendor`，不符合直接给出可操作提示。
2. 每个实例固定 Java 主版本（`instance.json` 里存 `javaMajor` 或具体路径），避免升级游戏后 Java 被静默换掉。
3. 启动命令使用参数数组（Rust `Command`），不拼字符串，彻底规避引号与转义问题。
4. 内存预检：可用物理内存不足时给出建议值，而不是等游戏 OOM。
5. 崩溃报告附带：Java 信息、内存、加载器版本、Mod 列表快照、日志尾部。

### 3.6 Mod 搜索与 CurseForge

**PCL 做法**：双平台并发搜索、facets 过滤、中文关键词映射、批量接口、镜像代理、CF CDN 多域名归一化（[ResourceSearcher.vb](./04-资源与本地Mod/ResourceSearcher.vb#L160-L480)、[ResourceVersion.vb](./04-资源与本地Mod/ResourceVersion.vb#L413-L420)、[ModDownload.vb](./01-网络与下载/ModDownload.vb#L1178-L1210)）。

**问题**：
- 搜索结果不落盘，重复搜索重复请求；
- CF 请求没有统一调度，多个页面/操作可能同时打 API，放大 429；
- 429 只等 10 秒，忽略 `Retry-After`；
- 镜像 `mod.mcimirror.top` 是硬编码第三方，默认设置偏保守但缺少健康检查与一键关闭；
- CF 的 10000 上限没有解释，也不会自动改用 Modrinth 补充。

**我们的更好方案**：
1. 全局 CF 请求调度器：并发 2、令牌桶 30 次/分钟（可配置）；用户主动搜索优先于后台刷新。
2. 读 `Retry-After`；429 时熔断该端点 30 秒，期间只走镜像。
3. 搜索缓存（见 3.3）；重复搜索直接秒出，手动刷新按钮绕过。
4. CF 上限提示：当 `totalCount >= 10000` 时提示「CF 只返回前 1 万条，建议缩小筛选或改用 Modrinth」；自动合并 Modrinth 结果时标注来源。
5. 镜像不做默认开启：设置里提供「官方 / 自动 / 镜像」三档，自动模式先探测官方延迟，失败再回退。
6. 批量接口分块（每批 50 个 id），避免单次请求过大再次触发限流。

### 3.7 本地 Mod 与实例元数据

**PCL 做法**：实例缓存与 Mod 信息写在 `PCL.ini`，Mod 在线信息写 `Cache/LocalMod.json`。

**问题**：
- 元数据分散在 INI，缺少 schema 与迁移，损坏时只能整段清空；
- 全局 `LocalMod.json` 不按实例隔离，同一 Mod 在不同实例重复存；
- 缓存键包含本地化的时间字符串，换系统语言会失效。

**我们的更好方案**：
1. 全局 `instances.json` + 每实例 `instance.json`，都带 `schemaVersion`，原子写。
   > **当前状态**：每实例 `instance.json`（含 schemaVersion 迁移链）已实现；
   > 全局索引 `instances.json` 未实现——属于 200+ 实例时的性能优化，在路线图中。
2. 本地 Mod 缓存按实例拆分（`instances/<id>/mods-cache.json`），键用 UTC 时间戳 + size + 路径。
3. 删除 Mod 时按增量清理缓存条目；启动时做一次孤儿条目清理。
4. 缓存容量上限与 LRU，避免长期使用后几十 MB。

### 3.8 桌面窗口框架

**PCL 做法**：WPF `WindowStyle=None` + `AllowsTransparency=True`，自己实现八方向缩放与 `WM_GETMINMAXINFO`（[MyResizer.vb](./05-桌面框架与页面逻辑/MyResizer.vb#L89-L311)）。

**问题**：透明窗口会失去部分硬件加速与原生阴影；自定义缩放逻辑复杂且平台相关；Snap Layouts 不可用。

**我们的更好方案**：
- 保持 Tauri `decorations: false` + `shadow: true` + 不透明底色（我们已是这样），只在确实需要 Mica 时才开透明。
- 缩放热区继续用 DOM 热区；后续可以评估用原生 `WM_NCHITTEST` 做更精确的边缘命中，但优先级不高。
- 不为 Snap Layouts 牺牲稳定性；在文档里明确这是 CSD 的取舍。

### 3.9 设置与密钥

**PCL 做法**：`Settings` 同时写注册表与 INI，`SettingService` 与 UI 控件双向绑定（[Settings.vb](./05-桌面框架与页面逻辑/Settings.vb#L5-L220)、[SettingService.vb](./05-桌面框架与页面逻辑/SettingService.vb)）。

**问题**：注册表 + INI 两套存储、全局可变、难迁移；密钥类字段（API Key / Token）在设置文件里，存在泄露风险；UI 与设置耦合，难测试。

**我们的更好方案**：
1. 单一 `settings.json`，`schemaVersion` + 迁移函数；原子写；默认值集中在 Rust 一处。
2. 实例级设置放 `instance.json`，不污染全局。
3. API Key、OAuth Token 放系统密钥库（Windows Credential Manager / `keyring`），设置文件只存「是否已配置」。
4. 变更用 watch channel 广播，不依赖 UI 绑定。

### 3.10 崩溃分析

**PCL 做法**：`ModCrash.vb` 用大量模式匹配归类崩溃（Java、内存、Mixin、Mod 冲突等）。

**问题**：规则硬编码在 77KB 的模块里，升级困难；匹配容易误报；缺少上下文快照。

**我们的更好方案**：
1. 规则外置为 JSON/TOML：`{id, pattern, section, severity, suggestion}`，带版本号，可热更新。
2. 匹配时结合上下文：Java 版本、内存、加载器、最近安装的 Mod、日志尾部 200 行，输出置信度。
3. 「复制报告」默认脱敏路径与账户名，方便用户发到社区求助。

---

## 四、落地优先级

| 优先级 | 改动 | 目标模块 | 风险 |
|---|---|---|---|
| P0 | 搜索缓存（列表 10 分钟 / 详情 6 小时） | `resource` 模块 + 前端搜索页 | 低 |
| P0 | CF 请求调度器 + 令牌桶 + `Retry-After` | `resource` 模块 | 低 |
| P0 | 加载器安装备份/校验/失败清理 | `loader/installer.rs` | 中 |
| P0 | 实例 `instance.json` + schema + 原子写 | `version/installed.rs`、启动页 | 中 |
| P1 | 哈希缓存改为 UTC 键 + 容量上限 + 清理 | 本地 Mod 扫描 | 低 |
| P1 | 并行度分层 + 单 host 熔断 | `download/` | 中 |
| P1 | Java 启动前校验 + 实例固定 Java | `launch/`、`java/` | 低 |
| P1 | 磁盘空间预检 | `download/` | 低 |
| P2 | 元数据 HTTP 缓存（ETag + TTL + 容量） | `download/` | 中 |
| P2 | 资源索引后台更新 | `version/plan.rs` | 中 |
| P2 | 崩溃规则外置 | 新模块 | 中 |

---

## 五、明确不照搬的部分

- WPF 的线程模型（`Thread.Sleep` 轮询、`Thread.Interrupt` 取消）——改用 Rust 异步 + `CancellationToken`。
- 全局可变模块与 UI 代码后置——机制逻辑与界面分离。
- CacheCow 对所有 GET 的粗放缓存——改为显式 TTL/容量/键控制。
- 把第三方镜像作为默认源——镜像只做兜底，可关闭。
- 注册表设置、INI 缓存键里的本地化时间——统一 JSON + UTC。
- 正式版 PCL 的私有 CDN 签名——开源代码里已删，无法参考。
- EasyTier 联机、音乐、皮肤、自定义首页等非核心功能——与本项目目标无关。

---

## 六、当前项目的坑与从零重开路线

这一节不是追责，而是把这次开发中真实踩到的坑固化成规则，避免新项目重复。

### 6.1 为什么现在会觉得乱

| 现象 | 根因 |
|---|---|
| 功能是「按页面堆」出来的 | 先做 DownloadsView、再补下载、再补加载器，业务逻辑落在页面和命令层，没有独立用例层 |
| 状态有两套 | Pinia 管页面数据，`useDownload` 用模块级单例管运行时任务；两者互不知情 |
| 版本身份靠字符串 | `fabric-loader-…`、`1.21.1-forge-…`、`neoforge-…` 都从 id 推导，加载器/原版版本没有显式模型 |
| 安装流程没有阶段模型 | 一个命令里串多批下载 + 安装器 + 补全，进度只能靠 `reset_progress` 和「完成但仍在跑 = 安装中」的启发式 |
| 缓存与请求调度缺位 | 只缓存了版本清单，搜索、项目详情、Mod 信息没有缓存；CF 请求没有统一限流 |
| 契约靠人工同步 | bindings 由运行程序生成，权限与 CSP 手写，命令参数一改前端就要跟着改 |
| 占位数据和真实数据混在一起 | `stores/download.ts` 里整合包/资源包/光影仍是示例数组，容易误当成真实功能 |
| 验证靠手工 | 关键网络测试标记为 `ignored`，窗口、安装、启动都靠截图和点击确认 |

### 6.2 必须避开的坑

| 坑 | 症状 | 新项目做法 |
|---|---|---|
| 多阶段安装没有统一 Job | 进度在阶段之间跳回 0，标题栏不知道怎么显示 | 所有长任务统一成 `Job + Phase`，快照里带阶段名与总进度 |
| 用版本 id 当身份 | 扫描、启动、删除各自假设 `versions/<id>/<id>.jar`，加载器版本全部要打补丁 | 实例落盘 `instance.json`，显式存游戏版本、加载器类型与版本、Java、路径 |
| 加载器特例散落各处 | Forge 补丁 jar 在 libraries、NeoForge 缺 `:client` 条目、安装器要 `launcher_profiles.json`，这些知识散在 merge / 扫描 / 启动 | 定义 `LoaderAdapter`，每个加载器只暴露「安装、解析主 jar、参数注入」三个接口 |
| 设置全量覆盖保存 | 新增 `downloadConcurrency` 时，前端仍只传旧字段，保存后新字段被重置 | `SettingsPatch` 只传改动字段，后端 merge；schemaVersion + 迁移 |
| 下载状态没有统一 Job 模型 | 运行时状态放在模块级单例；版本安装和 Mod 下载共用同一状态，但阶段、失败、任务类型无法区分，标题栏只能靠「完成但仍在跑」的启发式 | 一个 `jobs` store + `Job/Phase` 模型，所有下载任务统一进队列 |
| 搜索结果不缓存 | 每次切页/重开都重新打 API，CF 还容易吃 429 | 列表短 TTL 缓存 + 项目详情长 TTL + 请求调度器 |
| 共享 `minecraft/mods` | 所有实例共用一个 Mod 目录，换实例会互相污染 | 每个实例独立 `mods/`，共享内容用硬链接或复制策略显式选择 |
| Java 参数靠试错 | `${library_directory}`、`${classpath_separator}` 到启动才报错 | 启动前做参数完整性校验，缺占位符直接给出可读错误 |
| 占位数据混入正式 store | 界面出现「示例整合包/光影」，但点下载并不存在 | 生产 store 只放真实数据；没有接口就显示空状态，不放样例 |
| bindings / 权限 / CSP 手工维护 | 加命令忘记注册、加图片域名忘记改 CSP、拖动权限漏了静默失败 | 生成步骤进 CI，权限与 CSP 有清单和测试 |
| 错误原文直出 | 用户看到 `error sending request for url (...)` | 领域错误映射成用户文案，技术细节只进日志 |
| 只测 happy path | 断网、429、磁盘不足、取消、半成品目录都没覆盖 | 本地 mock HTTP + fixture JSON，断网/超时/429/取消都进测试 |
| 开发环境不稳定 | WebView2 profile 报错、Vite 没起就白屏、dev 与 dist 混用 | 固定 dev 启动脚本与临时 profile，CI 只用构建产物 |
| 大文件堆逻辑 | `commands/download.rs` 变成几百行编排，`DownloadsView.vue` 兼顾搜索/分页/下载 | 用例层拆文件；页面只读 store、只发意图 |
| 缓存键用本地化时间 | 换系统语言后缓存全失效 | 键用 UTC 时间戳 + 大小 + 路径，不用 `ToLongTimeString` |

### 6.3 新项目的架构原则

**依赖方向**

```
UI (Vue) → Tauri commands → app（用例/Job） → services（网络/下载/缓存/Java/安装器） → core（模型/计划/纯函数）
```

`core` 不依赖任何上层，保证版本 JSON、加载器合并、参数构建这些纯逻辑可以脱离 Tauri 和网络单测。

**目录骨架**

```
crates/
  sereno-core/        # 领域模型、安装计划、纯函数、错误类型
  sereno-services/    # HTTP、下载引擎、缓存、Java、加载器安装器
  sereno-app/         # Job / 用例编排，可独立测试
apps/desktop/src-tauri/   # 只做 IPC 适配与状态注册
apps/desktop/src/
  stores/           # jobs / search / settings / instances
  features/         # 按功能分目录，不按页面类型分
```

**Job 模型**

```rust
pub struct Job {
    pub id: String,
    pub kind: JobKind,          // InstallVersion / InstallLoader / DownloadMod ...
    pub phases: Vec<Phase>,
    pub status: JobStatus,      // Preparing / Running / Succeeded / Failed / Cancelled
    pub cancel: CancellationToken,
}

pub struct Phase {
    pub key: &'static str,      // download / install / verify / assets
    pub label: &'static str,
    pub weight: u32,            // 决定总进度占比
    pub status: PhaseStatus,
}
```

前端只消费 `JobSnapshot { overall, currentPhase, phaseLabel, indeterminate, failures }`，标题栏和下载页共用同一份数据。

**版本身份**

```json
{
  "schemaVersion": 1,
  "gameVersion": "1.21.1",
  "loader": { "type": "neoforge", "version": "21.1.265" },
  "java": { "major": 21 },
  "paths": { "mods": "mods" }
}
```

扫描、启动、删除、Mod 安装全部读这份 manifest，不再从目录名反推。

**设置**

```rust
pub struct SettingsPatch {
    pub auto_simplified_chinese: Option<bool>,
    pub download_concurrency: Option<u8>,
}
// 只应用 Some 字段；写入前做 schemaVersion 迁移；密钥进系统密钥库
```

**缓存**

统一 `MetadataService`：键、TTL、容量、原子写、single-flight。搜索、项目详情、版本清单、Mod 信息全部走它，页面不直接发 HTTP。

### 6.4 从零开发路线

每个阶段都必须「能独立演示 + 有测试 + 更新文档」，不把 UI 全做完才验证。

#### Phase 0：工程骨架与契约

- 交付：workspace、错误类型、DTO、设置 schema + 迁移、路径与实例 manifest、日志、CI 检查脚本、bindings 生成脚本。
- 验收：`cargo fmt / clippy / test`、`pnpm typecheck / lint / test` 全绿；不依赖网络。
- 不做：任何真实下载、任何页面功能。

#### Phase 1：基础设施

- 交付：HTTP 客户端（源策略、重试、`Retry-After`、按源熔断）、下载引擎（队列、分层并发、`.part` 续传、流式 SHA-1、取消）、缓存层（TTL/原子写/容量）、Java 检测、文件布局。
- 验收：本地 mock HTTP 覆盖成功/超时/429/断网/取消；断点续传可恢复；缓存命中与过期可测。
- 不做：加载器、Mod、页面。

#### Phase 2：原版安装

- 交付：版本清单与缓存、安装计划、资源索引、安装 Job、实例 manifest、磁盘空间预检。
- 验收：冷启动安装 1.21.1 全流程；取消后重试可续；已完整时全部跳过；空间不足有可读提示。

#### Phase 3：启动

- 交付：classpath、参数与占位符、natives、Java 选择与运行时校验、进程与日志监听。
- 验收：离线启动 1.21.1；参数缺失时给出定位明确的错误；崩溃日志能落盘。

#### Phase 4：加载器

- 交付：`LoaderAdapter` 接口；Fabric（JSON 合并）先行；Forge/NeoForge（安装器 + 预取 + staging/回滚）；加载器版本列表。
- 验收：Fabric profile / Forge 安装结果 JSON 的 fixture 测试；真实安装测试可选执行；失败后目录可清理、可重装。

#### Phase 5：Mod 与资源

- 交付：Modrinth（无 Key）先行；CurseForge 后接（密钥库、请求调度、429）；搜索/详情/版本/依赖；本地 Mod 扫描与更新；每实例独立 mods。
- 验收：搜索缓存命中；429 退避与熔断；依赖安装；Mod 哈希缓存按 mtime+size 失效。

#### Phase 6：UI 整合

- 交付：`jobs` store + 标题栏状态；下载页；实例页；设置页；错误与空状态。
- 验收：运行/失败/取消/完成四态齐全；页面无业务逻辑；没有示例数据混入。
- 原则：UI 只做「最小可用外壳」验证机制，机制稳定后再做视觉细化。

#### Phase 7：硬化

- 交付：崩溃规则外置、缓存 GC、诊断报告、性能与 DPI/多屏验证、文档收尾。
- 验收：长时间安装、低磁盘、断网重连、多显示器全部过一遍。

### 6.5 每个功能的 Definition of Done

- [ ] 领域模型与计划有 fixture 单测（不联网）
- [ ] 服务层有本地 mock HTTP 测试（含失败、超时、429、取消）
- [ ] 命令 DTO 稳定，`bindings.ts` 由脚本生成，无手工改动
- [ ] store 覆盖运行 / 失败 / 取消 / 完成四态
- [ ] 错误有用户文案，技术细节只进日志
- [ ] 文档更新：架构、契约、路线
- [ ] 手工验证清单执行并记录

### 6.6 禁止清单

- 页面里写业务逻辑或直接发 HTTP。
- 模块级单例管运行时状态（统一进 store）。
- 生产 store 放示例数据。
- 用版本 id 字符串做身份。
- 全量覆盖保存设置。
- 默认把第三方镜像当主源。
- 错误原文直接展示给用户。
- 手工编辑 `bindings.ts`。
- 只写 happy path 测试。

### 6.7 给 AI Agent 的开发约定

新项目建议先写四份文件，后续每个任务都引用它们：

- `docs/architecture.md`：分层、依赖方向、目录职责、禁止事项。
- `docs/contracts.md`：命令清单、DTO、错误码、bindings 生成方式。
- `docs/roadmap.md`：Phase 列表与当前进度。
- `AGENTS.md`：测试命令、代码风格、模块边界、任务卡模板。

任务卡固定结构：

```
目标：
涉及层：core / services / app / command / store / view
契约变化：有 / 无
验收：单测 + mock HTTP + 手工检查
```

这样新项目里无论人还是 Agent，都能按「领域模型 → 服务 → Job → 命令 → store → 页面」的顺序做，不会又回到按页面堆功能。

---

## 七、相关参考文件

- 下载引擎：[ModNet.vb](./01-网络与下载/ModNet.vb)
- 原版安装计划：[ModDownload.vb](./01-网络与下载/ModDownload.vb)
- 任务框架：[ModLoader.vb](./02-任务调度与安装器/ModLoader.vb)
- 加载器安装：[ModDownloadLib.vb](./02-任务调度与安装器/ModDownloadLib.vb)
- 启动与版本：[ModLaunch.vb](./03-启动与版本/ModLaunch.vb)、[ModMinecraft.vb](./03-启动与版本/ModMinecraft.vb)
- 资源搜索：[ResourceSearcher.vb](./04-资源与本地Mod/ResourceSearcher.vb)、[ResourceVersion.vb](./04-资源与本地Mod/ResourceVersion.vb)
- 本地 Mod 缓存：[LocalResourceFile.vb](./04-资源与本地Mod/LocalResourceFile.vb)、[LocalResourceLoaders.vb](./04-资源与本地Mod/LocalResourceLoaders.vb)
- 实例管理：[PageSelectLeft.xaml.vb](./05-桌面框架与页面逻辑/PageSelectLeft.xaml.vb)、[PageSelectRight.xaml.vb](./05-桌面框架与页面逻辑/PageSelectRight.xaml.vb)
- 窗口框架：[FormMain.xaml.vb](./05-桌面框架与页面逻辑/FormMain.xaml.vb)、[MyResizer.vb](./05-桌面框架与页面逻辑/MyResizer.vb)
````

### 📁 `00-工程配置`

#### `00-工程配置/PCLCS.csproj`

```xml
<Project Sdk="Microsoft.NET.Sdk">

	<PropertyGroup>
		<TargetFramework>net48</TargetFramework>
		<OutputType>Library</OutputType>
		<AllowUnsafeBlocks>True</AllowUnsafeBlocks>
		<OutDir>bin\</OutDir>
		<AppendTargetFrameworkToOutputPath>false</AppendTargetFrameworkToOutputPath>
		<AppendRuntimeIdentifierToOutputPath>false</AppendRuntimeIdentifierToOutputPath>
		<Configurations>Debug;Release;Snapshot</Configurations>
		<Deterministic>true</Deterministic>
		<LangVersion>13.0</LangVersion>
		<Nullable>enable</Nullable>
		<ImplicitUsings>enable</ImplicitUsings>
	</PropertyGroup>

	<ItemGroup>
		<PackageReference Include="PolySharp" Version="1.15.0">
			<PrivateAssets>all</PrivateAssets>
			<IncludeAssets>runtime; build; native; contentfiles; analyzers; buildtransitive</IncludeAssets>
		</PackageReference>
	</ItemGroup>

	<ItemGroup>
		<Using Include="System.Diagnostics.CodeAnalysis" />
		<Using Include="System.Collections" />
		<Using Include="System.Collections.Generic" />
		<Using Include="System.Collections.Concurrent" />
		<Using Include="System.Text" />
		<Using Include="System.Net" />
		<Using Include="System.Runtime.CompilerServices" />
		<Using Include="System.Runtime.InteropServices" />
		<Using Include="Newtonsoft.Json" />
		<Using Include="Newtonsoft.Json.Linq" />
		<Using Include="MeloongCore" />
		<Using Include="MeloongCore.Extensions" />
		<Using Include="MeloongCore.Wpf" />
		<Using Include="MeloongCore.Wpf.Extensions" />
	</ItemGroup>

	<ItemGroup>
	  <ProjectReference Include="..\MeloongCore\Shared\MeloongCore.csproj" />
	  <ProjectReference Include="..\MeloongCore\Wpf\MeloongCore.Wpf.csproj" />
	</ItemGroup>

	<PropertyGroup Condition="'$(Configuration)|$(Platform)'=='Debug|AnyCPU'">
		<DefineConstants>$(DefineConstants);SNAPSHOT</DefineConstants>
	</PropertyGroup>

	<PropertyGroup Condition="'$(Configuration)|$(Platform)'=='Release|AnyCPU'" />

	<PropertyGroup Condition="'$(Configuration)|$(Platform)'=='Snapshot|AnyCPU'">
		<Optimize>True</Optimize>
	</PropertyGroup>

	<!-- 内嵌的资源 -->
	<ItemGroup>
		<EmbeddedResource Include="Launch\JavaWrapper.jar" LogicalName="Launch\JavaWrapper.jar" />
		<EmbeddedResource Include="Launch\LwjglUnsafeAgent.jar" LogicalName="Launch\LwjglUnsafeAgent.jar" />
		<EmbeddedResource Include="Resource\WikiEntries.txt" LogicalName="Resource\WikiEntries.txt" />
	</ItemGroup>
	<ItemGroup>
	  <Reference Include="Microsoft.VisualBasic" />
	  <Reference Include="Newtonsoft.Json">
	    <HintPath>..\Plain Craft Launcher 2\Resources\Newtonsoft.Json.dll</HintPath>
	  </Reference>
	</ItemGroup>

</Project>
```

#### `00-工程配置/Plain Craft Launcher 2.sln`

```text
﻿
Microsoft Visual Studio Solution File, Format Version 12.00
# Visual Studio Version 17
VisualStudioVersion = 17.2.32616.157
MinimumVisualStudioVersion = 10.0.40219.1
Project("{F184B08F-C81C-45F6-A57F-5ABD9991F28F}") = "Plain Craft Launcher 2", "Plain Craft Launcher 2\Plain Craft Launcher 2.vbproj", "{4499F2D2-18EF-4238-B1F0-7BD547AFF925}"
EndProject
Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "PCLCS", "PCLCS\PCLCS.csproj", "{32DE4AAF-F4D6-42C5-9604-19236DE96C69}"
EndProject
Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "MeloongCore", "MeloongCore\Shared\MeloongCore.csproj", "{D22BBF3C-2177-0208-4E29-8466AA41AC6F}"
EndProject
Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "MeloongCore.Wpf", "MeloongCore\Wpf\MeloongCore.Wpf.csproj", "{2DFFE2A0-1101-816B-D240-F644B2D74A68}"
EndProject
Global
	GlobalSection(SolutionConfigurationPlatforms) = preSolution
		Debug|Any CPU = Debug|Any CPU
		Release|Any CPU = Release|Any CPU
		Snapshot|Any CPU = Snapshot|Any CPU
	EndGlobalSection
	GlobalSection(ProjectConfigurationPlatforms) = postSolution
		{4499F2D2-18EF-4238-B1F0-7BD547AFF925}.Debug|Any CPU.ActiveCfg = Debug|Any CPU
		{4499F2D2-18EF-4238-B1F0-7BD547AFF925}.Debug|Any CPU.Build.0 = Debug|Any CPU
		{4499F2D2-18EF-4238-B1F0-7BD547AFF925}.Release|Any CPU.ActiveCfg = Release|Any CPU
		{4499F2D2-18EF-4238-B1F0-7BD547AFF925}.Release|Any CPU.Build.0 = Release|Any CPU
		{4499F2D2-18EF-4238-B1F0-7BD547AFF925}.Snapshot|Any CPU.ActiveCfg = Snapshot|Any CPU
		{4499F2D2-18EF-4238-B1F0-7BD547AFF925}.Snapshot|Any CPU.Build.0 = Snapshot|Any CPU
		{32DE4AAF-F4D6-42C5-9604-19236DE96C69}.Debug|Any CPU.ActiveCfg = Debug|Any CPU
		{32DE4AAF-F4D6-42C5-9604-19236DE96C69}.Debug|Any CPU.Build.0 = Debug|Any CPU
		{32DE4AAF-F4D6-42C5-9604-19236DE96C69}.Release|Any CPU.ActiveCfg = Release|Any CPU
		{32DE4AAF-F4D6-42C5-9604-19236DE96C69}.Release|Any CPU.Build.0 = Release|Any CPU
		{32DE4AAF-F4D6-42C5-9604-19236DE96C69}.Snapshot|Any CPU.ActiveCfg = Snapshot|Any CPU
		{32DE4AAF-F4D6-42C5-9604-19236DE96C69}.Snapshot|Any CPU.Build.0 = Snapshot|Any CPU
		{AFB035DA-CB49-49CE-B96F-581BF8368CCF}.Debug|Any CPU.ActiveCfg = Debug|Any CPU
		{AFB035DA-CB49-49CE-B96F-581BF8368CCF}.Debug|Any CPU.Build.0 = Debug|Any CPU
		{AFB035DA-CB49-49CE-B96F-581BF8368CCF}.Release|Any CPU.ActiveCfg = Debug|Any CPU
		{AFB035DA-CB49-49CE-B96F-581BF8368CCF}.Release|Any CPU.Build.0 = Debug|Any CPU
		{AFB035DA-CB49-49CE-B96F-581BF8368CCF}.Snapshot|Any CPU.ActiveCfg = Debug|Any CPU
		{AFB035DA-CB49-49CE-B96F-581BF8368CCF}.Snapshot|Any CPU.Build.0 = Debug|Any CPU
		{D22BBF3C-2177-0208-4E29-8466AA41AC6F}.Debug|Any CPU.ActiveCfg = Debug|Any CPU
		{D22BBF3C-2177-0208-4E29-8466AA41AC6F}.Debug|Any CPU.Build.0 = Debug|Any CPU
		{D22BBF3C-2177-0208-4E29-8466AA41AC6F}.Release|Any CPU.ActiveCfg = Release|Any CPU
		{D22BBF3C-2177-0208-4E29-8466AA41AC6F}.Release|Any CPU.Build.0 = Release|Any CPU
		{D22BBF3C-2177-0208-4E29-8466AA41AC6F}.Snapshot|Any CPU.ActiveCfg = Snapshot|Any CPU
		{D22BBF3C-2177-0208-4E29-8466AA41AC6F}.Snapshot|Any CPU.Build.0 = Snapshot|Any CPU
		{2DFFE2A0-1101-816B-D240-F644B2D74A68}.Debug|Any CPU.ActiveCfg = Debug|Any CPU
		{2DFFE2A0-1101-816B-D240-F644B2D74A68}.Debug|Any CPU.Build.0 = Debug|Any CPU
		{2DFFE2A0-1101-816B-D240-F644B2D74A68}.Release|Any CPU.ActiveCfg = Release|Any CPU
		{2DFFE2A0-1101-816B-D240-F644B2D74A68}.Release|Any CPU.Build.0 = Release|Any CPU
		{2DFFE2A0-1101-816B-D240-F644B2D74A68}.Snapshot|Any CPU.ActiveCfg = Snapshot|Any CPU
		{2DFFE2A0-1101-816B-D240-F644B2D74A68}.Snapshot|Any CPU.Build.0 = Snapshot|Any CPU
	EndGlobalSection
	GlobalSection(SolutionProperties) = preSolution
		HideSolutionNode = FALSE
	EndGlobalSection
	GlobalSection(ExtensibilityGlobals) = postSolution
		SolutionGuid = {838A572E-025C-4A2B-B682-68732EC0FE76}
	EndGlobalSection
	GlobalSection(Performance) = preSolution
		HasPerformanceSessions = true
	EndGlobalSection
	GlobalSection(Performance) = preSolution
		HasPerformanceSessions = true
	EndGlobalSection
EndGlobal
```

### 📁 `03-启动与版本`

#### `03-启动与版本/Java.cs`

```csharp
namespace PCLCS;

/// <summary>
/// 单个 Java 的实例。
/// </summary>
[Serializable] public class Java(string folder, Version? version = null) : IEquatable<Java> {

    /// <summary>
    /// java.exe 文件所在的文件夹路径，这通常是 bin 文件夹。以 \ 结尾。
    /// </summary>
    public readonly string Folder = PathUtils.AddSlashSuffix(PathUtils.ForCompare(folder));
    /// <summary>
    /// java.exe 文件的完整路径。
    /// </summary>
    [JsonIgnore] public string JavaExePath => Path.Combine(Folder, "java.exe");

    /// <summary>
    /// Java 的版本号（例如 16.0.1），保证 <see cref="Version.Major"/> 为其主版本。
    /// <para/> 若为 null，则还需要调用 <see cref="CheckAsync"/> 以获取版本号。
    /// </summary>
    public Version? Version = version;
    /// <summary>Java 的默认字符编码。</summary>
    [JsonIgnore] public string? FileEncodingName;
    /// <summary>
    /// Java 根据系统环境确定的本地字符编码，仅 Java 17+ 可用。
    /// <para/>若系统开启了 UTF-8，部分 Java 会错误地返回 JDK，因此不能使用 <see cref="Encoding.Default"/>。
    /// </summary>
    [JsonIgnore] public string? NativeEncodingName;

    /// <summary>
    /// 检查该 Java 是否存在问题，并获取其版本号。
    /// 返回其是否通过了检查。
    /// </summary>
    public async Task<bool> CheckAsync(CancellationToken cancellationToken = default) {
        if (Interlocked.Exchange(ref isChecked, 1) == 1 && available != null) return available!.Value; // 已检查过
        string? output = null;
        try {
            if (!FileUtils.Exists(JavaExePath)) throw new FileNotFoundException("未找到 java.exe 文件", JavaExePath);

            // 运行 -version
            output = (await TaskUtils.RunProgramAsync(JavaExePath, "-XshowSettings:properties -version", 15000, cancellationToken: cancellationToken).NoCapture()).Output.Lower();
            if (output == "") throw new InvalidOperationException("尝试运行该 Java 失败");
            Logger.Trace($"Java 检查输出：{JavaExePath}{Environment.NewLine}{output}");
            if (output.Contains("/lib/ext exists")) throw new InvalidOperationException("无法运行该 Java，请在删除 Java 文件夹中的 /lib/ext 文件夹后再试");
            if (output.Contains("a fatal error") || output.Contains("error: ")) throw new InvalidOperationException("无法运行该 Java，该 Java 或系统存在问题");
           
            // 获取版本号
            var versionString = (output.RegexSeek(@"(?<=version "")[^""]+") ?? output.RegexSeek("(?<=openjdk )[0-9]+"))?.Replace("_", ".").Replace("+", ".").BeforeFirst("-");
            if (string.IsNullOrEmpty(versionString)) throw new InvalidOperationException($"未找到该 Java 的版本号{(output.Length < 500 ? $"\n原始输出为：\n{output}" : "")}");
            if (versionString.StartsWithF("1.")) versionString = versionString!.AfterFirst("."); // 去除开头的 1.
            var segments = versionString!.Split(".", true).ToList();
            while (segments.Count < 4) segments.Add("0");
            Version = new Version(segments.Take(4).Join("."));
            if (Version.Major is <= 4 or >= 100) throw new InvalidOperationException($"分析详细信息失败，获取的版本为 {Version}{(output.Length < 500 ? $"\n原始输出为：\n{output}" : "")}");

            // 获取 Java 的相关属性
            if (!output.Contains("64-bit")) throw new InvalidOperationException("该 Java 为 32 位版本，请安装 64 位的 Java");
            string? GetProperty(string name) {
                string prefix = name + " = ";
                string? line = output.Split('\n').Select(l => l.Trim()).FirstOrDefault(l => l.StartsWithF(prefix));
                return line?[prefix.Length..].Trim();
            }
            FileEncodingName = GetProperty("file.encoding");
            NativeEncodingName = GetProperty("native.encoding") ?? FileEncodingName; // 旧版本没有 native.encoding

            Logger.Info($"检查 Java 成功：{this}");
            available = true;
        } catch (Exception ex) {
            if (ex.IsCanceled()) {
                isChecked = 0;
                throw;
            }
            Logger.Warn($"检查 Java 失败（{JavaExePath}），其输出为：\n{output ?? "无程序输出"}");
            available = false;
        }
        return available!.Value;
    }

    public void InvalidateChecked() {
        isChecked = 0;
        available = null;
    }

    [JsonIgnore] private int isChecked = 0;
    /// <summary>若已进行过检查，表示该检查是否成功。若未检查，则为 null。</summary>
    [JsonIgnore] internal bool? available = null;

    // 相同检查
    public bool Equals(Java? other)
        => other is not null && string.Equals(Folder, other.Folder, StringComparison.OrdinalIgnoreCase);
    public override bool Equals(object? obj) => obj is Java other && Equals(other);
    public override int GetHashCode() => StringComparer.OrdinalIgnoreCase.GetHashCode(Folder);

    /// <summary>用户友好的描述文本。</summary>
    public override string ToString()
        => $"Java {Version?.Major.ToString() ?? "尚未检查"} ({Version})：{Folder}";
}

public static class JavaUtils {

    /// <summary>
    /// 检查所有指定的 Java 是否存在问题，并获取其版本号。
    /// </summary>
    /// <returns>所有通过检查的 Java。</returns>
    public static async Task<List<Java>> CheckAllAsync(IEnumerable<Java> javas, CancellationToken cancellationToken = default, ProgressProvider? progress = null) {
        await TaskUtils.WhenAll(javas.Where(j => j.available == null).Select(
            async java => await java.CheckAsync(cancellationToken).NoCapture()
        ), progress).NoCapture();
        return javas.Where(j => j.available == true).ToList();
    }

    /// <summary>
    /// 将 Java 列表进行排序。
    /// 这同时会检查所有未检查的 Java。
    /// </summary>
    public static async Task<List<Java>> SortAsync(List<Java> javas, CancellationToken cancellationToken = default, ProgressProvider? progress = null) {
        javas = await JavaUtils.CheckAllAsync(javas, cancellationToken, progress).NoCapture();
        javas.Sort((left, right) => {
            // 优先使用带完整 Java 的文件夹列表中的 Java
            var isLeftInCandidateFolder = JavaUtils.CandidateFolders.Value.Any(folder => PathUtils.IsParentOf(folder, left.Folder));
            var isRightInCandidateFolder = JavaUtils.CandidateFolders.Value.Any(folder => PathUtils.IsParentOf(folder, right.Folder));
            if (isLeftInCandidateFolder != isRightInCandidateFolder) return isLeftInCandidateFolder ? -1 : 1;
            // 其次优先使用主版本号接近 21 的 Java
            var leftDistance = left.Version is null ? int.MaxValue : Math.Abs(left.Version.Major - 21);
            var rightDistance = right.Version is null ? int.MaxValue : Math.Abs(right.Version.Major - 21);
            return leftDistance.CompareTo(rightDistance);
        });
        return javas;
    }

    #region 搜索

    /// <summary>
    /// 存在完整 Java 的文件夹列表。
    /// </summary>
    internal static readonly Lazy<List<string>> CandidateFolders = new(() => {
        List<string> folders = [
            Paths.AppData + @".minecraft\runtime\", // 这也是 PCL 下载 Java 的路径
            Paths.AppData + @".hmcl\java\",
            Paths.AppData + @"ATLauncher\runtimes\minecraft\",
            Paths.AppData + @"ModrinthApp\meta\java_versions\",
            Paths.AppData + @"PrismLauncher\java\",
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile) + @"\curseforge\minecraft\Install\runtime\",
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile) + @"\.jdks\",
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile) + @"\.sdkman\candidates\java\",
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData) + @"\.ftba\bin\runtime\",
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData) + @"\Packages\Microsoft.4297127D64EC6_8wekyb3d8bbwe\LocalCache\Local\runtime\",
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86) + @"\Minecraft Launcher\runtime\",
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86) + @"\Minecraft\runtime\",
            Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments) + @"\Curse\Minecraft\Install\runtime\",
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles) + @"\Java\",
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles) + @"\Eclipse Adoptium\",
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles) + @"\Amazon Corretto\",
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles) + @"\Zulu\",
            ..DirectoryUtils.EnumerateDirectories(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles) + @"\Microsoft\", searchPattern: "jdk-*"),
            // 环境变量
            ..(Environment.GetEnvironmentVariable("JDK_HOME") + ";" + Environment.GetEnvironmentVariable("JAVA_HOME")).
                Split(";", true).Select(path => path.Trim(' ', '"'))
        ];
        return folders.Select(f => {
            try {
                return PathUtils.ForCompare(f);
            } catch (Exception ex) {
                Logger.Warn(ex, $"Java 候选文件夹路径无效（{f}）");
                return null;
            }
        }).Where(f => f is not null).Cast<string>().ToList();
    });
    

    /// <summary>
    /// 从常见文件夹和环境变量中搜索 Java，并将搜索结果写入设置。
    /// </summary>
    public static async Task RefreshListAsync(CancellationToken c = default, ProgressProvider? p = null) {
        // 搜索
        List<Java> javaList = await SearchFoldersAsync(true, CandidateFolders.Value, c, p?.SplitTo(0.5)).NoCapture();
        // 将新发现的 Java 排在最前，原有列表保留既有顺序排在后面
        var oldJavaList = Configs.JavaList.Get()!;
        oldJavaList = await JavaUtils.CheckAllAsync(oldJavaList, c, p?.SplitTo(0.9)).NoCapture(); // 检查原有列表中的 Java
        javaList.RemoveAll(oldJavaList); // 此时 javaList 中只剩下新发现的 Java
        javaList = await JavaUtils.SortAsync(javaList, c).NoCapture(); // 将所有新发现的 Java 按优先级排序
        javaList.AddRange(oldJavaList);
        javaList = javaList.Distinct().ToList();
        // 删掉已被移除的 Java
        javaList.RemoveIf(j => Configs.JavaRemovedList.Get()!.Contains(j.Folder, StringComparer.OrdinalIgnoreCase));
        // 写入设置
        Configs.JavaList.Set([..javaList]);
        Logger.Info($"Java 搜索完成，发现 {javaList.Count} 个 Java");
    }

    /// <summary>
    /// 并行搜索所有指定的文件夹及其子文件夹中的 Java，并检查其有效性。
    /// </summary>
    public static async Task<List<Java>> SearchFoldersAsync(bool includeSubDirectories, IEnumerable<string> folders, CancellationToken cancellationToken = default, ProgressProvider? progress = null) {
        // 并行搜索
        ConcurrentBag<(string Folder, bool HasReparsePoint, bool IsSpecialPath)> results = [];
        await TaskUtils.ForEachAsync(folders.Distinct(StringComparer.OrdinalIgnoreCase), 4, async (folder, cp) => {
            await Task.Run(() => {
                try {
                    foreach (var file in DirectoryUtils.EnumerateFiles(folder, includeSubDirectories, "java.exe")) {
                        cancellationToken.ThrowIfCancellationRequested();
                        string targetFolder = PathUtils.AddSlashSuffix(PathUtils.ForCompare(PathUtils.RemoveLastPart(file)));
                        // 判断文件夹是否包含重解析点（例如符号链接）
                        static bool HasReparsePoint(FileSystemInfo info) {
                            if (PathUtils.IsParentOf(Paths.AppData + @".minecraft\runtime\", info.FullName)) return false; // 不筛查 PCL 的下载路径（#8928）
                            do {
                                if (info.Attributes.HasFlagF(FileAttributes.ReparsePoint)) return true;
                                info = info is FileInfo fileInfo ? fileInfo.Directory : ((DirectoryInfo) info).Parent;
                            } while (info is not null);
                            return false;
                        }
                        // 结果
                        var hasReparsePoint = HasReparsePoint(FileUtils.GetInfo(Path.Combine(targetFolder, "java.exe")));
                        var isSpecialPath = targetFolder.Contains("java8path_target_") || targetFolder.Contains("javapath_target_") || targetFolder.Contains("javatmp") || targetFolder.Contains("system32");
                        results.Add((targetFolder, hasReparsePoint, isSpecialPath));
                    }
                } catch (UnauthorizedAccessException) {
                    Logger.Info($"快速查找 Java 时没有权限（{folder}）");
                } catch (Exception ex) {
                    ex.ThrowIfCanceled();
                    Logger.Warn(ex, $"快速查找 Java 时出错（{folder}）");
                }
            }, cancellationToken).NoCapture();
        }, cancellationToken, progress?.SplitTo(0.2)).NoCapture();
        if (!results.Any()) return [];

        // 筛除重解析点
        if (results.All(r => r.HasReparsePoint)) {
            Logger.Warn("找到的所有 Java 都包含重解析点，保留全部结果");
        } else if (results.Any(r => r.HasReparsePoint)) {
            Logger.Warn($"找到的以下 Java 包含重解析点，将它们从列表中排除：{results.Where(r => r.HasReparsePoint).Select(r => r.Folder).Join('、')}");
            results = [..results.Where(r => !r.HasReparsePoint)];
        }
        // 筛除特殊路径
        if (results.All(r => r.IsSpecialPath)) {
            Logger.Warn("找到的所有 Java 都包含特殊路径，保留全部结果");
        } else if (results.Any(r => r.IsSpecialPath)) {
            Logger.Warn($"找到的以下 Java 包含特殊路径，将它们从列表中排除：{results.Where(r => r.IsSpecialPath).Select(r => r.Folder).Join('、')}");
            results = [..results.Where(r => !r.IsSpecialPath)];
        }

        // 检查并返回
        List<Java> javaList = results.Select(r => r.Folder).Distinct(StringComparer.OrdinalIgnoreCase).Select(folder => new Java(folder)).ToList();
        javaList = (await JavaUtils.CheckAllAsync(javaList, cancellationToken, progress?.SplitTo(1)).NoCapture()).ToList();
        javaList = await JavaUtils.SortAsync(javaList, cancellationToken).NoCapture();
        Logger.Info($"快速查找 Java 完成，共有 {javaList.Count} 个候选：{javaList.Join('、')}");
        return javaList;
    }

    #endregion

}
```

#### `03-启动与版本/LaunchUtils.cs`

```csharp
namespace PCLCS;

public static class LaunchUtils {

    /// <summary>
    /// 释放补丁文件并返回完整文件路径。
    /// </summary>
    /// <param name="patchFilePath">补丁文件路径
    /// <param name="outputDirectory">补丁文件的释放目录。</param>
    public static string ExtractPatch(string patchFilePath, string outputDirectory) {
        Logger.Info($"选定的资源 {patchFilePath} 输出路径：{outputDirectory}");
        lock (ExtractPatchLock) { // 避免 OptiFine 和 Forge 安装时同时释放导致冲突
            try {
                FileUtils.ExtractResources(patchFilePath, Path.Combine(outputDirectory, PathUtils.GetLastPart(patchFilePath)), typeof(LaunchUtils));
            } catch (Exception ex) {
                if (!FileUtils.Exists(outputDirectory)) throw new FileNotFoundException($"释放 {patchFilePath} 失败", ex);
                Logger.Warn(ex, $"{patchFilePath} 文件重新释放失败，将尝试更换文件名重新生成");
                patchFilePath = Path.Combine(outputDirectory, $"{PathUtils.GetFileNameWithoutExtension(patchFilePath)}2.{PathUtils.GetExtension(patchFilePath)}");
                try {
                    FileUtils.ExtractResources(patchFilePath, Path.Combine(outputDirectory, PathUtils.GetLastPart(patchFilePath)), typeof(LaunchUtils));
                } catch (Exception ex2) {
                    throw new FileNotFoundException($"释放 {patchFilePath} 最终尝试失败", ex2);
                }
            }
        }
        return Path.Combine(outputDirectory, PathUtils.GetLastPart(patchFilePath));
    }
    private static readonly object ExtractPatchLock = new();

}
```

### 📁 `04-资源与本地Mod`

#### `04-资源与本地Mod/Resource.cs`

```csharp
namespace PCLCS;
public static class Resource {

    /// <summary>
    /// 将 CurseForge 的 ModLoaderType 映射为 <see cref="ModLoaders"/> 枚举。
    /// </summary>
    public static ModLoaders FromCurseForgeModLoaderType(int modLoaderType) => modLoaderType switch {
        // https://docs.curseforge.com/rest-api/#tocS_ModLoaderType
        1 => ModLoaders.Forge,
        3 => ModLoaders.LiteLoader,
        4 => ModLoaders.Fabric,
        6 => ModLoaders.NeoForge,
        _ => ModLoaders.None,
    };

    /// <summary>
    /// 将 <see cref="ModLoaders"/> 映射为 CurseForge 的单一 ModLoaderType。
    /// </summary>
    public static int ToCurseForgeModLoaderType(ModLoaders loaders) {
        if (loaders == ModLoaders.None) return 0;
        if (!loaders.Flags().IsSingle()) throw new ArgumentException($"传入的枚举值 {loaders} 并非单个 Mod Loader", nameof(loaders));
        return loaders switch {
            ModLoaders.Forge => 1,
            ModLoaders.LiteLoader => 3,
            ModLoaders.Fabric => 4,
            ModLoaders.NeoForge => 6,
            _ => 0,
        };
    }

}
```

#### `04-资源与本地Mod/WikiEntry.cs`

```csharp
namespace PCLCS;

/// <summary>
/// MC 百科条目。
/// <para/>使用 <see cref="WikiEntry.All"/> 获取所有条目。
/// </summary>
public class WikiEntry {

    /// <summary>
    /// 在 MC 百科中的对应 ID。
    /// </summary>
    public int Id;
    /// <summary>
    /// 中文译名。
    /// <c>null</c> 代表没有翻译。
    /// </summary>
    public string? ChineseName;
    /// <summary>
    /// 各个 Mod 平台的 Slug（例如 advanced-solar-panels）。
    /// 若没有对应键则为不在该平台上。
    /// </summary>
    public Dictionary<ResourcePlatforms, string> Slugs = [];
    /// <summary>
    /// MC 百科的浏览量逆序排行，1 代表浏览量最低。
    /// </summary>
    public int Popularity;

    /// <summary>
    /// 内置数据库中的所有 MC 百科条目。
    /// </summary>
    public static readonly Lazy<List<WikiEntry>> All = new(() => {
        var dataLines = FileUtils.ReadAsLines(@"Resource\WikiEntries.txt", type: typeof(WikiEntry)).ToList();

        // 读取最后一行的浏览量
        Queue<int> popularities = [];
        for (int i = 0; i < dataLines[^1].Length; i += 3) { // 将每 3 个字符切割成一个元素
            popularities.Enqueue(int.Parse(dataLines[^1].Substring(i, 3).ConvertRadix(86, 10)));
        }
        dataLines.RemoveAt(dataLines.Count - 1);

        // 解析每一行
        List<WikiEntry> results = [];
        var lineNumber = 0;
        foreach (var lineData in dataLines) {
            lineNumber++;
            if (lineData == "") continue;

            var popularity = popularities.Dequeue();
            foreach (var entryData in lineData.Split('¨')) {
                var entry = new WikiEntry();
                var parts = entryData.Split('|');
                var slugs = parts[0];
                if (slugs.StartsWithF("@")) {
                    entry.Slugs.Add(ResourcePlatforms.Modrinth, slugs.Replace("@", ""));
                } else if (slugs.EndsWithF("@")) {
                    entry.Slugs.Add(ResourcePlatforms.CurseForge, slugs.TrimEnd('@'));
                    entry.Slugs.Add(ResourcePlatforms.Modrinth, entry.Slugs[ResourcePlatforms.CurseForge]);
                } else if (slugs.Contains("@")) {
                    var splitSlugs = slugs.Split('@');
                    entry.Slugs.Add(ResourcePlatforms.CurseForge, splitSlugs[0]);
                    entry.Slugs.Add(ResourcePlatforms.Modrinth, splitSlugs[1]);
                } else {
                    entry.Slugs.Add(ResourcePlatforms.CurseForge, slugs);
                }

                entry.Id = lineNumber;
                entry.Popularity = popularity;
                if (parts.Length >= 2) {
                    entry.ChineseName = parts[^1]; // 最后一项
                    if (entry.ChineseName.Contains("*")) { // 处理 *
                        var englishName = entry.Slugs.Values.First().Replace("-", " ").Capitalize();
                        entry.ChineseName = entry.ChineseName.Replace("*", $" ({englishName})");
                    }
                }
                results.Add(entry);
            }
        }
        return results;
    });

    public override string ToString()
        => $"{Slugs.GetOrDefault(ResourcePlatforms.CurseForge)}&{Slugs.GetOrDefault(ResourcePlatforms.Modrinth)}|{Id}|{ChineseName}，浏览量 {Popularity}";
}
```

### 📁 `06-基础工具`

#### `06-基础工具/Configs.cs`

```csharp
namespace PCLCS;
public static class Configs {

    /// <summary>当前 Java 相关配置的版本。0 代表尚未从老的 Settings 系统中迁移。</summary>
    public static readonly ConfigEntry<int> JavaConfigVersion = new("JavaConfigVersion", 0);
    public static readonly ConfigEntry<ConcurrentList<Java>> JavaList = new("JavaList", []);
    public static readonly ConfigEntry<List<string>> JavaRemovedList = new("JavaRemovedList", []);
    public static readonly DynamicConfigEntry<bool> JavaMigrated = new("InstanceMigratedJava", false);
    public static readonly DynamicConfigEntry<Java> JavaForced = new("InstanceForcedJava", null);

    static Configs() {
        /// 当 JavaForced 获取到一个不在 JavaList 中的值时，将其重置为 null。
        JavaForced.PreviewGet += (args, provider) => {
            if (args.Value is not null && !JavaList.Get()!.Contains(args.Value)) {
                args.Value = null;
                JavaForced.Reset(provider);
            }
        };
    }
}
```

#### `06-基础工具/Constants.cs`

```csharp
namespace PCLCS;

/// <summary>
/// 社区资源的类型。
/// </summary>
[Flags]
public enum ResourceTypes {
    /// <summary>Mod。</summary>
    Mod = 1,
    /// <summary>整合包。</summary>
    ModPack = 2,
    /// <summary>资源包。</summary>
    ResourcePack = 4,
    /// <summary>光影包。</summary>
    Shader = 8,
    /// <summary>数据包。</summary>
    DataPack = 16,
    /// <summary>服务端插件。</summary>
    Plugin = 32,
    /// <summary>地图。</summary>
    Map = 64,
    /// <summary>同时包含数据包以及 Mod。</summary>
    ModOrDataPack = Mod | DataPack,
    /// <summary>允许任意种类，或种类未知。</summary>
    Any = Mod | ModPack | ResourcePack | Shader | DataPack | Plugin | Map,
}

/// <summary>
/// 社区资源的来源平台。
/// </summary>
[Flags]
public enum ResourcePlatforms {
    CurseForge = 1,
    Modrinth = 2,
    Any = CurseForge | Modrinth,
}

/// <summary>
/// Mod 加载器的类型。
/// </summary>
[Flags]
public enum ModLoaders {
    None = 0,
    Forge = 1,
    LiteLoader = 2,
    Fabric = 4,
    NeoForge = 16,
    All = Forge | LiteLoader | Fabric | NeoForge,
}

/// <summary>
/// 在爱发电中的赞助等级。
/// </summary>
public enum DonationRank {
    None = 0,
    Rank6 = 6,
    Rank12 = 12,
    Rank23 = 23,
    Rank54 = 54,
    Rank98 = 98,
}

public static class Versions {
    /// <summary>
    /// 土豆码版本号。
    /// </summary>
    public const char PotatoVersion = '1';
    /// <summary>
    /// 版本管理中 Mod 信息缓存的版本号。
    /// </summary>
    public const int LocalModCacheVersion = 19;
    /// <summary>
    /// Minecraft 本地实例信息缓存的版本号。
    /// </summary>
    public const int McInstanceCacheVersion = 38;
    /// <summary>
    /// Java 相关配置的版本号。
    /// </summary>
    public const int JavaConfigVersion = 1;
}
```

