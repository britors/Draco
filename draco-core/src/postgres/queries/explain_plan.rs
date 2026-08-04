use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use tokio::sync::watch;
use tokio_postgres::Row;

fn explain_statement(sql: &str) -> String {
    // Keep F10 read-only: ANALYZE would execute INSERT/UPDATE/DELETE/MERGE statements. BUFFERS
    // also requires ANALYZE on supported PostgreSQL versions, so it does not belong in the safe
    // planning-only path.
    format!("EXPLAIN (FORMAT JSON) {sql}")
}

fn plan_from_rows(rows: &[Row]) -> serde_json::Value {
    rows.first()
        .and_then(|row| row.try_get::<_, serde_json::Value>("QUERY PLAN").ok())
        .unwrap_or(serde_json::Value::Null)
}

pub async fn execute_explain(driver: &PostgresDriver, sql: &str) -> Result<serde_json::Value> {
    let rows = driver.query(&explain_statement(sql), &[]).await?;
    Ok(plan_from_rows(&rows))
}

pub async fn execute_explain_cancelable(
    driver: &PostgresDriver,
    sql: &str,
    cancel_rx: watch::Receiver<bool>,
) -> Result<serde_json::Value> {
    let rows = driver
        .query_cancelable(&explain_statement(sql), &[], cancel_rx)
        .await?;
    Ok(plan_from_rows(&rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_is_planning_only() {
        let statement = explain_statement("UPDATE accounts SET active = false");
        assert_eq!(
            statement,
            "EXPLAIN (FORMAT JSON) UPDATE accounts SET active = false"
        );
        assert!(!statement.contains("ANALYZE"));
        assert!(!statement.contains("BUFFERS"));
    }
}
