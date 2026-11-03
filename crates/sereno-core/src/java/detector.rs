//! Java 探测：扫描系统并解析 `java -version`。

use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::models::java::JavaCandidate;
use crate::{LauncherError, Result};

/// 探测当前系统上所有可用的 Java。
///
/// ## 覆盖范围
///
/// 参考 PCL 的候选目录清单，逐类补齐：
///
/// 1. `JAVA_HOME`、`PATH`（显式配置最可靠）；
/// 2. 发行版默认安装目录：Oracle、Adoptium、Zulu、BellSoft、Microsoft、
///    Amazon Corretto、IBM Semeru；
/// 3. 启动器自带的运行时：Minecraft 官方启动器（`runtime`）、HMCL、Prism、
///    ATLauncher、Modrinth App；
/// 4. 开发工具链目录：`.jdks`、`.sdkman`、Gradle 缓存；
/// 5. 启动器自管的 `%APPDATA%\Sereno\java`。
///
/// 目录扫描**最多下探 3 层**（`runtime/<组件>/bin/java.exe` 恰好是 3 层），
/// 再深就会在 Program Files 里扫出无关目录。
///
/// **未覆盖**：Windows 注册表。注册表里的项基本都同时存在于上面的目录，
/// 收益不大，却要额外引入一次注册表读取。
pub async fn detect_all() -> Result<Vec<JavaCandidate>> {
    let mut candidates = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. JAVA_HOME：最常见、最可靠的显式配置
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let java_exe = PathBuf::from(home).join("bin").join("java.exe");
        if java_exe.exists() {
            add_candidate(java_exe, &mut candidates, &mut seen_paths).await;
        }
    }

    // 2. PATH 中的 java
    if let Ok(path_env) = std::env::var("PATH") {
        for path in std::env::split_paths(&path_env) {
            let java_exe = path.join("java.exe");
            if java_exe.exists() {
                add_candidate(java_exe, &mut candidates, &mut seen_paths).await;
            }
        }
    }

    // 3. 常见安装目录与启动器运行时
    for root in candidate_roots() {
        scan_root(&root, 0, &mut candidates, &mut seen_paths).await;
    }

    // 新版本在前，方便界面直接展示推荐项
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.major_version));
    Ok(candidates)
}

/// 候选根目录清单。
///
/// 每个元素是一棵「可能包含 `bin/java.exe`」的目录：
/// 既可能是 `Java\jdk-21`（要下探一层），也可能是
/// `HMCL\java` 这类已经直接含 `bin\java.exe` 的目录。
fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    let pf = std::env::var("ProgramFiles").ok();
    let pf86 = std::env::var("ProgramFiles(x86)").ok();
    let local = std::env::var("LOCALAPPDATA").ok();
    let roaming = std::env::var("APPDATA").ok();
    let home = dirs::home_dir();

    // 3.1 发行版默认安装位置
    for base in [pf.as_deref(), pf86.as_deref()] {
        let Some(base) = base else { continue };
        for sub in [
            "Java",
            "Eclipse Adoptium",
            "Eclipse Foundation",
            "Zulu",
            "BellSoft",
            "Microsoft",
            "Amazon Corretto",
            "IBM Semeru",
            "Java\\jdk",
        ] {
            roots.push(PathBuf::from(base).join(sub));
        }
    }

    // 3.2 LOCALAPPDATA\Programs（用户级安装，安装器默认装到这里）
    if let Some(base) = local.as_deref() {
        for sub in [
            "Programs\\Eclipse Adoptium",
            "Programs\\Eclipse Foundation",
            "Programs\\Zulu",
            "Programs\\Microsoft",
            "Programs\\Amazon Corretto",
            "Programs\\BellSoft",
            "Programs\\Java",
        ] {
            roots.push(PathBuf::from(base).join(sub));
        }

        // 3.3 微软商店版 Minecraft 官方启动器自带的运行时
        if let Ok(entries) = std::fs::read_dir(PathBuf::from(base).join("Packages")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.starts_with("microsoft.4297127d64ec6") {
                    roots.push(
                        entry
                            .path()
                            .join("LocalCache")
                            .join("Local")
                            .join("runtime"),
                    );
                }
            }
        }
    }

    // 3.4 第三方启动器的运行时目录
    if let Some(base) = roaming.as_deref() {
        for sub in [
            "PrismLauncher\\java",
            "PrismLauncher\\meta",
            "ModrinthApp\\java",
            "HMCL\\java",
            "ATLauncher\\runtime",
            ".minecraft\\runtime",
        ] {
            roots.push(PathBuf::from(base).join(sub));
        }
    }

    // 3.5 开发工具链目录
    if let Some(home) = home {
        for sub in [".jdks", ".sdkman\\candidates\\java", ".gradle\\jdks"] {
            roots.push(home.join(sub));
        }
    }

    // 3.6 启动器自管的 JRE（自动下载的 Zulu）
    if let Ok(state) = crate::utils::paths::state_dir() {
        roots.push(state.join("java"));
    }

    roots
}

