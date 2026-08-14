use serenity::builder::CreateCommand;

pub fn register() -> CreateCommand {
    CreateCommand::new("ticket").description("Open a support ticket")
}
