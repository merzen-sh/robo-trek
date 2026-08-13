use std::{path::Path, sync::Arc};

use redb::{Database, ReadableDatabase, TableDefinition};

use tokio::task;

const RELEASES: TableDefinition<&str, &[u8]> = TableDefinition::new("releases");

#[derive(Clone)]
pub struct Db {
    inner: Arc<Database>,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let db = Database::create(path).map_err(|e| e.to_string())?;
        let tx = db.begin_write().map_err(|e| e.to_string())?;
        {
            let _ = tx.open_table(RELEASES).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Self {
            inner: Arc::new(db),
        })
    }

    pub async fn put_release(&self, version: &str, png: &[u8]) -> Result<(), String> {
        let db = self.inner.clone();
        let version = version.to_string();
        let png = png.to_vec();
        task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = tx.open_table(RELEASES).map_err(|e| e.to_string())?;
                table
                    .insert(version.as_str(), png.as_slice())
                    .map_err(|e| e.to_string())?;
            }
            tx.commit().map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| format!("db task failed: {e}"))?
    }

    pub async fn get_release(&self, version: &str) -> Result<Option<Vec<u8>>, String> {
        let db = self.inner.clone();
        let version = version.to_string();
        task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(|e| e.to_string())?;
            let table = tx.open_table(RELEASES).map_err(|e| e.to_string())?;
            Ok(table
                .get(version.as_str())
                .map_err(|e| e.to_string())?
                .map(|v| v.value().to_vec()))
        })
        .await
        .map_err(|e| format!("db task failed: {e}"))?
    }
}
