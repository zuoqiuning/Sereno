//! 旧版（1.13 之前）natives 解压。
//!
//! ## 为什么需要"增量"
//!
//! 这些版本的 LWJGL 2 不会自己从 jar 里取 `.dll`，必须由启动器解压到
//! `natives/` 才能被 `-Djava.library.path` 找到。而解压逻辑以前是
//! **每次启动都把整个目录删掉重建**——几百个文件、几十到几百毫秒，
//! 每次点"启动"都白付一次。
//!
//! 这里加一个内容签名（jar 路径 + 大小 + mtime）：签名没变就说明
//! natives 依赖没动过，直接复用上次解压的结果。

use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

use crate::version::rules::{OsContext, filter_libraries};
use crate::{LauncherError, Result};

/// 签名文件名（放在 natives 目录里）。
const STAMP_FILE: &str = ".sereno-natives";

pub fn extract_natives(
    libraries: &[crate::models::version::Library],
    instance_dir: &Path,
    natives_dir: &Path,
) -> Result<()> {
    let jars = native_jars(libraries, instance_dir);

    let signature = signature_of(&jars);
    let stamp = natives_dir.join(STAMP_FILE);
    if !jars.is_empty()
        && natives_dir.is_dir()
        && std::fs::read_to_string(&stamp)
            .map(|existing| existing.trim() == signature)
            .unwrap_or(false)
    {
        tracing::debug!(dir = %natives_dir.display(), "natives 未变化，复用上次解压结果");
        return Ok(());
    }

    // 依赖变了（或首次启动）：清空重建，避免旧 dll 残留导致版本串味
    let _ = std::fs::remove_dir_all(natives_dir);
    crate::utils::paths::ensure_dir(natives_dir)?;

    for jar_path in &jars {
        extract_jar(jar_path, natives_dir)?;
    }

    let _ = std::fs::write(&stamp, format!("{signature}\n"));
    Ok(())
}

/// 收集当前平台上需要解压的 natives jar。
fn native_jars(libraries: &[crate::models::version::Library], instance_dir: &Path) -> Vec<PathBuf> {
    let os_ctx = OsContext::current();
    let mut jars = Vec::new();

    for lib in filter_libraries(libraries, &os_ctx) {
        let Some(classifiers) = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()) else {
            continue;
        };
        let Some(natives_key) = crate::version::rules::native_classifier(lib, &os_ctx) else {
            continue;
        };
        let Some(native_artifact) = classifiers.get(&natives_key) else {
            continue;
        };

        // 优先用 JSON 里的真实路径：natives jar 的文件名并不总是
        // `artifact-version-classifier.jar`，按坐标硬拼会漏。
        let relative = native_artifact
            .path
            .as_deref()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let parts: Vec<&str> = lib.name.split(':').collect();
                if parts.len() != 3 {
                    return None;
                }
                Some(
                    PathBuf::from(parts[0].replace('.', "/"))
                        .join(parts[1])
                        .join(parts[2])
                        .join(format!("{}-{}-{}.jar", parts[1], parts[2], natives_key)),
                )
            });

        let Some(relative) = relative else {
            continue;
        };
        let jar_path = instance_dir.join("libraries").join(&relative);

        if jar_path.exists() {
            jars.push(jar_path);
        } else {
            // 缺 natives 会在启动后表现为 UnsatisfiedLinkError，
            // 提前一条 warn 比让用户对着崩溃日志猜要好。
            tracing::warn!(
                lib = %lib.name,
                path = %jar_path.display(),
                "natives jar 不存在，跳过解压；请重新修复实例"
            );
        }
    }

    jars
}

