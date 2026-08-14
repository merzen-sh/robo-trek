use std::sync::Arc;

use serenity::{
    builder::{CreateAttachment, CreateMessage},
    http::Http,
    model::id::ChannelId,
};

use crate::{db, render};

/// Spawns the release worker task.
///
/// Drains release versions from a bounded channel. Rendering via Headless
/// Chrome and caching via redb are CPU/IO bound, so they run inside
/// `spawn_blocking`; the Discord send stays on the async runtime. Returns a
/// `JoinHandle` so the caller can monitor for unexpected exit.
pub fn spawn(
    mut release_rx: tokio::sync::mpsc::Receiver<String>,
    http: Arc<Http>,
    channel_id: ChannelId,
    db: db::Db,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(version) = release_rx.recv().await {
            if let Err(e) = process_release(&version, &http, channel_id, &db).await {
                eprintln!("failed to process release {version}: {e}");
            }
        }
    })
}

async fn process_release(
    version: &str,
    http: &Http,
    channel_id: ChannelId,
    db: &db::Db,
) -> Result<(), String> {
    let version = version.to_string();
    let render_version = version.clone();

    let png = tokio::task::spawn_blocking(move || render::release_card(&render_version))
        .await
        .map_err(|e| format!("render task failed: {e}"))??;

    db.put_release(&version, &png).await?;

    let attachment = CreateAttachment::bytes(png, "release.png");
    let msg = CreateMessage::new()
        .content(format!("Version {version} is out!"))
        .add_file(attachment);
    channel_id
        .send_message(http, msg)
        .await
        .map_err(|e| format!("discord send failed: {e}"))?;

    Ok(())
}
