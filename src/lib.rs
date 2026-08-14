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
    pub release_tx: tokio::sync::mpsc::Sender<String>,
    pub config: Arc<Config>,
    pub kv: kv::Kv,
    pub db: db::Db,
    pub tickets: storages::tickets::TicketStore,
    pub metrics_history: Arc<Mutex<metrics::MetricsHistory>>,
}
