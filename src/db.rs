use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tempfile::TempDir;

/// Shared SQLite connection pool.
#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
    _temp_dir: Option<Arc<TempDir>>,
}

impl Db {
    pub async fn open(path: &str) -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        Self::with_options(opts, None).await
    }

    pub async fn open_in_memory() -> Result<Self, sqlx::Error> {
        let dir = TempDir::new()?;
        let path = dir.path().join("db.sqlite");
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        Self::with_options(opts, Some(Arc::new(dir))).await
    }

    async fn with_options(
        opts: SqliteConnectOptions,
        temp_dir: Option<Arc<TempDir>>,
    ) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self {
            pool,
            _temp_dir: temp_dir,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
