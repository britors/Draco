use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct QueryStatRow {
    pub query: String,
    pub calls: i64,
    pub total_exec_ms: f64,
    pub mean_exec_ms: f64,
    pub rows: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryStats {
    pub installed: bool,
    pub queries: Vec<QueryStatRow>,
}

pub async fn get_query_stats(driver: &PostgresDriver) -> Result<QueryStats> {
    let ext_rows = driver.query("SELECT extname FROM pg_extension WHERE extname = 'pg_stat_statements'", &[]).await?;
    if ext_rows.is_empty() {
        return Ok(QueryStats { installed: false, queries: vec![] });
    }
    let rows = driver
        .query(
            "SELECT LEFT(s.query, 300) AS query, s.calls, s.total_exec_time, s.mean_exec_time, s.rows \
             FROM pg_stat_statements s JOIN pg_database d ON d.oid = s.dbid \
             WHERE d.datname = current_database() \
             ORDER BY s.total_exec_time DESC LIMIT 30",
            &[],
        )
        .await?;
    Ok(QueryStats {
        installed: true,
        queries: rows
            .iter()
            .map(|r| QueryStatRow {
                query: get_str(r, "query"),
                calls: get_i64(r, "calls"),
                total_exec_ms: r.try_get::<_, f64>("total_exec_time").unwrap_or(0.0),
                mean_exec_ms: r.try_get::<_, f64>("mean_exec_time").unwrap_or(0.0),
                rows: get_i64(r, "rows"),
            })
            .collect(),
    })
}

pub async fn reset_query_stats(driver: &PostgresDriver) -> Result<()> {
    driver.query("SELECT pg_stat_statements_reset()", &[]).await?;
    Ok(())
}
