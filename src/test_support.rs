use std::sync::Arc;

use crate::{AppState, config::Config, db, prometheus};

pub async fn test_state(
    _name: &str,
) -> (
    Arc<AppState>,
    tokio::sync::mpsc::Receiver<crate::worker::Task>,
) {
    let db = db::Db::open_in_memory().await.unwrap();
    let (state, task_rx) = AppState::new(
        Arc::new(Config {
            discord_token: "token".into(),
            discord_release_channel_id: 123,
            discord_tickets_channel_id: 789,
            api_port: 3000,
            api_key: "secret".into(),
            guild_id: 456,
            log_level: "info".into(),
        }),
        Arc::new(prometheus::Metrics::new()),
        db,
    );
    (Arc::new(state), task_rx)
}
