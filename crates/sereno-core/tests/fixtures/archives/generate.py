"""生成恶意归档 fixture。

**这些归档是恶意构造的**，用途只有一个：断言解压被拒绝。
里面**不含任何可执行内容**，只有纯文本。

用法（需要 Python 3）：

```powershell
python crates/sereno-core/tests/fixtures/archives/generate.py
```

为什么用 Python 而不是 Rust 的 `zip` crate：
`zip` crate 写出的归档一律标记为 **MS-DOS 系统**（version_made_by 高字节 = 0），
读回来时 `unix_mode()` 只能拿到 DOS 推导值，**符号链接信息根本不可见**。
而真实的恶意归档（Linux 上 `zip -y` 打的）标记的是 Unix 系统（= 3），
符号链接就藏在 `external_attr` 的高 16 位里。Python 的 `zipfile` 可以直接
设置 `create_system = 3`，能构造出真实场景。
"""

import os
import struct
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))

# 文件类型位
S_IFREG = 0o100000
S_IFLNK = 0o120000


def write_zip(name, entries):
    """写一个 Unix 系统的 zip。

    entries: [(条目名, 内容, 权限位), ...]
    """
    path = os.path.join(HERE, name)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        for entry_name, content, mode in entries:
            info = zipfile.ZipInfo(entry_name)
            info.create_system = 3  # Unix —— 符号链接元数据靠这个才可见
            info.external_attr = (mode & 0xFFFF) << 16
            zf.writestr(info, content)
    print(f"  生成 {name}")
    return path


def patch_declared_size(path, claimed_size):
    """把中央目录里的「解压后大小」改成声称值，实际数据保持很小。

    这是最常见的 zip bomb 形态：头部声明 3 GB，实际只有几百字节。
    解压器若信任头部就会往磁盘上写 3 GB；只信实际字节数又会漏掉
    「单文件超限」这条规则。两边都要查。
    """
    with open(path, "rb") as handle:
        data = bytearray(handle.read())

    # 中央目录文件头签名 PK\x01\x02
    signature = b"PK\x01\x02"
    index = data.find(signature)
    if index < 0:
        raise RuntimeError("没找到中央目录头")
    # uncompressed size 位于中央目录头偏移 24，长 4 字节（小端）
    struct.pack_into("<I", data, index + 24, claimed_size)

    with open(path, "wb") as handle:
        handle.write(data)
    print(f"  声明大小改写为 {claimed_size} 字节: {os.path.basename(path)}")


def main():
    print(f"生成目录: {HERE}")

    # 1. 路径穿越
    write_zip(
        "traversal.zip",
        [
            ("../evil.txt", "pwned", S_IFREG | 0o644),
            ("normal.txt", "ok", S_IFREG | 0o644),
        ],
    )

    # 2. 绝对路径（盘符）
    write_zip(
        "absolute.zip",
        [("C:/Windows/evil.dll", "pwned", S_IFREG | 0o644)],
    )

    # 3. 符号链接
    write_zip(
        "symlink.zip",
        [("link", "/etc/passwd", S_IFLNK | 0o777)],
    )

    # 4. 大小写冲突（Windows 上 Mod.jar 与 mod.jar 是同一个文件）
    write_zip(
        "case-conflict.zip",
        [
            ("Mod.jar", "a", S_IFREG | 0o644),
            ("mod.jar", "b", S_IFREG | 0o644),
        ],
    )

    # 5. Windows 保留名
    write_zip(
        "reserved-name.zip",
        [("CON", "pwned", S_IFREG | 0o644)],
    )

    # 6. zip bomb —— 头部声明 3 GB，实际内容只有几百字节
    bomb = write_zip(
        "zip-bomb.zip",
        [("bomb.bin", b"0" * 512, S_IFREG | 0o644)],
    )
    patch_declared_size(bomb, 3 * 1024 * 1024 * 1024)

    # 7. 正常归档（对照组，必须能正常解压）
    write_zip(
        "normal.zip",
        [
            ("mods/example.jar", "jar-content", S_IFREG | 0o644),
            ("readme.txt", "hello", S_IFREG | 0o644),
        ],
    )

    print("完成")


if __name__ == "__main__":
    main()
