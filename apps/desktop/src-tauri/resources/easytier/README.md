# easytier-resources

本目录存放**构建期生成**的 EasyTier 内核 zip（随安装包分发，首次联机零下载）。

生成方式（不入库，30MB 二进制不进 git）：

```
node scripts/fetch-easytier.mjs
```

生成物 `kernel.zip` 已被 .gitignore 排除；运行时 `kernel.rs` 会再次校验
其 SHA-256 与硬编码官方摘要一致，校验失败自动回退在线下载。
