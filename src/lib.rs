use crate::config::Config;
use crossbeam_channel::Sender;
use std::sync::Arc;

pub mod api;
pub mod commands;
pub mod config;
pub mod handler;
pub mod render;
pub mod worker;

#[derive(Clone)]
pub struct AppState {
    pub release_tx: Sender<String>,
    pub config: Arc<Config>,
}
