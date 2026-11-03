//! Maven 坐标 → 库内相对路径。

/// 从 `group:artifact:version[:classifier]` 推导
/// `group/artifact/version/artifact-version[-classifier].jar`。
///
/// 加载器注入的依赖经常只有坐标与仓库地址，没有 `downloads.artifact`，
/// 此时只能按 Maven 规则自己拼路径。
pub fn maven_relative_path(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }

    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);

    let file_name = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };

    Some(format!("{group}/{artifact}/{version}/{file_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_plain_and_classified_paths() {
        assert_eq!(
            maven_relative_path("org.lwjgl:lwjgl:3.3.3").as_deref(),
            Some("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar")
        );
        assert_eq!(
            maven_relative_path("org.lwjgl:lwjgl:3.3.3:natives-windows").as_deref(),
            Some("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar")
        );
    }
}
