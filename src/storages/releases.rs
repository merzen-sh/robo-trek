use crate::db;

#[derive(Clone)]
pub struct ReleaseStore {
    db: db::Db,
}

impl ReleaseStore {
    pub fn new(db: db::Db) -> Self {
        Self { db }
    }

    /// Opens a standalone store over its own SQLite file. Only used by the
    /// unit tests below; production shares one `db::Db` via `ReleaseStore::new`.
    pub async fn open(path: &str) -> Result<Self, sqlx::Error> {
        Ok(Self::new(db::Db::open(path).await?))
    }

    /// Opens a standalone store over a throwaway SQLite file in a temp dir.
    /// Only used by the unit tests below; production shares one `db::Db`.
    pub async fn open_in_memory() -> Result<Self, sqlx::Error> {
        Ok(Self::new(db::Db::open_in_memory().await?))
    }

    pub async fn put_release(&self, version: &str, png: &[u8]) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO releases (version, png) VALUES (?1, ?2)
             ON CONFLICT(version) DO UPDATE SET png = excluded.png",
        )
        .bind(version)
        .bind(png)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn get_release(&self, version: &str) -> Result<Option<Vec<u8>>, sqlx::Error> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as("SELECT png FROM releases WHERE version = ?1")
            .bind(version)
            .fetch_optional(self.db.pool())
            .await?;
        Ok(row.map(|(png,)| png))
    }

    pub async fn list_releases(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT version FROM releases ORDER BY version")
            .fetch_all(self.db.pool())
            .await?;
        Ok(rows.into_iter().map(|(version,)| version).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_get_roundtrip() {
        let store = ReleaseStore::open_in_memory().await.unwrap();
        store.put_release("v1.0.0", b"release-png").await.unwrap();
        assert_eq!(
            store.get_release("v1.0.0").await.unwrap(),
            Some(b"release-png".to_vec())
        );
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let store = ReleaseStore::open_in_memory().await.unwrap();
        assert_eq!(store.get_release("v9.9.9").await.unwrap(), None);
    }

    #[tokio::test]
    async fn list_releases_sorts_versions() {
        let store = ReleaseStore::open_in_memory().await.unwrap();
        store.put_release("v2.0.0", b"b").await.unwrap();
        store.put_release("v1.0.0", b"a").await.unwrap();
        let versions = store.list_releases().await.unwrap();
        assert_eq!(versions, vec!["v1.0.0".to_string(), "v2.0.0".to_string()]);
    }

    #[tokio::test]
    async fn put_release_overwrites_existing() {
        let store = ReleaseStore::open_in_memory().await.unwrap();
        store.put_release("v1.0.0", b"old").await.unwrap();
        store.put_release("v1.0.0", b"new").await.unwrap();
        assert_eq!(
            store.get_release("v1.0.0").await.unwrap(),
            Some(b"new".to_vec())
        );
        assert_eq!(store.list_releases().await.unwrap(), vec!["v1.0.0"]);
    }

    #[tokio::test]
    async fn releases_persist_across_reopen() {
        let path = format!(
            "/tmp/robo-trek-releases-reopen-{}.sqlite",
            std::process::id()
        );
        let _ = std::fs::remove_file(&path);
        {
            let store = ReleaseStore::open(&path).await.unwrap();
            store.put_release("v1.0.0", b"png").await.unwrap();
        }
        let store = ReleaseStore::open(&path).await.unwrap();
        assert_eq!(
            store.get_release("v1.0.0").await.unwrap(),
            Some(b"png".to_vec())
        );
        let _ = std::fs::remove_file(&path);
    }
}
