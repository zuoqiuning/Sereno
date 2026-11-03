pub mod args;
pub mod classpath;
pub mod natives;
pub mod options;
pub mod process;

use crate::Result;
use crate::models::version::VersionDetail;

pub struct LaunchConfig {
    pub java_exe: String,
    pub instance_dir: std::path::PathBuf,
    pub version: VersionDetail,
    pub options: args::LaunchOptions,
}

pub struct PreparedLaunch {
    pub java_exe: String,
    pub instance_dir: std::path::PathBuf,
    pub args: args::LaunchArgs,
}

pub fn prepare(config: &LaunchConfig) -> Result<PreparedLaunch> {
    let natives_dir = config.instance_dir.join("natives");
    let game_dir = config.instance_dir.join("minecraft");
    let assets_dir = game_dir.join("assets");
    crate::utils::paths::ensure_dir(&game_dir)?;
    crate::utils::paths::ensure_dir(&assets_dir)?;

    // `--lang` 不会覆盖 options.txt 里已有的 lang，必须直接改文件
    if config.options.auto_chinese {
        let mc_version = config.version.jar.as_deref().unwrap_or(&config.version.id);
        options::ensure_chinese(&game_dir, mc_version)?;
    }

    // 加载器版本没有自己的 client.jar，必须沿用父版本（合并时写进 `jar`）
    let jar_id = config.version.jar.as_deref().unwrap_or(&config.version.id);
    let client_jar = config
        .instance_dir
        .join("versions")
        .join(jar_id)
        .join(format!("{jar_id}.jar"));

    // 1. 解压本地库 (natives .dll/.so)
    natives::extract_natives(
        &config.version.libraries,
        &config.instance_dir,
        &natives_dir,
    )?;

    // 2. 构建 Classpath
    let classpath =
        classpath::build_classpath(&config.version.libraries, &config.instance_dir, &client_jar);

    // 3. 构建参数
    let args = args::build_args(
        &config.version,
        &classpath,
        &natives_dir.to_string_lossy(),
        &game_dir.to_string_lossy(),
        &assets_dir.to_string_lossy(),
        &config.instance_dir.join("libraries").to_string_lossy(),
        &config.options,
    )?;

    Ok(PreparedLaunch {
        java_exe: config.java_exe.clone(),
        instance_dir: config.instance_dir.clone(),
        args,
    })
}
