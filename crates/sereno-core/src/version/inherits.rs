//! `inheritsFrom` 继承链的合并。
//!
//! ## 为什么需要合并
//!
//! 有一类版本（加载器注入的版本、部分整合包）**只声明自己额外加的东西**，
//! 其余部分从父版本继承：
//!
//! ```json
//! { "id": "1.21.1-forge-52.0.16", "inheritsFrom": "1.21.1", "libraries": [ ... ] }
//! ```
//!
//! 启动器要下载的依赖库其实是「父的 + 子的」，只看子版本会漏掉绝大部分依赖。
//!
//! ## 合并规则
//!
//! - **依赖库**：父的在前、子的在后；同坐标（Maven `name`）以**子**为准；
//! - **标量字段**（主类、资源目录名、Java 版本……）：子声明了就用子的，
//!   没声明就继承父的；
//! - **下载条目**：逐键覆盖（子的 `client` 覆盖父的 `client`，
//!   父有 `server` 而子没有时保留父的）；
//! - 合并完成后 `inheritsFrom` 被清空——合并结果已经是一个完整版本。
//!
//! ## 规则求值的时机
//!
//! **不在合并时过滤依赖库**。规则是在「当前系统上是否生效」的问题，
//! 合并只管拼装；过滤交给 [`crate::version::rules::filter_libraries`]，
//! 这样同一份合并结果可以在不同系统上得出不同结论。

use crate::models::version::{Downloads, Library, VersionDetail};
use crate::{LauncherError, Result};

/// 继承链的最大深度。
///
/// 真实世界最多两三层；超过 16 层基本是数据损坏或恶意构造的环。
pub const MAX_INHERIT_DEPTH: usize = 16;

/// 合并依赖库列表：父的在前，子覆盖同名项。
pub fn merge_libraries(parent: &[Library], child: &[Library]) -> Vec<Library> {
    let mut merged: Vec<Library> = parent.to_vec();

    for lib in child {
        match merged.iter().position(|l| l.name == lib.name) {
            Some(pos) => merged[pos] = lib.clone(),
            None => merged.push(lib.clone()),
        }
    }

    merged
}

/// 合并下载条目：逐键以子为准。
fn merge_downloads(parent: Option<&Downloads>, child: Option<&Downloads>) -> Option<Downloads> {
    match (parent, child) {
        (None, c) => c.cloned(),
        (p, None) => p.cloned(),
        (Some(p), Some(c)) => Some(Downloads {
            client: c.client.clone(),
            server: c.server.clone().or_else(|| p.server.clone()),
            client_mappings: c
                .client_mappings
                .clone()
                .or_else(|| p.client_mappings.clone()),
            server_mappings: c
                .server_mappings
                .clone()
                .or_else(|| p.server_mappings.clone()),
        }),
    }
}

/// 合并启动参数对象。
///
/// 官方语义：`arguments.game` / `arguments.jvm` 是**累加**的（父在前、子在后），
/// 其余键由子版本覆盖。加载器注入的参数（如 Forge 的 `--launchTarget`）
/// 正是靠这一步才不会被丢掉。
fn merge_arguments(
    parent: Option<&serde_json::Value>,
    child: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    match (parent, child) {
        (None, c) => c.cloned(),
        (p, None) => p.cloned(),
        (Some(p), Some(c)) => {
            let (Some(p_obj), Some(c_obj)) = (p.as_object(), c.as_object()) else {
                return Some(c.clone());
            };
            let mut merged = p_obj.clone();
            for (key, value) in c_obj {
                if matches!(key.as_str(), "game" | "jvm")
                    && let (Some(base), Some(extra)) = (merged.get(key), value.as_array())
                {
                    let mut list = base.as_array().cloned().unwrap_or_default();
                    list.extend(extra.iter().cloned());
                    merged.insert(key.clone(), serde_json::Value::Array(list));
                    continue;
                }
                merged.insert(key.clone(), value.clone());
            }
            Some(serde_json::Value::Object(merged))
        }
    }
}

