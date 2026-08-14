use crate::config::Config;
use std::sync::Arc;

pub mod api;
pub mod commands;
pub mod config;
pub mod db;
pub mod handler;
pub mod handlers;
pub mod middlewares;
pub mod render;
pub mod routes;
pub mod worker;

#[cfg(test)]
pub mod test_support;

#[derive(Clone)]
pub struct AppState {
    pub release_tx: tokio::sync::mpsc::Sender<String>,
    pub config: Arc<Config>,
    pub db: db::Db,
}
