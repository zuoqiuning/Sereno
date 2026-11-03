//! Fabric 加载器安装：复用 Meta 通用流程。

use std::path::Path;

use crate::Result;
use crate::models::instance::LoaderInfo;
use crate::models::settings::AppSettings;

pub async fn install_fabric(
    client: &reqwest::Client,
    settings: &AppSettings,
    instance_dir: &Path,
    mc_version: &str,
    specified_version: Option<&str>,
) -> Result<LoaderInfo> {
    crate::loader::meta::install_meta(
        client,
        settings,
        instance_dir,
        mc_version,
        specified_version,
        &crate::loader::meta::FABRIC,
    )
    .await
}
