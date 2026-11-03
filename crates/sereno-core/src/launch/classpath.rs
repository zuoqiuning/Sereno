use std::path::Path;

use crate::models::version::Library;
use crate::version::artifact::maven_relative_path;
use crate::version::rules::{OsContext, applies_to_client, filter_libraries};

pub fn build_classpath(libraries: &[Library], instance_dir: &Path, client_jar: &Path) -> String {
    let os_ctx = OsContext::current();
    let mut cp = Vec::new();

    for lib in filter_libraries(libraries, &os_ctx) {
        if !applies_to_client(lib) {
            continue;
        }
        if let Some(relative) = library_relative_path(lib) {
            let path = instance_dir.join("libraries").join(relative);
            cp.push(path.to_string_lossy().replace('\\', "/"));
        }
    }

    cp.push(client_jar.to_string_lossy().replace('\\', "/"));
    if cfg!(windows) {
        cp.join(";")
    } else {
        cp.join(":")
    }
}

/// 该依赖库对应的 jar 在 `libraries/` 下的相对路径。
///
/// 三类库的处理：
/// 1. 正常库：优先用 Mojang 声明的 `downloads.artifact.path`，
///    没有 path 时按 Maven 坐标推导；
/// 2. 旧版 natives 库（只有 `downloads.classifiers`）：不放进 classpath，
///    它们会被解压到 `natives/`；
/// 3. 只有 Maven 仓库地址的库：按坐标推导路径（下载任务在安装计划里生成）。
fn library_relative_path(lib: &Library) -> Option<String> {
    if let Some(downloads) = &lib.downloads {
        return downloads
            .artifact
            .as_ref()
            .and_then(|artifact| artifact.path.clone())
            .or_else(|| maven_relative_path(&lib.name));
    }

    lib.url
        .as_ref()
        .and_then(|_| maven_relative_path(&lib.name))
}
