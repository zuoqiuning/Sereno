# 测试 fixtures

`crates/sereno-core/tests/fixtures/` —— 集成测试用的样本数据。

## 目录

| 目录 | 放什么 | 谁用 |
| :--- | :--- | :--- |
| `archives/` | **恶意归档**与正常归档 | `tests/archive_security.rs` |
| `versions/` | Mojang 版本清单 / 版本详情 JSON 样本 | `version/` 域（待实现） |
| `java/` | `java -version` 输出样本、架构样本 | `java/` 域（待实现） |
| `crash/` | 崩溃报告、`latest.log`、JVM 错误日志样本 | `crash/` 域（待实现） |
| `mrpack/` | Modrinth 整合包（`.mrpack`）样本 | `resource/` 域（待实现） |

**只有 `archives/` 目前有内容**，其余目录是骨架，等对应业务模块开工时填。

## 归档 fixture

`archives/` 里的归档是**恶意构造的**，用途只有一个：断言解压被拒绝。
**里面不含任何可执行内容**，只有纯文本。

重新生成：

```powershell
python crates/sereno-core/tests/fixtures/archives/generate.py
```

生成脚本同目录下的 `generate.py`，里面写了每个样本模拟什么攻击。

### 为什么用 Python 生成

`zip` crate（Rust）写出的归档一律标记为 **MS-DOS 系统**（version_made_by 高字节 = 0），
读回来时 `unix_mode()` 只能拿到 DOS 推导值，**符号链接信息根本不可见**。
而真实的恶意归档（Linux 上 `zip -y` 打的）标记的是 Unix 系统（= 3），
符号链接藏在 `external_attr` 的高 16 位里。Python 的 `zipfile` 能直接设
`create_system = 3`，构造出真实场景。

### 样本清单

| 文件 | 模拟的攻击 | 期望结果 |
| :--- | :--- | :--- |
| `traversal.zip` | 条目名含 `../`（路径穿越） | 拒绝 |
| `absolute.zip` | 条目名是绝对路径（`C:/...`） | 拒绝 |
| `symlink.zip` | 条目是符号链接 | 拒绝 |
| `case-conflict.zip` | `Mod.jar` 与 `mod.jar`（Windows 同名） | 拒绝 |
| `reserved-name.zip` | 条目名是 Windows 保留名 `CON` | 拒绝 |
| `zip-bomb.zip` | 头部声明 3 GB，实际几百字节 | 拒绝 |
| `normal.zip` | 正常归档 | **必须能正常解压**（对照组） |

断言要求见《安全审计清单》：**被拒绝且目标目录无新增文件**。
