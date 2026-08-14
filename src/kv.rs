use std::{path::Path, sync::Arc};

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use tokio::task;

const RELEASES: TableDefinition<&str, &[u8]> = TableDefinition::new("releases");
const METRICS: TableDefinition<u64, &[u8]> = TableDefinition::new("metrics");

#[derive(Clone)]
pub struct Kv {
    inner: Arc<Database>,
}

impl Kv {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let database = Database::create(path).map_err(|e| e.to_string())?;
        let tx = database.begin_write().map_err(|e| e.to_string())?;
        {
            let _ = tx.open_table(RELEASES).map_err(|e| e.to_string())?;
            let _ = tx.open_table(METRICS).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Self {
            inner: Arc::new(database),
        })
    }

    pub async fn put_release(&self, version: &str, png: &[u8]) -> Result<(), String> {
        let database = self.inner.clone();
        let version = version.to_string();
        let png = png.to_vec();
        task::spawn_blocking(move || {
            let tx = database.begin_write().map_err(|e| e.to_string())?;
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
        .map_err(|e| format!("kv task failed: {e}"))?
    }

    pub async fn get_release(&self, version: &str) -> Result<Option<Vec<u8>>, String> {
        let database = self.inner.clone();
        let version = version.to_string();
        task::spawn_blocking(move || {
            let tx = database.begin_read().map_err(|e| e.to_string())?;
            let table = tx.open_table(RELEASES).map_err(|e| e.to_string())?;
            Ok(table
                .get(version.as_str())
                .map_err(|e| e.to_string())?
                .map(|v| v.value().to_vec()))
        })
        .await
        .map_err(|e| format!("kv task failed: {e}"))?
    }

    pub async fn list_releases(&self) -> Result<Vec<String>, String> {
        let database = self.inner.clone();
        task::spawn_blocking(move || {
            let tx = database.begin_read().map_err(|e| e.to_string())?;
            let table = tx.open_table(RELEASES).map_err(|e| e.to_string())?;
            let mut versions = Vec::new();
            for entry in table.iter().map_err(|e| e.to_string())? {
                let (key, _) = entry.map_err(|e| e.to_string())?;
                versions.push(key.value().to_string());
            }
            Ok(versions)
        })
        .await
        .map_err(|e| format!("kv task failed: {e}"))?
    }

    pub async fn put_metrics(&self, ts: u64, cpu: f64, mem: f64) -> Result<(), String> {
        let database = self.inner.clone();
        task::spawn_blocking(move || {
            let mut buf = [0u8; 16];
            buf[..8].copy_from_slice(&cpu.to_le_bytes());
            buf[8..].copy_from_slice(&mem.to_le_bytes());
            let tx = database.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = tx.open_table(METRICS).map_err(|e| e.to_string())?;
                table
                    .insert(ts, buf.as_slice())
                    .map_err(|e| e.to_string())?;
            }
            tx.commit().map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| format!("kv task failed: {e}"))?
    }

    pub async fn get_metrics_since(&self, since: u64) -> Result<Vec<(u64, f64, f64)>, String> {
        let database = self.inner.clone();
        task::spawn_blocking(move || {
            let tx = database.begin_read().map_err(|e| e.to_string())?;
            let table = tx.open_table(METRICS).map_err(|e| e.to_string())?;
            let mut rows = Vec::new();
            for entry in table.range(since..).map_err(|e| e.to_string())? {
                let (key, value) = entry.map_err(|e| e.to_string())?;
                let bytes = value.value();
                let mut cpu_bytes = [0u8; 8];
                let mut mem_bytes = [0u8; 8];
                cpu_bytes.copy_from_slice(&bytes[..8]);
                mem_bytes.copy_from_slice(&bytes[8..16]);
                rows.push((
                    key.value(),
                    f64::from_le_bytes(cpu_bytes),
                    f64::from_le_bytes(mem_bytes),
                ));
            }
            Ok(rows)
        })
        .await
        .map_err(|e| format!("kv task failed: {e}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_tmp(name: &str) -> Kv {
        let path = format!("/tmp/robo-trek-kv-{}-{name}.redb", std::process::id());
        let _ = std::fs::remove_file(&path);
        Kv::open(path).unwrap()
    }

    #[tokio::test]
    async fn put_get_roundtrip() {
        let kv = open_tmp("roundtrip");
        kv.put_release("v1.0.0", b"release-png").await.unwrap();
        assert_eq!(
            kv.get_release("v1.0.0").await.unwrap(),
            Some(b"release-png".to_vec())
        );
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let kv = open_tmp("missing");
        assert_eq!(kv.get_release("v9.9.9").await.unwrap(), None);
    }

    #[tokio::test]
    async fn list_releases_sorts_versions() {
        let kv = open_tmp("list");
        kv.put_release("v2.0.0", b"b").await.unwrap();
        kv.put_release("v1.0.0", b"a").await.unwrap();
        let mut versions = kv.list_releases().await.unwrap();
        versions.sort();
        assert_eq!(versions, vec!["v1.0.0".to_string(), "v2.0.0".to_string()]);
    }

    #[tokio::test]
    async fn metrics_persist_across_reopen() {
        let path = format!("/tmp/robo-trek-kv-reopen-{}.redb", std::process::id());
        let _ = std::fs::remove_file(&path);
        {
            let kv = Kv::open(&path).unwrap();
            kv.put_metrics(100, 1.5, 2.5).await.unwrap();
        }
        let kv = Kv::open(&path).unwrap();
        let rows = kv.get_metrics_since(0).await.unwrap();
        assert_eq!(rows, vec![(100, 1.5, 2.5)]);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn metrics_roundtrip_and_range() {
        let kv = open_tmp("metrics");
        kv.put_metrics(100, 10.5, 20.5).await.unwrap();
        kv.put_metrics(200, 30.5, 40.5).await.unwrap();
        kv.put_metrics(300, 50.5, 60.5).await.unwrap();

        let rows = kv.get_metrics_since(150).await.unwrap();
        assert_eq!(rows, vec![(200, 30.5, 40.5), (300, 50.5, 60.5)]);
        assert!(kv.get_metrics_since(0).await.unwrap().len() == 3);
    }
}
