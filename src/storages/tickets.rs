use crate::db;
use crate::models::ticket::Ticket;

#[derive(Clone)]
pub struct TicketStore {
    db: db::Db,
}

impl TicketStore {
    pub fn new(db: db::Db) -> Self {
        Self { db }
    }

    /// Opens a standalone store over its own SQLite file. Only used by the
    /// unit tests below; production shares one `db::Db` via `TicketStore::new`.
    pub async fn open(path: &str) -> Result<Self, sqlx::Error> {
        Ok(Self::new(db::Db::open(path).await?))
    }

    /// Opens a standalone store over a throwaway SQLite file in a temp dir.
    /// Only used by the unit tests below; production shares one `db::Db`.
    pub async fn open_in_memory() -> Result<Self, sqlx::Error> {
        Ok(Self::new(db::Db::open_in_memory().await?))
    }

    pub async fn create_ticket(
        &self,
        guild_id: &str,
        user_id: &str,
        username: &str,
        subject: &str,
        description: &str,
    ) -> Result<Ticket, sqlx::Error> {
        let now = now_secs();
        let result = sqlx::query(
            "INSERT INTO tickets (guild_id, user_id, username, subject, description, opened_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        )
        .bind(guild_id)
        .bind(user_id)
        .bind(username)
        .bind(subject)
        .bind(description)
        .bind(now)
        .execute(self.db.pool())
        .await?;
        self.get_ticket(result.last_insert_rowid())
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn get_ticket(&self, id: i64) -> Result<Option<Ticket>, sqlx::Error> {
        sqlx::query_as::<_, Ticket>(
            "SELECT id, guild_id, channel_id, user_id, username, subject, description, status, \
             opened_at, updated_at, closed_at, closed_by FROM tickets WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
    }

    pub async fn list_tickets(&self) -> Result<Vec<Ticket>, sqlx::Error> {
        sqlx::query_as::<_, Ticket>(
            "SELECT id, guild_id, channel_id, user_id, username, subject, description, status, \
             opened_at, updated_at, closed_at, closed_by FROM tickets \
             ORDER BY CASE status WHEN 'open' THEN 0 WHEN 'in_progress' THEN 1 ELSE 2 END, \
             opened_at DESC",
        )
        .fetch_all(self.db.pool())
        .await
    }

    pub async fn set_channel(&self, id: i64, channel_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE tickets SET channel_id = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(channel_id)
            .bind(now_secs())
            .bind(id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    pub async fn close_ticket(
        &self,
        id: i64,
        closed_by: &str,
    ) -> Result<Option<Ticket>, sqlx::Error> {
        sqlx::query(
            "UPDATE tickets
             SET status = 'closed', closed_at = ?1, closed_by = ?2, updated_at = ?1
             WHERE id = ?3 AND status != 'closed'",
        )
        .bind(now_secs())
        .bind(closed_by)
        .bind(id)
        .execute(self.db.pool())
        .await?;
        self.get_ticket(id).await
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    #[test]
    fn timestamps_are_utc() {
        let now = Utc::now();
        let ts = now.timestamp();
        let roundtrip = DateTime::<Utc>::from_timestamp(ts, 0).unwrap();
        assert_eq!(roundtrip.timestamp(), ts);
        assert_eq!(roundtrip.timezone(), Utc);
        assert_eq!(roundtrip.format("%Y-%m-%d %H:%M:%S").to_string().len(), 19);
    }

    #[tokio::test]
    async fn create_and_fetch_ticket() {
        let store = TicketStore::open_in_memory().await.unwrap();
        let ticket = store
            .create_ticket("guild", "u1", "mark", "Broken thing", "It broke")
            .await
            .unwrap();
        assert_eq!(ticket.id, 1);
        assert_eq!(ticket.status, "open");
        assert!(ticket.channel_id.is_none());
        let age = (Utc::now() - ticket.opened_at).num_seconds();
        assert!(
            (0..5).contains(&age),
            "opened_at should be recent, got {age}s old"
        );
        let fetched = store.get_ticket(ticket.id).await.unwrap().unwrap();
        assert_eq!(fetched.subject, "Broken thing");
        assert_eq!(fetched.username, "mark");
        assert_eq!(fetched.opened_at, ticket.opened_at);
    }

    #[tokio::test]
    async fn list_orders_open_first() {
        let store = TicketStore::open_in_memory().await.unwrap();
        let open = store
            .create_ticket("g", "u1", "a", "Open one", "")
            .await
            .unwrap();
        store
            .create_ticket("g", "u2", "b", "Closed one", "")
            .await
            .unwrap();
        store.close_ticket(2, "staff").await.unwrap();
        let tickets = store.list_tickets().await.unwrap();
        assert_eq!(tickets.len(), 2);
        assert_eq!(tickets[0].id, open.id);
        assert_eq!(tickets[0].status, "open");
        assert_eq!(tickets[1].status, "closed");
    }

    #[tokio::test]
    async fn close_ticket_sets_fields_once() {
        let store = TicketStore::open_in_memory().await.unwrap();
        let ticket = store
            .create_ticket("g", "u1", "a", "Close me", "details")
            .await
            .unwrap();
        let closed = store
            .close_ticket(ticket.id, "staff")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(closed.status, "closed");
        assert_eq!(closed.closed_by.as_deref(), Some("staff"));
        assert!(closed.closed_at.is_some());

        let again = store
            .close_ticket(ticket.id, "staff2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(again.closed_by.as_deref(), Some("staff"));
    }

    #[tokio::test]
    async fn set_channel_persists() {
        let store = TicketStore::open_in_memory().await.unwrap();
        let ticket = store
            .create_ticket("g", "u1", "a", "Thread", "")
            .await
            .unwrap();
        store.set_channel(ticket.id, "123456").await.unwrap();
        let fetched = store.get_ticket(ticket.id).await.unwrap().unwrap();
        assert_eq!(fetched.channel_id.as_deref(), Some("123456"));
    }

    #[tokio::test]
    async fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tickets.sqlite");
        let path_str = path.to_str().unwrap();
        let id = {
            let store = TicketStore::open(path_str).await.unwrap();
            store
                .create_ticket("g", "u1", "a", "Persist", "")
                .await
                .unwrap()
                .id
        };
        let store = TicketStore::open(path_str).await.unwrap();
        let ticket = store.get_ticket(id).await.unwrap().unwrap();
        assert_eq!(ticket.subject, "Persist");
    }
}
