use crate::config::Config;
use std::sync::Arc;

pub mod api;
pub mod config;
pub mod db;
pub mod discord;
pub mod handlers;
pub mod logging;
pub mod metrics;
pub mod middlewares;
pub mod models;
pub mod prometheus;
pub mod render;
pub mod routes;
pub mod storages;
pub mod worker;

#[cfg(test)]
pub mod test_support;

#[derive(Clone)]
pub struct AppState {
    pub task_tx: tokio::sync::mpsc::Sender<worker::Task>,
    pub config: Arc<Config>,
    pub prometheus: Arc<prometheus::Metrics>,
    pub db: db::Db,
    pub tickets: storages::tickets::TicketStore,
    pub releases: storages::releases::ReleaseStore,
}

impl AppState {
    /// Builds the application state from its backing resources and returns the
    /// worker queue receiver to hand to `worker::spawn`.
    pub fn new(
        config: Arc<Config>,
        prometheus: Arc<prometheus::Metrics>,
        db: db::Db,
    ) -> (Self, tokio::sync::mpsc::Receiver<worker::Task>) {
        let (task_tx, task_rx) = tokio::sync::mpsc::channel(64);
        (
            Self {
                task_tx,
                config,
                prometheus,
                db: db.clone(),
                tickets: storages::tickets::TicketStore::new(db.clone()),
                releases: storages::releases::ReleaseStore::new(db.clone()),
            },
            task_rx,
        )
    }
}

pub async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
