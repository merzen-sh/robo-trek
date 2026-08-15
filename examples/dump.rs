use std::env;

use redb::{
    Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition, TableHandle,
};

const METRICS: TableDefinition<u64, &[u8]> = TableDefinition::new("metrics");

fn main() -> Result<(), String> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "robo-trek.redb".to_string());

    let db = Database::open(&path).map_err(|e| format!("open {}: {e}", path))?;

    let tx = db.begin_read().map_err(|e| e.to_string())?;

    println!("tables:");
    for table in tx.list_tables().map_err(|e| e.to_string())? {
        println!("  {}", table.name());
    }

    let table = tx.open_table(METRICS).map_err(|e| e.to_string())?;

    println!(
        "\nmetrics ({} entries):",
        table.len().map_err(|e| e.to_string())?
    );
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (key, val) = entry.map_err(|e| e.to_string())?;
        println!("  {}: {} bytes", key.value(), val.value().len());
    }

    Ok(())
}
