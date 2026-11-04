//! 原版游戏安装：版本 JSON 落盘 + 支持库 + 资源。
//!
//! 与命令层分开的原因：整合包安装也要先装一遍原版，
//! 这段逻辑必须只有一份实现。

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::Result;
use crate::download::counter::SessionCounter;
use crate::download::queue::DownloadQueue;
use crate::download::session::{DownloadSession, spawn_summary_pusher};
use crate::download::sink::ProgressSink;
use crate::install::assets::AssetIndexJson;
use crate::install::plan::build_base_plan;
use crate::models::download::DownloadProgress;
use crate::models::settings::AppSettings;

/// 安装原版游戏，全程通过 `sink` 推送同一条整体进度。
pub async fn install_base(
    settings: &AppSettings,
    inst_dir: &std::path::Path,
    game_version: &str,
    sink: Arc<dyn ProgressSink>,
) -> Result<()> {
    crate::utils::paths::ensure_dir(&inst_dir.join("minecraft").join("mods"))?;

    let client = crate::utils::http::build_client(settings)?;
    let queue = Arc::new(DownloadQueue::new(
        client.clone(),
        settings.download.max_concurrent as usize,
    ));

    let manifest = crate::version::fetch_manifest(&client).await?;
    let version_detail = crate::version::resolve_detail(&client, &manifest, game_version).await?;

    // 版本 JSON 落盘：断网启动、加载器安装、后续修复都依赖它
    crate::version::detail::save_local(inst_dir, &version_detail)?;

    let base_plan = build_base_plan(&version_detail, inst_dir, settings.download.prefer_mirror)?;

    let session_id = format!(
        "install-{}",
        inst_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    let counter = Arc::new(SessionCounter::new(session_id.clone(), "install"));
    let summary_cancel = CancellationToken::new();
    let summary_handle = spawn_summary_pusher(
        Arc::clone(&counter),
        Arc::clone(&sink),
        summary_cancel.clone(),
    );

    let session = DownloadSession::new(session_id.clone(), Arc::clone(&queue));

    let result: Result<()> = async {
        counter.set_stage(1);
        if let Some(index_task) = base_plan.index_task {
            counter.add_total(1, index_task.size_hint.unwrap_or(0));
            session
                .run_with_counter(vec![index_task], Arc::clone(&sink), Arc::clone(&counter))
                .await?;
        }

        counter.set_stage(2);
        let mut all_tasks = base_plan.tasks;
        let mut all_bytes: u64 = all_tasks.iter().filter_map(|t| t.size_hint).sum();

        if let Some(asset_id) = &base_plan.asset_index_id {
            let index_path = inst_dir
                .join("minecraft")
                .join("assets")
                .join("indexes")
                .join(format!("{asset_id}.json"));
            if index_path.exists() {
                let index = AssetIndexJson::from_file(&index_path)?;
                let asset_tasks =
                    index.build_download_tasks(inst_dir, settings.download.prefer_mirror);
                all_bytes += asset_tasks.iter().filter_map(|t| t.size_hint).sum::<u64>();
                all_tasks.extend(asset_tasks);
            }
        }
        counter.add_total(all_tasks.len() as u32, all_bytes);

        session
            .run_with_counter(all_tasks, Arc::clone(&sink), Arc::clone(&counter))
            .await?;

        // 1.6 及更早版本要读 `assets/virtual/legacy`，缺了它游戏没有声音与语言文件。
        // 必须在资源下载**完成之后**做，否则对象文件还不存在。
        if base_plan.asset_index_id.as_deref() == Some("legacy") {
            let index_path = inst_dir
                .join("minecraft")
                .join("assets")
                .join("indexes")
                .join("legacy.json");
            if index_path.exists() {
                let index = AssetIndexJson::from_file(&index_path)?;
                index.materialize_virtual_legacy(inst_dir)?;
            }
        }

        Ok(())
    }
    .await;

    summary_cancel.cancel();
    summary_handle.abort();
    sink.publish(DownloadProgress::Session {
        summary: counter.snapshot(),
    });

    result
}
