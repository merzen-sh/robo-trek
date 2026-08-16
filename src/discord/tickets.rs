use crate::config::Config;
use crate::storages::tickets::TicketStore;
use serenity::builder::{
    CreateActionRow, CreateButton, CreateInputText, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateMessage,
    CreateModal, CreateThread, EditInteractionResponse, EditThread,
};
use serenity::model::application::{
    ActionRowComponent, ButtonStyle, ComponentInteraction, InputTextStyle, ModalInteraction,
};
use serenity::model::channel::AutoArchiveDuration;
use serenity::model::id::ChannelId;
use serenity::prelude::*;
use tracing::error;

pub const MODAL_ID: &str = "ticket_modal";
pub const CLOSE_PREFIX: &str = "close_ticket";

pub fn open_modal() -> CreateModal {
    CreateModal::new(MODAL_ID, "Open a ticket").components(vec![
        CreateActionRow::InputText(
            CreateInputText::new(InputTextStyle::Short, "Subject", "subject")
                .placeholder("Short summary of the issue")
                .max_length(100),
        ),
        CreateActionRow::InputText(
            CreateInputText::new(InputTextStyle::Paragraph, "Description", "description")
                .placeholder("Describe the issue in detail")
                .max_length(1000),
        ),
    ])
}

pub async fn handle_modal(
    ctx: &Context,
    modal: ModalInteraction,
    config: &Config,
    tickets: &TicketStore,
) {
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

    let ticket = match tickets
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
            error!("failed to create ticket: {why}");
            let content = CreateInteractionResponseFollowup::new()
                .content("Failed to create the ticket. Try again later.");
            let _ = modal.create_followup(&ctx.http, content).await;
            return;
        }
    };

    let channel = ChannelId::new(config.discord_tickets_channel_id);
    let thread_name = thread_name(ticket.id, &ticket.subject);
    match channel
        .create_thread(
            &ctx.http,
            CreateThread::new(thread_name).auto_archive_duration(AutoArchiveDuration::ThreeDays),
        )
        .await
    {
        Ok(thread) => {
            let _ = tickets.set_channel(ticket.id, &thread.id.to_string()).await;

            let button = CreateButton::new(format!("{CLOSE_PREFIX}:{}", ticket.id))
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
            error!("failed to create ticket thread: {why}");
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

pub async fn handle_close(ctx: &Context, component: ComponentInteraction, tickets: &TicketStore) {
    let Some(id) = component
        .data
        .custom_id
        .strip_prefix(CLOSE_PREFIX)
        .and_then(|rest| rest.strip_prefix(':'))
        .and_then(|id| id.parse::<i64>().ok())
    else {
        return;
    };
    let closed_by = component.user.name.clone();

    let acknowledged = component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await
        .is_ok();

    let content = match tickets.close_ticket(id, &closed_by).await {
        Ok(Some(_)) => {
            let _ = component
                .channel_id
                .edit_thread(&ctx.http, EditThread::new().archived(true).locked(true))
                .await;
            format!("Ticket #{id} closed by {closed_by}.")
        }
        Ok(None) => format!("Ticket #{id} is already closed."),
        Err(why) => {
            error!("failed to close ticket {id}: {why}");
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
