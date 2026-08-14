use std::sync::Arc;

pub mod commands;
pub mod tickets;

use crate::config::Config;
use crate::storages::tickets::TicketStore;
use serenity::async_trait;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::gateway::ActivityData;
use serenity::model::application::{ComponentInteraction, Interaction, ModalInteraction};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::user::OnlineStatus;
use serenity::prelude::*;

pub struct Handler {
    config: Arc<Config>,
    tickets: TicketStore,
}

impl Handler {
    pub fn new(config: Arc<Config>, tickets: TicketStore) -> Self {
        Self { config, tickets }
    }

    async fn handle_command(
        &self,
        ctx: &Context,
        command: serenity::model::application::CommandInteraction,
    ) {
        let response = match command.data.name.as_str() {
            "ping" => CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(commands::ping::run(&command.data.options())),
            ),
            "ticket" => CreateInteractionResponse::Modal(tickets::open_modal()),
            _ => CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content("not implemented :("),
            ),
        };

        if let Err(why) = command.create_response(&ctx.http, response).await {
            println!("Cannot respond to slash command: {why}");
        }
    }

    async fn handle_modal(&self, ctx: &Context, modal: ModalInteraction) {
        if modal.data.custom_id == tickets::MODAL_ID {
            tickets::handle_modal(ctx, modal, &self.config, &self.tickets).await;
        }
    }

    async fn handle_component(&self, ctx: &Context, component: ComponentInteraction) {
        let Some((prefix, _)) = component.data.custom_id.split_once(':') else {
            return;
        };
        if prefix == tickets::CLOSE_PREFIX {
            tickets::handle_close(ctx, component, &self.tickets).await;
        }
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
            .set_commands(
                &ctx.http,
                vec![commands::ping::register(), commands::ticket::register()],
            )
            .await;
    }

    async fn message(&self, ctx: Context, msg: serenity::model::channel::Message) {
        if msg.content == "!ping"
            && let Err(why) = msg.channel_id.say(&ctx.http, "Pong!").await
        {
            println!("Error sending message: {why:?}");
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => self.handle_command(&ctx, command).await,
            Interaction::Modal(modal) => self.handle_modal(&ctx, modal).await,
            Interaction::Component(component) => self.handle_component(&ctx, component).await,
            _ => {}
        }
    }
}
