use crate::error::Result;
use crate::postgres::pool::PostgresDriver;

pub async fn execute_explain(driver: &PostgresDriver, sql: &str) -> Result<serde_json::Value> {
    // Keep F10 read-only: ANALYZE would execute the statement and therefore run a DML query.
    let rows = driver.query(&format!("EXPLAIN (BUFFERS, FORMAT JSON) {sql}"), &[]).await?;
    Ok(rows
        .first()
        .and_then(|row| row.try_get::<_, serde_json::Value>("QUERY PLAN").ok())
        .unwrap_or(serde_json::Value::Null))
}