/// 把子版本合并到父版本之上，得到一个完整的版本。
///
/// 传入顺序是「父 → 子」。
pub fn merge_inherits(parent: &VersionDetail, child: &VersionDetail) -> VersionDetail {
    let mut merged = parent.clone();

    merged.libraries = merge_libraries(&parent.libraries, &child.libraries);
    merged.downloads = merge_downloads(parent.downloads.as_ref(), child.downloads.as_ref());
    merged.arguments = merge_arguments(parent.arguments.as_ref(), child.arguments.as_ref());
    merged.minecraft_arguments = child
        .minecraft_arguments
        .clone()
        .or_else(|| parent.minecraft_arguments.clone());

    // 客户端 jar 永远沿用父版本：加载器版本不产出自己的 client.jar。
    // 链条多层时，父版本的 jar 已经被解析成根版本，这里直接继承。
    merged.jar = child
        .jar
        .clone()
        .or_else(|| parent.jar.clone())
        .or_else(|| Some(parent.id.clone()));

    // 标量字段：子声明了就用子的
    if child.main_class.is_some() {
        merged.main_class = child.main_class.clone();
    }
    if child.minimum_launcher_version.is_some() {
        merged.minimum_launcher_version = child.minimum_launcher_version;
    }
    if child.release_time.is_some() {
        merged.release_time = child.release_time.clone();
    }
    if child.time.is_some() {
        merged.time = child.time.clone();
    }
    if child.assets.is_some() {
        merged.assets = child.assets.clone();
    }
    if child.asset_index.is_some() {
        merged.asset_index = child.asset_index.clone();
    }
    if child.logging.is_some() {
        merged.logging = child.logging.clone();
    }
    if child.java_version.is_some() {
        merged.java_version = child.java_version.clone();
    }

    // 身份信息永远取子版本
    merged.id = child.id.clone();
    merged.version_type = child.version_type;

    // 合并结果已是完整版本，不再有继承关系
    merged.inherits_from = None;

    merged
}

/// 沿 `inheritsFrom` 链向上找到根版本，再从根往下逐层合并。
///
/// `fetch` 负责按版本号取到父版本的详情（通常走 HTTP 或本地缓存）。
///
/// 继承链出现环时（a → b → a）会被 [`MAX_INHERIT_DEPTH`] 拦下，
/// 而不是无限循环把栈跑爆。
pub fn resolve_with<F>(start: &VersionDetail, fetch: F) -> Result<VersionDetail>
where
    F: Fn(&str) -> Result<VersionDetail>,
{
    // chain 从「叶子」排到「根」
    let mut chain: Vec<VersionDetail> = vec![start.clone()];
    let mut current = start.clone();
    let mut depth = 0usize;

    while let Some(parent_id) = current.inherits_from.clone() {
        depth += 1;
        if depth > MAX_INHERIT_DEPTH {
            return Err(LauncherError::Parse {
                message: format!(
                    "版本 {} 的 inheritsFrom 链超过 {MAX_INHERIT_DEPTH} 层，疑似循环引用",
                    start.id
                ),
            });
        }

        let parent = fetch(&parent_id)?;
        current = parent.clone();
        chain.push(parent);
    }

    Ok(merge_chain(chain))
}

/// [`resolve_with`] 的异步版本：父版本要现去网络取。
pub async fn resolve_with_async<F, Fut>(start: &VersionDetail, fetch: F) -> Result<VersionDetail>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<VersionDetail>>,
{
    let mut chain: Vec<VersionDetail> = vec![start.clone()];
    let mut current = start.clone();
    let mut depth = 0usize;

    while let Some(parent_id) = current.inherits_from.clone() {
        depth += 1;
        if depth > MAX_INHERIT_DEPTH {
            return Err(LauncherError::Parse {
                message: format!(
                    "版本 {} 的 inheritsFrom 链超过 {MAX_INHERIT_DEPTH} 层，疑似循环引用",
                    start.id
                ),
            });
        }

        let parent = fetch(parent_id).await?;
        current = parent.clone();
        chain.push(parent);
    }

    Ok(merge_chain(chain))
}

