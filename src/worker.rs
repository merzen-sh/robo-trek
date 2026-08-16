use std::sync::Arc;

use serenity::{
    builder::{CreateAttachment, CreateMessage},
    http::Http,
    model::id::ChannelId,
};
use tracing::error;

use crate::{render, storages::releases::ReleaseStore};

/// A unit of work for the background worker.
#[derive(Debug, PartialEq)]
pub enum Task {
    Release { title: String, content: String },
}

/// Shared dependencies for task handlers.
#[derive(Clone)]
pub struct WorkerState {
    pub http: Arc<Http>,
    pub channel_id: ChannelId,
    pub releases: ReleaseStore,
}

impl WorkerState {
    pub fn new(http: Arc<Http>, channel_id: ChannelId, releases: ReleaseStore) -> Self {
        Self {
            http,
            channel_id,
            releases,
        }
    }
}

/// Spawns the worker loop, processing tasks sequentially as they arrive.
pub fn spawn(
    mut rx: tokio::sync::mpsc::Receiver<Task>,
    state: WorkerState,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(task) = rx.recv().await {
            if let Err(e) = process(&task, &state).await {
                error!("failed to process task {task:?}: {e}");
            }
        }
    })
}

async fn process(task: &Task, state: &WorkerState) -> Result<(), String> {
    match task {
        Task::Release { title, content } => process_release(title, content, state).await,
    }
}

/// Renders the release card, caches it in SQLite, and posts it to Discord.
async fn process_release(title: &str, content: &str, state: &WorkerState) -> Result<(), String> {
    let title = title.to_string();
    let content = content.to_string();
    let render_content = content.clone();

    let png = tokio::task::spawn_blocking(move || render::release_card(&title, &render_content))
        .await
        .map_err(|e| format!("render task failed: {e}"))??;

    state
        .releases
        .put_release(&content, &png)
        .await
        .map_err(|e| format!("failed to cache release: {e}"))?;

    let attachment = CreateAttachment::bytes(png, "release.png");
    let msg = CreateMessage::new()
        .content(format!("Version {content} is out!"))
        .add_file(attachment);
    state
        .channel_id
        .send_message(&state.http, msg)
        .await
        .map_err(|e| format!("discord send failed: {e}"))?;

    Ok(())
}
