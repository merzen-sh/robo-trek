use std::sync::Arc;

use crate::commands;
use crate::config::Config;
use crate::tickets;
use serenity::async_trait;
use serenity::builder::{
    CreateActionRow, CreateButton, CreateInputText, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateMessage,
    CreateModal, CreateThread, EditInteractionResponse, EditThread,
};
use serenity::gateway::ActivityData;
use serenity::model::application::{
    ActionRowComponent, ButtonStyle, ComponentInteraction, InputTextStyle, Interaction,
    ModalInteraction,
};
use serenity::model::channel::AutoArchiveDuration;
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId};
use serenity::model::user::OnlineStatus;
use serenity::prelude::*;

pub struct Handler {
    config: Arc<Config>,
    tickets: tickets::TicketStore,
}

impl Handler {
    pub fn new(config: Arc<Config>, tickets: tickets::TicketStore) -> Self {
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
            "ticket" => {
                let modal = CreateModal::new("ticket_modal", "Open a ticket").components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "Subject", "subject")
                            .placeholder("Short summary of the issue")
                            .max_length(100),
                    ),
                    CreateActionRow::InputText(
                        CreateInputText::new(
                            InputTextStyle::Paragraph,
                            "Description",
                            "description",
                        )
                        .placeholder("Describe the issue in detail")
                        .max_length(1000),
                    ),
                ]);
                CreateInteractionResponse::Modal(modal)
            }
            _ => CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content("not implemented :("),
            ),
        };

        if let Err(why) = command.create_response(&ctx.http, response).await {
            println!("Cannot respond to slash command: {why}");
        }
    }

    async fn handle_modal(&self, ctx: &Context, modal: ModalInteraction) {
        if modal.data.custom_id != "ticket_modal" {
            return;
        }

        let subject = modal_value(&modal, "subject").unwrap_or_default();
        let description = modal_value(&modal, "description").unwrap_or_default();
        let Some(guild_id) = modal.guild_id else {
            return;
        };
        let user = modal.user.clone();

        // Acknowledge quickly so the flow can survive slow thread creation.
        let _ = modal
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()),
            )
            .await;

        let ticket = match self
            .tickets
            .create_ticket(
                &guild_id.to_string(),
                &user.id.to_string(),
                &user.name,
                &subject,
                &description,
            )
            .await
        {
            Ok(ticket) => ticket,
            Err(why) => {
                println!("failed to create ticket: {why}");
                let content = CreateInteractionResponseFollowup::new()
                    .content("Failed to create the ticket. Try again later.");
                let _ = modal.create_followup(&ctx.http, content).await;
                return;
            }
        };

        let channel = ChannelId::new(self.config.discord_tickets_channel_id);
        let thread_name = thread_name(ticket.id, &ticket.subject);
        match channel
            .create_thread(
                &ctx.http,
                CreateThread::new(thread_name)
                    .auto_archive_duration(AutoArchiveDuration::ThreeDays),
            )
            .await
        {
            Ok(thread) => {
                let _ = self
                    .tickets
                    .set_channel(ticket.id, &thread.id.to_string())
                    .await;

                let button = CreateButton::new(format!("close_ticket:{}", ticket.id))
                    .label("Close ticket")
                    .style(ButtonStyle::Danger);
                let message = CreateMessage::new()
                    .content(format!(
                        "Ticket **#{ticket_id}** by **{username}**:\n\n**{subject}**\n\n{description}",
                        ticket_id = ticket.id,
                        username = user.name
                    ))
                    .components(vec![CreateActionRow::Buttons(vec![button])]);
                let _ = thread.id.send_message(&ctx.http, message).await;

                let content = CreateInteractionResponseFollowup::new().content(format!(
                    "Ticket **#{ticket_id}** opened! <#{thread_id}>",
                    ticket_id = ticket.id,
                    thread_id = thread.id.get()
                ));
                let _ = modal.create_followup(&ctx.http, content).await;
            }
            Err(why) => {
                println!("failed to create ticket thread: {why}");
                let content = CreateInteractionResponseFollowup::new()
                    .content(format!(
                        "Ticket **#{ticket_id}** created, but the thread could not be opened: {why}",
                        ticket_id = ticket.id
                    ))
                    .ephemeral(true);
                let _ = modal.create_followup(&ctx.http, content).await;
            }
        }
    }

    async fn handle_component(&self, ctx: &Context, component: ComponentInteraction) {
        let Some((prefix, id_str)) = component.data.custom_id.split_once(':') else {
            return;
        };
        if prefix != "close_ticket" {
            return;
        }
        let Ok(id) = id_str.parse::<i64>() else {
            return;
        };
        let closed_by = component.user.name.clone();

        let acknowledged = component
            .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
            .await
            .is_ok();

        let outcome = self.tickets.close_ticket(id, &closed_by).await;
        let content = match &outcome {
            Ok(Some(_)) => {
                let _ = component
                    .channel_id
                    .edit_thread(&ctx.http, EditThread::new().archived(true).locked(true))
                    .await;
                format!("Ticket #{id} closed by {closed_by}.")
            }
            Ok(None) => format!("Ticket #{id} is already closed."),
            Err(why) => {
                println!("failed to close ticket {id}: {why}");
                format!("Failed to close ticket #{id}. Try again later.")
            }
        };

        if !acknowledged {
            return;
        }
        let edit = EditInteractionResponse::new()
            .content(content)
            .components(vec![]);
        let _ = component.edit_response(&ctx.http, edit).await;
    }
}

fn modal_value(modal: &ModalInteraction, custom_id: &str) -> Option<String> {
    modal
        .data
        .components
        .iter()
        .flat_map(|row| row.components.iter())
        .find_map(|c| match c {
            ActionRowComponent::InputText(input) if input.custom_id == custom_id => {
                input.value.clone()
            }
            _ => None,
        })
}

fn thread_name(id: i64, subject: &str) -> String {
    let base = format!("ticket-{id}-{subject}");
    let mut name: String = base.chars().take(90).collect();
    if name.chars().count() < base.chars().count() {
        name.push('…');
    }
    name
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
