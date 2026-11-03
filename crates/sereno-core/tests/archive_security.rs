//! 归档解压的安全集成测试。
//!
//! fixture 在 `tests/fixtures/archives/`，由同目录的 `generate.py` 生成
//! （**恶意构造的**，不含任何可执行内容，只有纯文本）。
//!
//! 每个用例都断言两件事：
//! 1. `extract_zip_safely` 返回错误；
//! 2. **目标目录没有新增任何文件**——这是《安全审计清单》的硬要求，
//!    「被拒绝」不等于「拒绝得干净」。

use std::fs;
use std::path::{Path, PathBuf};

use sereno_core::utils::archive::extract_zip_safely;
use tempfile::TempDir;

/// 取一个 fixture 的绝对路径。
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("archives")
        .join(name)
}

/// 解压到临时目录，返回 (结果, 持有目录生命周期的 TempDir)。
fn extract_to_temp(
    archive: &Path,
) -> (
    Result<sereno_core::utils::archive::ExtractionReport, sereno_core::LauncherError>,
    TempDir,
) {
    let temp = TempDir::new().unwrap();
    let dest = temp.path().join("out");
    let result = extract_zip_safely(fs::File::open(archive).unwrap(), &dest);
    (result, temp)
}

/// 统计目录里的文件数（不含目录）。
fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .count()
}

/// 所有恶意归档。每个都必须被拒绝。
const MALICIOUS: &[&str] = &[
    "traversal.zip",
    "absolute.zip",
    "symlink.zip",
    "case-conflict.zip",
    "reserved-name.zip",
    "zip-bomb.zip",
];

#[test]
fn rejects_all_malicious_archives() {
    for name in MALICIOUS {
        let (result, _keep) = extract_to_temp(&fixture(name));
        assert!(result.is_err(), "{name} 应被拒绝");
    }
}

#[test]
fn rejection_leaves_no_files_behind() {
    for name in MALICIOUS {
        let out = TempDir::new().unwrap();
        let dest = out.path().join("target");
        let result = extract_zip_safely(fs::File::open(fixture(name)).unwrap(), &dest);

        assert!(result.is_err(), "{name} 应被拒绝");

        let files = if dest.exists() { count_files(&dest) } else { 0 };
        assert_eq!(files, 0, "{name} 被拒绝后目标目录仍有 {files} 个文件");
    }
}

#[test]
fn rejects_path_traversal_with_clear_reason() {
    let (result, _keep) = extract_to_temp(&fixture("traversal.zip"));
    let err = result.unwrap_err().to_string();
    assert!(err.contains("路径穿越"), "错误信息应说明是路径穿越: {err}");
}

#[test]
fn rejects_symlink_with_clear_reason() {
    let (result, _keep) = extract_to_temp(&fixture("symlink.zip"));
    let err = result.unwrap_err().to_string();
    assert!(err.contains("符号链接"), "错误信息应说明是符号链接: {err}");
}

#[test]
fn rejects_oversized_entry_with_clear_reason() {
    let (result, _keep) = extract_to_temp(&fixture("zip-bomb.zip"));
    let err = result.unwrap_err().to_string();
    assert!(err.contains("过大"), "错误信息应说明是体积超限: {err}");
}

#[test]
fn rejects_case_conflict_with_clear_reason() {
    let (result, _keep) = extract_to_temp(&fixture("case-conflict.zip"));
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("大小写冲突"),
        "错误信息应说明是大小写冲突: {err}"
    );
}

#[test]
fn extracts_a_normal_archive() {
    let (result, _keep) = extract_to_temp(&fixture("normal.zip"));
    let report = result.expect("正常归档应能解压");

    assert_eq!(report.files, 2);

    let out = TempDir::new().unwrap();
    let dest = out.path().join("target");
    extract_zip_safely(fs::File::open(fixture("normal.zip")).unwrap(), &dest).unwrap();

    assert_eq!(
        fs::read_to_string(dest.join("mods").join("example.jar")).unwrap(),
        "jar-content"
    );
    assert_eq!(
        fs::read_to_string(dest.join("readme.txt")).unwrap(),
        "hello"
    );
}
