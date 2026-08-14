use std::sync::{Arc, Mutex};

use robo_trek::{AppState, api, config, db, handler, metrics, worker};
use serenity::{model::id::ChannelId, prelude::*};

// Multi-threaded runtime: the Discord gateway, the API server, and the
// release worker all run concurrently, while Headless Chrome rendering is
// isolated on Tokio's blocking pool.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Arc::new(config::Config::from_env()?);
    let db = db::Db::open("robo-trek.redb")?;

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&config.discord_token, intents)
        .event_handler(handler::Handler::new(Arc::clone(&config)))
        .await?;

    let http = client.http.clone();
    let channel_id = ChannelId::new(config.discord_release_channel_id);

    // Bounded queue gives backpressure when the worker falls behind.
    let (release_tx, release_rx) = tokio::sync::mpsc::channel(64);

    let state = Arc::new(AppState {
        release_tx,
        config: Arc::clone(&config),
        db: db.clone(),
        metrics_history: Arc::new(Mutex::new(metrics::MetricsHistory::new(60))),
    });

    let worker_task = worker::spawn(release_rx, http, channel_id, db.clone());
    let sampler_task = metrics::spawn_sampler(db, state.metrics_history.clone());
    let api_task = tokio::spawn(api::serve(state));
    let shard_manager = client.shard_manager.clone();
    let started = client.start();

    tokio::select! {
        result = api_task => match result {
            Ok(Ok(())) => println!("API server stopped"),
            Ok(Err(e)) => eprintln!("API server error: {e}"),
            Err(e) => eprintln!("API server task panicked: {e}"),
        },
        result = started => match result {
            Ok(()) => println!("Discord client stopped"),
            Err(e) => eprintln!("Discord client error: {e:?}"),
        },
        result = worker_task => match result {
            Ok(()) => println!("release worker stopped"),
            Err(e) => eprintln!("release worker panicked: {e}"),
        },
        result = sampler_task => match result {
            Ok(()) => println!("metrics sampler stopped"),
            Err(e) => eprintln!("metrics sampler panicked: {e}"),
        },
        _ = shutdown_signal() => {
            println!("shutting down");
            shard_manager.shutdown_all().await;
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
