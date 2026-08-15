use crate::config::Config;
use std::sync::{Arc, Mutex};

pub mod api;
pub mod config;
pub mod db;
pub mod discord;
pub mod handlers;
pub mod kv;
pub mod metrics;
pub mod middlewares;
pub mod models;
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
    pub kv: kv::Kv,
    pub db: db::Db,
    pub tickets: storages::tickets::TicketStore,
    pub releases: storages::releases::ReleaseStore,
    pub metrics_history: Arc<Mutex<metrics::MetricsHistory>>,
}

impl AppState {
    /// Builds the application state from its backing resources and returns the
    /// worker queue receiver to hand to `worker::spawn`.
    pub fn new(
        config: Arc<Config>,
        kv: kv::Kv,
        db: db::Db,
        metrics_history: Arc<Mutex<metrics::MetricsHistory>>,
    ) -> (Self, tokio::sync::mpsc::Receiver<worker::Task>) {
        let (task_tx, task_rx) = tokio::sync::mpsc::channel(64);
        (
            Self {
                task_tx,
                config,
                kv,
                db: db.clone(),
                tickets: storages::tickets::TicketStore::new(db.clone()),
                releases: storages::releases::ReleaseStore::new(db.clone()),
                metrics_history,
            },
            task_rx,
        )
    }
}
