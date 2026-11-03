use crate::version::rules::{OsContext, filter_libraries};
use crate::{LauncherError, Result};
use std::path::Path;

pub fn extract_natives(
    libraries: &[crate::models::version::Library],
    instance_dir: &Path,
    natives_dir: &Path,
) -> Result<()> {
    let os_ctx = OsContext::current();
    let natives_key = os_ctx.natives_classifier();
    // 强制清空旧的 natives 目录
    let _ = std::fs::remove_dir_all(natives_dir);
    crate::utils::paths::ensure_dir(natives_dir)?;

    for lib in filter_libraries(libraries, &os_ctx) {
        if let Some(dl) = &lib.downloads {
            // 只有旧版（1.18-）才有 classifiers 字段，才需要手动解压到 natives 文件夹
            if let Some(classifiers) = &dl.classifiers
                && classifiers.contains_key(natives_key)
            {
                let parts: Vec<&str> = lib.name.split(':').collect();
                if parts.len() == 3 {
                    let group = parts[0].replace('.', "/");
                    let file_name = format!("{}-{}-{}.jar", parts[1], parts[2], natives_key);
                    let jar_path = instance_dir
                        .join("libraries")
                        .join(&group)
                        .join(parts[1])
                        .join(parts[2])
                        .join(&file_name);

                    if jar_path.exists() {
                        extract_jar(&jar_path, natives_dir)?;
                    }
                }
            }
        }
    }

    Ok(())
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
        if entry.name().ends_with(".dll")
            || entry.name().ends_with(".so")
            || entry.name().ends_with(".dylib")
        {
            let out_path = natives_dir.join(Path::new(entry.name()).file_name().unwrap());
            let mut out = std::fs::File::create(&out_path).map_err(|e| LauncherError::Io {
                message: format!("创建 natives 文件失败: {e}"),
            })?;
            std::io::copy(&mut entry, &mut out).map_err(|e| LauncherError::Io {
                message: format!("写入 natives 文件失败: {e}"),
            })?;
        }
    }
    Ok(())
}
