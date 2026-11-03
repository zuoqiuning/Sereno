# 版本域测试固件

这些文件模拟 Mojang 官方格式的 JSON，用来验证 `core/version/` 的解析与求值。

**它们是「裁剪过的真实格式」，不是随手编的形状**：字段名、层级、
`rules` 的写法都与 Mojang 的 `version_manifest_v2.json` 和版本详情 JSON 一致。
但**内容是假的**（sha1 是占位、URL 指向 example.invalid），不会真的下载任何东西。

| 文件 | 模拟什么 |
| :--- | :--- |
| `manifest.json` | 版本清单顶层：`latest` 指针 + 5 条版本摘要（release / snapshot / old_beta），其中 `1.21.1-loader` 用于测 `inheritsFrom` |
| `detail-1.21.1.json` | 一个正式版的完整详情：主类、资源索引、下载条目、6 个依赖库、3 组平台规则、Java 21 要求 |
| `detail-loader.json` | 一个「加载器注入版」：只声明自己加的 2 个依赖 + `inheritsFrom: "1.21.1"` |

## 依赖库里的三类规则

`detail-1.21.1.json` 刻意覆盖了三种典型形态：

1. **无规则**（`com.mojang:brigadier`）—— 所有系统都要；
2. **按系统名**（`natives-windows` / `natives-macos` / `natives-linux`）—— 互斥；
3. **带版本正则**（`netty-transport-native-unix-common` 的 `^10\\.`）——
   只有 Windows 10+ 才命中，用来验证正则求值与 `RtlGetVersion` 取到的真实版本。

## 集成测试

`crates/sereno-core/tests/version_manifest.rs` 读取这些固件，跑完整链路：

```
解析清单 → 查版本 → 解析详情 → 按当前系统过滤依赖 → 合并继承链
```

不发起任何网络请求。
