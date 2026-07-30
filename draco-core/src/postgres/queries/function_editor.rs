use serde::Serialize;

use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct FunctionOverload {
    pub ddl: String,
    pub args: String,
    pub oid: String,
}

pub async fn get_function_ddl(driver: &PostgresDriver, schema: &str, func_name: &str) -> Result<Vec<FunctionOverload>> {
    let rows = driver
        .query(
            "SELECT pg_get_functiondef(p.oid) AS ddl, pg_get_function_identity_arguments(p.oid) AS args, p.oid::text AS oid \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname = $1 AND p.proname = $2 ORDER BY p.oid",
            &[&schema, &func_name],
        )
        .await?;
    Ok(rows.iter().map(|r| FunctionOverload { ddl: get_str(r, "ddl"), args: get_str(r, "args"), oid: get_str(r, "oid") }).collect())
}

pub async fn save_function(driver: &PostgresDriver, ddl: &str) -> Result<()> {
    driver.query(ddl, &[]).await?;
    Ok(())
}

pub async fn validate_function(driver: &PostgresDriver, ddl: &str) -> Result<Option<String>> {
    driver.query("BEGIN", &[]).await?;
    let result = driver.query(ddl, &[]).await;
    driver.query("ROLLBACK", &[]).await.ok();
    match result {
        Ok(_) => Ok(None),
        Err(err) => Ok(Some(err.to_string())),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionTestResult {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}

pub async fn call_function(driver: &PostgresDriver, sql: &str) -> Result<FunctionTestResult> {
    execute_query(driver, sql).await
}

/// Result shape shared by the SQL query editor and the function tester — same "run arbitrary
/// SQL, get columns + rows back" operation either way.
pub type QueryResult = FunctionTestResult;

pub async fn execute_query(driver: &PostgresDriver, sql: &str) -> Result<QueryResult> {
    let rows = driver.query(sql, &[]).await?;
    let columns = rows.first().map(|r| r.columns().iter().map(|c| c.name().to_string()).collect()).unwrap_or_default();
    Ok(QueryResult { columns, rows: rows.iter().map(row_to_json_map).collect() })
}

/// Runs `sql` as a multi-statement script via the simple query protocol (`simple_query`,
/// the row-preserving sibling of `batch_execute` — already used by `alter_table` for the
/// side-effect-only case) instead of `execute_query`'s single prepared statement — lets the
/// editor's "Run as script" button execute a buffer with several `;`-separated statements,
/// which `execute_query` would reject.
///
/// Only the *last* statement's outcome is returned (a single grid can't show one shape per
/// statement): its rows if it produced any (e.g. a trailing `SELECT`/`RETURNING`), otherwise a
/// synthetic one-row `rows_affected` result so a plain `UPDATE`/`INSERT`/DDL command still
/// surfaces something instead of leaving the grid looking like nothing happened.
pub async fn execute_script(driver: &PostgresDriver, sql: &str) -> Result<QueryResult> {
    let messages = driver.simple_query(sql).await?;

    let mut current_columns: Option<Vec<String>> = None;
    let mut current_rows: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    let mut final_columns: Vec<String> = Vec::new();
    let mut final_rows: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    let mut final_affected: Option<u64> = None;

    for message in messages {
        match message {
            tokio_postgres::SimpleQueryMessage::RowDescription(columns) => {
                current_columns = Some(columns.iter().map(|c| c.name().to_string()).collect());
                current_rows.clear();
            }
            tokio_postgres::SimpleQueryMessage::Row(row) => {
                let mut map = serde_json::Map::new();
                for (i, col) in row.columns().iter().enumerate() {
                    let value = row.get(i).map(|v| serde_json::Value::String(v.to_string())).unwrap_or(serde_json::Value::Null);
                    map.insert(col.name().to_string(), value);
                }
                current_rows.push(map);
            }
            tokio_postgres::SimpleQueryMessage::CommandComplete(affected) => {
                // A statement just finished — commit it as the result-so-far, overwriting
                // whatever the previous statement left, so after the loop this holds only the
                // last statement's outcome.
                if let Some(columns) = current_columns.take() {
                    final_columns = columns;
                    final_rows = std::mem::take(&mut current_rows);
                    final_affected = None;
                } else {
                    final_columns = Vec::new();
                    final_rows = Vec::new();
                    final_affected = Some(affected);
                }
            }
            _ => {}
        }
    }

    if final_columns.is_empty() {
        final_columns = vec!["rows_affected".to_string()];
        let mut map = serde_json::Map::new();
        map.insert("rows_affected".to_string(), serde_json::Value::from(final_affected.unwrap_or(0)));
        final_rows = vec![map];
    }

    Ok(QueryResult { columns: final_columns, rows: final_rows })
}
