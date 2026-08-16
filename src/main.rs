use std::sync::Arc;

use robo_trek::{
    AppState, api, config, db, discord, logging, metrics, prometheus, storages, worker,
};
use serenity::{model::id::ChannelId, prelude::*};
use tracing::{error, info, warn};

// Multi-threaded runtime: the Discord gateway, the API server, and the
// release worker all run concurrently, while Headless Chrome rendering is
// isolated on Tokio's blocking pool.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Arc::new(config::Config::from_env()?);
    let metrics = Arc::new(prometheus::Metrics::new());
    logging::init_tracing(&config);

    let db = db::Db::open("robo-trek.sqlite").await?;
    let tickets = storages::tickets::TicketStore::new(db.clone());

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&config.discord_token, intents)
        .event_handler(discord::Handler::new(Arc::clone(&config), tickets.clone()))
        .await?;

    let http = client.http.clone();
    let channel_id = ChannelId::new(config.discord_release_channel_id);

    let (state, task_rx) = AppState::new(Arc::clone(&config), metrics, db);
    let state = Arc::new(state);
    let worker_state = worker::WorkerState::new(http, channel_id, state.releases.clone());

    // Spawning Services
    let worker_task = worker::spawn(task_rx, worker_state);
    let sampler_task = metrics::spawn_sampler(state.prometheus.clone());
    let api_task = tokio::spawn(api::serve(state));

    // Monitoring Background Tasks (Isolation Phase)
    let api_monitor = tokio::spawn(async move {
        match api_task.await {
            Ok(Ok(())) => warn!("API server stopped cleanly"),
            Ok(Err(e)) => error!("API server error: {e}"),
            Err(e) => error!("API server task panicked: {e}"),
        }
    });

    let worker_monitor = tokio::spawn(async move {
        match worker_task.await {
            Ok(()) => warn!("Release worker stopped cleanly"),
            Err(e) => error!("Release worker task panicked: {e}"),
        }
    });

    let sampler_monitor = tokio::spawn(async move {
        match sampler_task.await {
            Ok(()) => warn!("Metrics sampler stopped cleanly"),
            Err(e) => error!("Metrics sampler task panicked: {e}"),
        }
    });

    let shard_manager = client.shard_manager.clone();
    let started = client.start();

    // Main Critical Loop
    tokio::select! {
        result = started => match result {
            Ok(()) => info!("Discord client stopped"),
            Err(e) => error!("Discord client error: {e:?}"),
        },
        _ = shutdown_signal() => {
            info!("Shutting down via signal");
        }
    }

    // Teardown
    info!("Cleaning up resources...");
    shard_manager.shutdown_all().await;

    api_monitor.abort();
    worker_monitor.abort();
    sampler_monitor.abort();

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