/// 在目录里查找 `bin/java.exe`，最多下探三层。
///
/// 用 Box::pin 包装递归，避免 async 递归的无限尺寸类型错误。
fn scan_root<'a>(
    root: &'a Path,
    depth: usize,
    candidates: &'a mut Vec<JavaCandidate>,
    seen_paths: &'a mut std::collections::HashSet<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        const MAX_DEPTH: usize = 3;
        if depth > MAX_DEPTH {
            return;
        }

        // 目录本身就是 Java 根目录
        let direct = root.join("bin").join("java.exe");
        if direct.exists() {
            add_candidate(direct, candidates, seen_paths).await;
        }
        if depth == MAX_DEPTH {
            return;
        }

        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            // 跳过明显无关的大目录，避免在 Program Files 里扫穿
            if name.starts_with('.') && name != ".jdks" && name != ".sdkman" && name != ".gradle" {
                continue;
            }
            scan_root(&path, depth + 1, candidates, seen_paths).await;
        }
    })
}

/// 探测一个候选路径并在去重后加入列表；探测失败静默跳过。
async fn add_candidate(
    path: PathBuf,
    candidates: &mut Vec<JavaCandidate>,
    seen_paths: &mut std::collections::HashSet<String>,
) {
    let canonical = path.canonicalize().unwrap_or(path);
    let key = normalize_path_key(&canonical.to_string_lossy());
    if seen_paths.contains(&key) {
        return;
    }
    if let Ok(candidate) = probe_java(&canonical).await {
        seen_paths.insert(normalize_path_key(&candidate.path));
        candidates.push(candidate);
    }
}

/// 让前端「重新扫描」时也能直接探测用户指定的路径。
pub async fn probe(path: &str) -> Result<JavaCandidate> {
    probe_java(Path::new(path)).await
}

/// 按版本要求选出最合适的 Java。
///
/// 规则：
/// 1. 用户手动指定的路径优先（但仍要过架构校验）；
/// 2. 精确主版本优先；
/// 3. 没有精确版本时，**只在现代版本（≥16）上允许向上兼容**——
///    1.8 用 Java 21 启动会直接崩，宁可给出「需要 Java 8」的明确错误；
/// 4. 64 位系统上排除 32 位 Java（32 位启动 64 位 natives 必失败）。
pub async fn resolve(
    required_major: Option<u32>,
    custom_path: Option<&str>,
) -> Result<JavaCandidate> {
    if let Some(path) = custom_path.map(str::trim).filter(|p| !p.is_empty()) {
        let mut candidate = probe(path).await?;
        ensure_launchable(&candidate)?;
        candidate.is_custom = true;
        return Ok(candidate);
    }

    let mut candidates = detect_all().await?;
    if cfg!(target_pointer_width = "64") {
        candidates.retain(|c| c.arch != "x86");
    }

    let required = required_major.unwrap_or(8);

    if let Some(exact) = candidates.iter().find(|c| c.major_version == required) {
        return Ok(exact.clone());
    }

    if required >= 16
        && let Some(newer) = candidates
            .iter()
            .filter(|c| c.major_version > required)
            .min_by_key(|c| c.major_version)
    {
        return Ok(newer.clone());
    }

    Err(LauncherError::JavaNotFound {
        required_major: required,
    })
}

/// 启动前的运行时校验。
///
/// 目前只做「位数不匹配」这一项硬性检查：32 位 Java 启动 64 位游戏
/// 必然在 natives 加载阶段失败，提前拦住比事后看崩溃日志好得多。
/// 架构一致但版本偏低的情况由 [`resolve`] 的版本规则负责。
pub fn ensure_launchable(candidate: &JavaCandidate) -> Result<()> {
    if cfg!(target_pointer_width = "64") && candidate.arch == "x86" {
        return Err(LauncherError::JavaArchMismatch {
            path: candidate.path.clone(),
            arch: "32 位 (x86)".into(),
            system_arch: "64 位".into(),
        });
    }
    Ok(())
}

