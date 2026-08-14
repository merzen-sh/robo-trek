use std::sync::Arc;

use crate::config::Config;
use serenity::all::Message;
use serenity::async_trait;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::gateway::ActivityData;
use serenity::model::application::Interaction;
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::user::OnlineStatus;
use serenity::prelude::*;

use crate::commands;

pub struct Handler {
    config: Arc<Config>,
}

impl Handler {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        ctx.set_presence(
            Some(ActivityData::playing("Robo Trek")),
            OnlineStatus::Online,
        );

        let guild_id = GuildId::new(self.config.guild_id);

        let _ = guild_id
            .set_commands(&ctx.http, vec![commands::ping::register()])
            .await;
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!ping"
            && let Err(why) = msg.channel_id.say(&ctx.http, "Pong!").await
        {
            println!("Error sending message: {why:?}");
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            let content = match command.data.name.as_str() {
                "ping" => Some(commands::ping::run(&command.data.options())),
                _ => Some("not implemented :(".to_string()),
            };

            if let Some(content) = content {
                let data = CreateInteractionResponseMessage::new().content(content);
                let builder = CreateInteractionResponse::Message(data);
                if let Err(why) = command.create_response(&ctx.http, builder).await {
                    println!("Cannot respond to slash command: {why}");
                }
            }
        }
    }
}
