use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Ticket {
    pub id: i64,
    pub guild_id: String,
    pub channel_id: Option<String>,
    pub user_id: String,
    pub username: String,
    pub subject: String,
    pub description: String,
    pub status: String,
    pub opened_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub closed_by: Option<String>,
}