/// 将路径转换为统一的去重标识（去掉 `\\?\` 前缀并转小写）。
fn normalize_path_key(raw_path: &str) -> String {
    let stripped = raw_path.trim_start_matches(r"\\?\");
    stripped.to_lowercase()
}

/// 执行 `java -XshowSettings:properties -version` 并解析输出。
///
/// 用 `-XshowSettings:properties` 而不是裸 `-version`：
/// 一次调用就能同时拿到 `java.version`、`os.arch`（架构是启动前的硬性校验项），
/// 比跑两次进程省一半时间。老版本 Java 不支持这个参数时，退回 `-version`。
async fn probe_java(path: &Path) -> Result<JavaCandidate> {
    let mut output = Command::new(path)
        .arg("-XshowSettings:properties")
        .arg("-version")
        .output()
        .await
        .map_err(|e| LauncherError::Io {
            message: format!("执行 Java 探测失败 {}: {e}", path.display()),
        })?;

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    // 极老的 Java 不认这个参数时，输出里不会有 java.version，回退一次
    if !combined.contains("java.version") {
        output = Command::new(path)
            .arg("-version")
            .output()
            .await
            .map_err(|e| LauncherError::Io {
                message: format!("执行 Java 探测失败 {}: {e}", path.display()),
            })?;
    }
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let (major_version, version_string) =
        parse_version(&combined).ok_or_else(|| LauncherError::Parse {
            message: format!("无法解析 Java 版本输出: {combined}"),
        })?;

    // 架构优先取 `os.arch`（权威值），取不到再用输出文本里的线索
    let arch = match parse_property(&combined, "os.arch").as_deref() {
        Some("amd64") | Some("x86_64") => "x64".to_string(),
        Some("aarch64") | Some("arm64") => "arm64".to_string(),
        Some("x86") | Some("i386") | Some("i686") => "x86".to_string(),
        Some(other) => other.to_string(),
        None if combined.contains("ARM64") || combined.contains("aarch64") => "arm64".to_string(),
        None if combined.contains("64-Bit") || combined.contains("x86_64") => "x64".to_string(),
        None => "x86".to_string(),
    };

    // 存给前端展示的路径，去掉 `\\?\`，好看一点
    let display_path = path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();

    Ok(JavaCandidate {
        path: display_path,
        major_version,
        version_string,
        arch,
        is_custom: false,
    })
}

/// 从 `-XshowSettings:properties` 的输出里取一个属性值。
fn parse_property(output: &str, key: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix(&format!("{key} = "))?;
        let value = rest.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

fn parse_version(output: &str) -> Option<(u32, String)> {
    let re = regex::Regex::new(r#"(?:java|openjdk) version "([^"]+)""#).ok()?;
    let caps = re.captures(output)?;
    let version_str = caps.get(1)?.as_str();

    let major = if version_str.starts_with("1.") {
        version_str.split('.').nth(1)?.parse::<u32>().ok()?
    } else {
        version_str.split('.').next()?.parse::<u32>().ok()?
    };

    Some((major, version_str.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jdk8() {
        let out = r#"java version "1.8.0_402" 2024-01-16"#;
        let (major, ver) = parse_version(out).unwrap();
        assert_eq!(major, 8);
        assert_eq!(ver, "1.8.0_402");
    }

    #[test]
    fn parses_jdk21() {
        let out = r#"openjdk version "21.0.4" 2024-07-16"#;
        let (major, ver) = parse_version(out).unwrap();
        assert_eq!(major, 21);
        assert_eq!(ver, "21.0.4");
    }

    #[test]
    fn reads_properties_from_show_settings() {
        let out = r#"
Property settings:
    java.home = C:\Program Files\Java\jdk-21
    java.version = 21.0.4
    os.arch = amd64
    user.dir = D:\Desktop\sereno
"#;
        assert_eq!(parse_property(out, "os.arch").as_deref(), Some("amd64"));
        assert_eq!(
            parse_property(out, "java.version").as_deref(),
            Some("21.0.4")
        );
        assert_eq!(parse_property(out, "missing"), None);
    }

    #[test]
    fn rejects_32_bit_java_on_64_bit_system() {
        let candidate = JavaCandidate {
            path: "C:\\Java\\jre8\\bin\\java.exe".into(),
            major_version: 8,
            version_string: "1.8.0_402".into(),
            arch: "x86".into(),
            is_custom: false,
        };

        let result = ensure_launchable(&candidate);
        if cfg!(target_pointer_width = "64") {
            assert!(result.is_err(), "64 位系统必须拒绝 32 位 Java");
        }

        let good = JavaCandidate {
            arch: "x64".into(),
            ..candidate
        };
        assert!(ensure_launchable(&good).is_ok());
    }
}
