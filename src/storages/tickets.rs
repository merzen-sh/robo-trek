use crate::db;
use crate::models::ticket::Ticket;

#[derive(Default, Clone)]
pub struct TicketFilters<'a> {
    pub status: Option<&'a str>,
    pub search: Option<&'a str>,
}

#[derive(Clone)]
pub struct TicketStore {
    db: db::Db,
}

impl TicketStore {
    pub fn new(db: db::Db) -> Self {
        Self { db }
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
             opened_at DESC, id DESC",
        )
        .fetch_all(self.db.pool())
        .await
    }

    pub const PAGE_SIZE: i64 = 10;

    fn push_filters(builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>, filters: &TicketFilters<'_>) {
        let mut clause = false;
        if let Some(status) = &filters.status {
            builder.push(" WHERE status = ");
            builder.push_bind(status);
            clause = true;
        }
        if let Some(q) = &filters.search {
            builder.push(if clause { " AND (" } else { " WHERE (" });
            builder.push("subject LIKE ");
            builder.push_bind(format!("%{q}%"));
            builder.push(" OR username LIKE ");
            builder.push_bind(format!("%{q}%"));
            builder.push(" OR id = ");
            builder.push_bind(q.parse::<i64>().unwrap_or(-1));
            builder.push(")");
        }
    }

    pub async fn count_tickets(&self, filters: &TicketFilters<'_>) -> Result<i64, sqlx::Error> {
        let mut builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tickets");
        Self::push_filters(&mut builder, filters);
        builder.build_query_scalar().fetch_one(self.db.pool()).await
    }

    pub async fn list_tickets_page(
        &self,
        page: i64,
        filters: &TicketFilters<'_>,
    ) -> Result<Vec<Ticket>, sqlx::Error> {
        let page = page.max(1);
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, guild_id, channel_id, user_id, username, subject, description, status, \
             opened_at, updated_at, closed_at, closed_by FROM tickets",
        );
        Self::push_filters(&mut builder, filters);
        builder.push(
            " ORDER BY CASE status WHEN 'open' THEN 0 WHEN 'in_progress' THEN 1 ELSE 2 END, \
             opened_at DESC, id DESC LIMIT ",
        );
        builder.push_bind(Self::PAGE_SIZE);
        builder.push(" OFFSET ");
        builder.push_bind((page - 1) * Self::PAGE_SIZE);
        builder
            .build_query_as::<Ticket>()
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
        let store = TicketStore::new(db::Db::open_in_memory().await.unwrap());
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
        let store = TicketStore::new(db::Db::open_in_memory().await.unwrap());
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
    async fn paged_list_slices_by_page_size() {
        let store = TicketStore::new(db::Db::open_in_memory().await.unwrap());
        let filters = TicketFilters::default();
        for i in 0..25 {
            store
                .create_ticket("g", "u1", "mark", &format!("Ticket {i}"), "")
                .await
                .unwrap();
        }
        assert_eq!(store.count_tickets(&filters).await.unwrap(), 25);
        assert_eq!(
            store.list_tickets_page(1, &filters).await.unwrap().len(),
            10
        );
        assert_eq!(
            store.list_tickets_page(2, &filters).await.unwrap().len(),
            10
        );
        let last = store.list_tickets_page(3, &filters).await.unwrap();
        assert_eq!(last.len(), 5);
        assert_eq!(last[0].id, 5);
        assert!(
            store
                .list_tickets_page(4, &filters)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn filters_status_and_search() {
        let store = TicketStore::new(db::Db::open_in_memory().await.unwrap());
        store
            .create_ticket("g", "u1", "mark", "Server down", "outage")
            .await
            .unwrap();
        store
            .create_ticket("g", "u2", "jane", "UI bug", "styling")
            .await
            .unwrap();
        let third = store
            .create_ticket("g", "u3", "bob", "Server down", "again")
            .await
            .unwrap();
        store.close_ticket(third.id, "staff").await.unwrap();

        let open = TicketFilters {
            status: Some("open"),
            search: None,
        };
        assert_eq!(store.count_tickets(&open).await.unwrap(), 2);
        assert!(
            store
                .list_tickets_page(1, &open)
                .await
                .unwrap()
                .iter()
                .all(|t| t.status == "open")
        );

        let subject = TicketFilters {
            status: None,
            search: Some("Server"),
        };
        assert_eq!(store.count_tickets(&subject).await.unwrap(), 2);

        let username = TicketFilters {
            status: None,
            search: Some("jane"),
        };
        let hits = store.list_tickets_page(1, &username).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].username, "jane");

        let by_id_str = third.id.to_string();
        let by_id = TicketFilters {
            status: None,
            search: Some(&by_id_str),
        };
        assert_eq!(store.count_tickets(&by_id).await.unwrap(), 1);

        let combined = TicketFilters {
            status: Some("open"),
            search: Some("Server"),
        };
        assert_eq!(store.count_tickets(&combined).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn close_ticket_sets_fields_once() {
        let store = TicketStore::new(db::Db::open_in_memory().await.unwrap());
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
        let store = TicketStore::new(db::Db::open_in_memory().await.unwrap());
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
            let store = TicketStore::new(db::Db::open(path_str).await.unwrap());
            store
                .create_ticket("g", "u1", "a", "Persist", "")
                .await
                .unwrap()
                .id
        };
        let store = TicketStore::new(db::Db::open(path_str).await.unwrap());
        let ticket = store.get_ticket(id).await.unwrap().unwrap();
        assert_eq!(ticket.subject, "Persist");
    }
}
