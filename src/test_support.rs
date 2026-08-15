use std::sync::{Arc, Mutex};

use crate::{AppState, config::Config, db, kv::Kv, metrics};

pub async fn test_state(
    name: &str,
) -> (
    Arc<AppState>,
    tokio::sync::mpsc::Receiver<crate::worker::Task>,
) {
    let path = format!("/tmp/robo-trek-test-{}-{}.redb", std::process::id(), name);
    let _ = std::fs::remove_file(&path);
    let kv = Kv::open(path).unwrap();
    let db = db::Db::open_in_memory().await.unwrap();
    let (state, task_rx) = AppState::new(
        Arc::new(Config {
            discord_token: "token".into(),
            discord_release_channel_id: 123,
            discord_tickets_channel_id: 789,
            api_port: 3000,
            api_key: "secret".into(),
            guild_id: 456,
        }),
        kv,
        db,
        Arc::new(Mutex::new(metrics::MetricsHistory::new(10))),
    );
    (Arc::new(state), task_rx)
}
