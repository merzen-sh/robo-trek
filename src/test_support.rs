use std::sync::{Arc, Mutex};

use crate::{AppState, config::Config, db::Db, metrics};

pub fn test_state(name: &str) -> (Arc<AppState>, tokio::sync::mpsc::Receiver<String>) {
    let path = format!("/tmp/robo-trek-test-{}-{}.redb", std::process::id(), name);
    let _ = std::fs::remove_file(&path);
    let db = Db::open(path).unwrap();
    let (release_tx, release_rx) = tokio::sync::mpsc::channel(8);
    let state = Arc::new(AppState {
        release_tx,
        config: Arc::new(Config {
            discord_token: "token".into(),
            discord_release_channel_id: 123,
            api_port: 3000,
            api_key: "secret".into(),
            guild_id: 456,
        }),
        db,
        metrics_history: Arc::new(Mutex::new(metrics::MetricsHistory::new(10))),
    });
    (state, release_rx)
}