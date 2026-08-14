use std::{path::Path, sync::Arc};

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

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

    pub async fn list_releases(&self) -> Result<Vec<String>, String> {
        let db = self.inner.clone();
        task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(|e| e.to_string())?;
            let table = tx.open_table(RELEASES).map_err(|e| e.to_string())?;
            let mut versions = Vec::new();
            for entry in table.iter().map_err(|e| e.to_string())? {
                let (key, _) = entry.map_err(|e| e.to_string())?;
                versions.push(key.value().to_string());
            }
            Ok(versions)
        })
        .await
        .map_err(|e| format!("db task failed: {e}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_tmp(name: &str) -> Db {
        let path = format!("/tmp/robo-trek-db-{}-{name}.redb", std::process::id());
        let _ = std::fs::remove_file(&path);
        Db::open(path).unwrap()
    }

    #[tokio::test]
    async fn put_get_roundtrip() {
        let db = open_tmp("roundtrip");
        db.put_release("v1.0.0", b"release-png").await.unwrap();
        assert_eq!(
            db.get_release("v1.0.0").await.unwrap(),
            Some(b"release-png".to_vec())
        );
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let db = open_tmp("missing");
        assert_eq!(db.get_release("v9.9.9").await.unwrap(), None);
    }

    #[tokio::test]
    async fn list_releases_sorts_versions() {
        let db = open_tmp("list");
        db.put_release("v2.0.0", b"b").await.unwrap();
        db.put_release("v1.0.0", b"a").await.unwrap();
        let mut versions = db.list_releases().await.unwrap();
        versions.sort();
        assert_eq!(versions, vec!["v1.0.0".to_string(), "v2.0.0".to_string()]);
    }
}