/// 依赖集合的内容签名：任一个 jar 被替换/升级，签名就会变。
fn signature_of(jars: &[PathBuf]) -> String {
    let mut hasher = Sha1::new();
    for jar in jars {
        hasher.update(jar.to_string_lossy().as_bytes());
        if let Ok(meta) = std::fs::metadata(jar) {
            hasher.update(meta.len().to_le_bytes());
            if let Ok(modified) = meta.modified()
                && let Ok(stamp) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                hasher.update(stamp.as_millis().to_le_bytes());
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

fn extract_jar(jar_path: &Path, natives_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(jar_path).map_err(|e| LauncherError::Io {
        message: format!("打开 natives jar 失败: {e}"),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| LauncherError::Parse {
        message: format!("解析 natives jar 失败: {e}"),
    })?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| LauncherError::Parse {
            message: format!("读取 zip 条目失败: {e}"),
        })?;
        // 只取本机可加载的二进制：META-INF、签名、文本一律忽略
        if !(entry.name().ends_with(".dll")
            || entry.name().ends_with(".so")
            || entry.name().ends_with(".dylib"))
        {
            continue;
        }
        let Some(file_name) = Path::new(entry.name()).file_name() else {
            continue;
        };
        let out_path = natives_dir.join(file_name);
        let mut out = std::fs::File::create(&out_path).map_err(|e| LauncherError::Io {
            message: format!("创建 natives 文件失败: {e}"),
        })?;
        std::io::copy(&mut entry, &mut out).map_err(|e| LauncherError::Io {
            message: format!("写入 natives 文件失败: {e}"),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(tmp: &Path, classifier: &str) -> Vec<crate::models::version::Library> {
        use crate::models::version::{DownloadInfo, Library, LibraryDownloads};
        use std::collections::BTreeMap;

        let mut classifiers = BTreeMap::new();
        classifiers.insert(
            classifier.to_string(),
            DownloadInfo {
                url: "https://example.com/x.jar".into(),
                sha1: "x".into(),
                size: 1,
                path: None,
            },
        );
        // 按 classifier 反推系统键，保证在任何平台上跑这份测试都能命中
        let os_key = if classifier.contains("osx") || classifier.contains("macos") {
            "osx"
        } else if classifier.contains("linux") {
            "linux"
        } else {
            "windows"
        };
        let library = Library {
            name: "org.lwjgl:lwjgl:2.9.4".into(),
            url: None,
            downloads: Some(LibraryDownloads {
                artifact: None,
                classifiers: Some(classifiers),
            }),
            rules: vec![],
            natives: Some(BTreeMap::from([(
                os_key.to_string(),
                classifier.to_string(),
            )])),
            extract: None,
            client: None,
            server: None,
        };

        let dir = tmp.join("libraries").join("org/lwjgl/lwjgl/2.9.4");
        std::fs::create_dir_all(&dir).unwrap();

        // 必须是**合法 ZIP**：解压路径会真的打开它，假字节会让测试失败在
        // 「解析 jar」而不是「是否重复解压」上。
        let jar_path = dir.join(format!("lwjgl-2.9.4-{classifier}.jar"));
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .start_file(
                    "test.dll",
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            use std::io::Write;
            writer.write_all(b"fake dll").unwrap();
            writer.finish().unwrap();
        }
        std::fs::write(&jar_path, cursor.into_inner()).unwrap();

        vec![library]
    }

    #[test]
    fn skips_second_extraction_when_signature_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let classifier = OsContext::current().natives_classifier().to_string();
        let libs = setup(tmp.path(), &classifier);
        let natives = tmp.path().join("natives");

        // 首次：目录为空 → 建目录并写签名
        extract_natives(&libs, tmp.path(), &natives).unwrap();
        assert!(natives.join(STAMP_FILE).is_file(), "应写入签名文件");

        // 手工放一个标记文件：如果第二次重新解压，它会被清掉
        let marker = natives.join("marker.txt");
        std::fs::write(&marker, b"keep").unwrap();

        extract_natives(&libs, tmp.path(), &natives).unwrap();
        assert!(marker.exists(), "签名未变时不应清空目录重解压");
    }
}