/// 把「根 → 叶」的版本链逐层合并成一个完整版本。
pub(crate) fn merge_chain(chain: Vec<VersionDetail>) -> VersionDetail {
    let mut chain = chain;
    // chain 目前是「叶 → 根」，反转成「根 → 叶」
    chain.reverse();

    let mut merged = chain.remove(0);
    for child in chain {
        merged = merge_inherits(&merged, &child);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::version::{DownloadInfo, JavaVersion};

    fn lib(name: &str) -> Library {
        Library {
            name: name.to_string(),
            url: None,
            downloads: None,
            rules: vec![],
            natives: None,
            extract: None,
            client: None,
            server: None,
        }
    }

    fn detail(id: &str, libraries: Vec<Library>) -> VersionDetail {
        VersionDetail {
            id: id.to_string(),
            version_type: crate::models::version::VersionType::Release,
            main_class: None,
            minimum_launcher_version: None,
            release_time: None,
            time: None,
            assets: None,
            asset_index: None,
            downloads: None,
            libraries,
            logging: None,
            java_version: None,
            inherits_from: None,
            arguments: None,
            minecraft_arguments: None,
            jar: None,
        }
    }

    #[test]
    fn child_arguments_are_appended_after_parent() {
        let mut parent = detail("parent", vec![]);
        parent.arguments = Some(serde_json::json!({
            "game": ["--version", "${version_name}"],
            "jvm": ["-cp", "${classpath}"]
        }));

        let mut child = detail("child", vec![]);
        child.arguments = Some(serde_json::json!({
            "game": ["--launchTarget", "forgeclient"]
        }));

        let merged = merge_inherits(&parent, &child);
        let game = merged.arguments.unwrap()["game"]
            .as_array()
            .unwrap()
            .clone();
        let values: Vec<&str> = game.iter().filter_map(|v| v.as_str()).collect();

        assert_eq!(
            values,
            vec![
                "--version",
                "${version_name}",
                "--launchTarget",
                "forgeclient"
            ],
            "父参数在前、子参数在后"
        );
    }

    #[test]
    fn loader_inherits_parent_jar() {
        let parent = detail("1.21.1", vec![lib("a")]);
        let child = VersionDetail {
            inherits_from: Some("1.21.1".into()),
            ..detail("1.21.1-forge-52.0.16", vec![lib("b")])
        };

        let merged = merge_inherits(&parent, &child);
        assert_eq!(
            merged.jar.as_deref(),
            Some("1.21.1"),
            "加载器版本沿用父 jar"
        );
    }

    #[test]
    fn minecraft_arguments_fall_back_to_parent() {
        let mut parent = detail("parent", vec![]);
        parent.minecraft_arguments = Some("--username ${auth_player_name}".into());

        let child = detail("child", vec![]);
        let merged = merge_inherits(&parent, &child);

        assert_eq!(
            merged.minecraft_arguments.as_deref(),
            Some("--username ${auth_player_name}")
        );
    }

    #[test]
    fn child_libraries_are_appended_after_parent() {
        let merged = merge_libraries(&[lib("a"), lib("b")], &[lib("c")]);
        let names: Vec<&str> = merged.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn child_overrides_same_coordinate_library() {
        let mut parent_lib = lib("a:b:1");
        parent_lib.natives = None;
        let mut child_lib = lib("a:b:1");
        child_lib.natives = Some(crate::models::version::NativeLibrary {
            classifier: "natives-windows".into(),
        });

        let merged = merge_libraries(&[parent_lib], &[child_lib]);
        assert_eq!(merged.len(), 1, "同坐标不应重复");
        assert!(merged[0].natives.is_some(), "应以子版本为准");
    }

    #[test]
    fn merge_keeps_parent_scalars_when_child_omits() {
        let mut parent = detail("parent", vec![lib("a")]);
        parent.main_class = Some("Parent.Main".into());
        parent.assets = Some("12".into());
        parent.java_version = Some(JavaVersion {
            component: "java-runtime-gamma".into(),
            major_version: 21,
        });

        let child = detail("child", vec![lib("b")]);
        let merged = merge_inherits(&parent, &child);

        assert_eq!(merged.id, "child", "身份取子版本");
        assert_eq!(merged.main_class.as_deref(), Some("Parent.Main"));
        assert_eq!(merged.assets.as_deref(), Some("12"));
        assert_eq!(merged.required_java_major(), Some(21));
        assert_eq!(merged.libraries.len(), 2, "父的库 + 子的库");
        assert!(merged.inherits_from.is_none());
    }

    #[test]
    fn child_scalars_override_parent() {
        let mut parent = detail("parent", vec![]);
        parent.main_class = Some("Parent.Main".into());
        parent.java_version = Some(JavaVersion {
            component: "c".into(),
            major_version: 17,
        });

        let mut child = detail("child", vec![]);
        child.main_class = Some("Child.Main".into());
        child.java_version = Some(JavaVersion {
            component: "c".into(),
            major_version: 21,
        });

        let merged = merge_inherits(&parent, &child);
        assert_eq!(merged.main_class.as_deref(), Some("Child.Main"));
        assert_eq!(merged.required_java_major(), Some(21));
    }

    #[test]
    fn downloads_merge_key_by_key() {
        let parent = VersionDetail {
            downloads: Some(Downloads {
                client: DownloadInfo {
                    url: "parent-client".into(),
                    sha1: "a".into(),
                    size: 1,
                    path: None,
                },
                server: Some(DownloadInfo {
                    url: "parent-server".into(),
                    sha1: "b".into(),
                    size: 2,
                    path: None,
                }),
                client_mappings: None,
                server_mappings: None,
            }),
            ..detail("parent", vec![])
        };

        let child = VersionDetail {
            downloads: Some(Downloads {
                client: DownloadInfo {
                    url: "child-client".into(),
                    sha1: "c".into(),
                    size: 3,
                    path: None,
                },
                server: None,
                client_mappings: None,
                server_mappings: None,
            }),
            ..detail("child", vec![])
        };

        let merged = merge_inherits(&parent, &child);
        let downloads = merged.downloads.unwrap();
        assert_eq!(downloads.client.url, "child-client");
        assert_eq!(downloads.server.unwrap().url, "parent-server");
    }

    #[test]
    fn resolves_chain_from_root_to_leaf() {
        let root = detail("root", vec![lib("root-lib")]);
        let mid = VersionDetail {
            inherits_from: Some("root".into()),
            ..detail("mid", vec![lib("mid-lib")])
        };
        let leaf = VersionDetail {
            inherits_from: Some("mid".into()),
            ..detail("leaf", vec![lib("leaf-lib")])
        };

        let resolved = resolve_with(&leaf, |id| {
            Ok(match id {
                "root" => root.clone(),
                "mid" => mid.clone(),
                other => panic!("不该请求 {other}"),
            })
        })
        .unwrap();

        assert_eq!(resolved.id, "leaf");
        assert_eq!(resolved.libraries.len(), 3);
        let names: Vec<&str> = resolved.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["root-lib", "mid-lib", "leaf-lib"]);
    }

    #[test]
    fn cyclic_inheritance_is_rejected() {
        // a → b → a
        let a = VersionDetail {
            inherits_from: Some("b".into()),
            ..detail("a", vec![])
        };
        let b = VersionDetail {
            inherits_from: Some("a".into()),
            ..detail("b", vec![])
        };

        let err =
            resolve_with(&a, |id| Ok(if id == "a" { a.clone() } else { b.clone() })).unwrap_err();

        assert_eq!(err.code(), "parse");
    }
}
