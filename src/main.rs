use std::sync::Arc;

use crossbeam_channel::unbounded;
use robo_trek::{AppState, api, config, handler, worker};
use serenity::{model::id::ChannelId, prelude::*};

#[tokio::main]
async fn main() {
    let config = Arc::new(config::Config::from_env().expect("failed to load configuration"));

    let token = config.discord_token.clone();

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(handler::Handler::new(Arc::clone(&config)))
        .await
        .expect("Err creating client");

    let http = client.http.clone();
    let channel_id = ChannelId::new(config.discord_release_channel_id);

    let (release_tx, release_rx) = unbounded::<String>();

    worker::spawn(release_rx, http, channel_id);

    let state = Arc::new(AppState { release_tx, config });
    let _ = tokio::spawn(api::serve(state));

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
