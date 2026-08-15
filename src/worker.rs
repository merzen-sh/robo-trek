use std::sync::Arc;

use serenity::{
    builder::{CreateAttachment, CreateMessage},
    http::Http,
    model::id::ChannelId,
};

use crate::{render, storages::releases::ReleaseStore};

/// A unit of work for the background worker. Add a variant here (and a branch
/// in `process`) for each new task type; the worker loop dispatches on it.
#[derive(Debug, PartialEq)]
pub enum Task {
    Release { version: String },
}

/// Shared dependencies every task handler may need. Clone to pass around.
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

/// Spawns the worker loop. Drains tasks from the bounded channel and runs each
/// through `process` sequentially, so ordering is preserved and backpressure
/// applies. Returns a `JoinHandle` so the caller can monitor for unexpected
/// exit.
pub fn spawn(
    mut rx: tokio::sync::mpsc::Receiver<Task>,
    state: WorkerState,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(task) = rx.recv().await {
            if let Err(e) = process(&task, &state).await {
                eprintln!("failed to process task {task:?}: {e}");
            }
        }
    })
}

async fn process(task: &Task, state: &WorkerState) -> Result<(), String> {
    match task {
        Task::Release { version } => process_release(version, state).await,
    }
}

/// Renders the release card, caches it in SQLite, and posts it to Discord.
/// Rendering via Headless Chrome and caching are CPU/IO bound, so they run
/// inside `spawn_blocking`; the Discord send stays on the async runtime.
async fn process_release(version: &str, state: &WorkerState) -> Result<(), String> {
    let version = version.to_string();
    let render_version = version.clone();

    let png = tokio::task::spawn_blocking(move || render::release_card(&render_version))
        .await
        .map_err(|e| format!("render task failed: {e}"))??;

    state
        .releases
        .put_release(&version, &png)
        .await
        .map_err(|e| format!("failed to cache release: {e}"))?;

    let attachment = CreateAttachment::bytes(png, "release.png");
    let msg = CreateMessage::new()
        .content(format!("Version {version} is out!"))
        .add_file(attachment);
    state
        .channel_id
        .send_message(&state.http, msg)
        .await
        .map_err(|e| format!("discord send failed: {e}"))?;

    Ok(())
}
