use std::{env, sync::Arc};

use crossbeam_channel::unbounded;
use serenity::{model::id::ChannelId, prelude::*};

mod api;
mod commands;
mod handler;
mod render;
mod worker;

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(handler::Handler)
        .await
        .expect("Err creating client");

    let http = client.http.clone();
    let channel_id = ChannelId::new(
        env::var("DISCORD_RELEASE_CHANNEL_ID")
            .expect("Expected DISCORD_RELEASE_CHANNEL_ID in environment")
            .parse()
            .expect("DISCORD_RELEASE_CHANNEL_ID must be an integer"),
    );

    let (release_tx, release_rx) = unbounded::<String>();

    worker::spawn(release_rx, http, channel_id);

    let state = Arc::new(api::AppState { release_tx });
    let _api_handle = tokio::spawn(api::serve(state));

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
