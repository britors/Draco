use serde::Serialize;

use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;
use tokio::sync::watch;

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

fn validate_type_list(value: &str) -> Result<()> {
    if value.len() > 500
        || value.contains('\0')
        || value.contains(';')
        || value.contains("--")
        || value.contains("/*")
        || value.contains("*/")
        || value.chars().any(char::is_control)
    {
        return Err(crate::error::CoreError::Other("Argument type list is invalid".to_string()));
    }
    Ok(())
}

/// Looks up the extension (if any) that owns `schema.name(identity_arguments)` via `pg_depend`'s
/// `'e'` (extension member) dependency — the same relationship `\dx+` reports. A hit means the
/// object was installed by `CREATE EXTENSION`, not authored by a user, and must not be dropped
/// directly (PostgreSQL itself would refuse without `CASCADE`, but we want a clear message
/// instead of a raw driver error, and to keep the UI from offering the action at all).
async fn extension_owning_routine(
    driver: &PostgresDriver,
    schema: &str,
    name: &str,
    identity_arguments: &str,
) -> Result<Option<String>> {
    let qualified = format!("{}.{}({})", quote_ident(schema), quote_ident(name), identity_arguments);
    // `to_regprocedure($1)` rather than `$1::regprocedure`: a direct cast on the parameter makes
    // Postgres infer $1's type as `regprocedure` (an OID type) itself, which tokio-postgres can't
    // encode a Rust `String` into ("error serializing parameter 0"). Calling the function instead
    // resolves $1 to its declared `text` argument type, which `String` serializes fine as.
    let rows = driver
        .query(
            "SELECT e.extname FROM pg_depend d \
             JOIN pg_extension e ON e.oid = d.refobjid \
             WHERE d.classid = 'pg_proc'::regclass AND d.deptype = 'e' AND d.objid = to_regprocedure($1)",
            &[&qualified],
        )
        .await?;
    Ok(rows.first().map(|r| get_str(r, "extname")))
}

/// Drops a function or procedure. `identity_arguments` is the exact parameter type list
/// PostgreSQL itself reports via `pg_get_function_identity_arguments` (e.g. `"integer, text"`)
/// — required so DROP resolves the right overload instead of every overload sharing the name.
pub async fn drop_routine(
    driver: &PostgresDriver,
    schema: &str,
    name: &str,
    identity_arguments: &str,
    is_procedure: bool,
) -> Result<()> {
    validate_type_list(identity_arguments)?;
    let kind = if is_procedure { "PROCEDURE" } else { "FUNCTION" };
    if let Some(extname) = extension_owning_routine(driver, schema, name, identity_arguments).await? {
        return Err(crate::error::CoreError::Other(format!(
            "\"{name}\" was installed by the \"{extname}\" extension and can't be dropped directly. Drop or alter the extension instead."
        )));
    }
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));
    let sql = format!("DROP {kind} {qualified}({identity_arguments})");
    driver.query(&sql, &[]).await?;
    Ok(())
}

/// Runs an existing function/procedure with caller-supplied arguments, bound as real `$N`
/// parameters (never interpolated into the SQL text) so a value like `1); DROP TABLE x; --`
/// is just a literal text argument, not executable SQL. `params` covers only the IN/INOUT/
/// VARIADIC positions in call order — OUT-only parameters aren't passed by the caller in
/// PostgreSQL's own call syntax, so the UI must already have filtered those out.
pub async fn call_routine(
    driver: &PostgresDriver,
    schema: &str,
    name: &str,
    is_procedure: bool,
    params: &[Option<String>],
) -> Result<FunctionTestResult> {
    let placeholders: Vec<String> = (1..=params.len()).map(|i| format!("${i}")).collect();
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));
    let sql = if is_procedure {
        format!("CALL {}({})", qualified, placeholders.join(", "))
    } else {
        format!("SELECT * FROM {}({})", qualified, placeholders.join(", "))
    };
    let values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        params.iter().map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
    let rows = driver.query(&sql, &values).await?;
    let columns = rows.first().map(|r| r.columns().iter().map(|c| c.name().to_string()).collect()).unwrap_or_default();
    Ok(FunctionTestResult { columns, rows: rows.iter().map(row_to_json_map).collect() })
}

/// Result shape shared by the SQL query editor and the function tester — same "run arbitrary
/// SQL, get columns + rows back" operation either way.
pub type QueryResult = FunctionTestResult;

pub async fn execute_query(driver: &PostgresDriver, sql: &str) -> Result<QueryResult> {
    let rows = driver.query(sql, &[]).await?;
    let columns = rows.first().map(|r| r.columns().iter().map(|c| c.name().to_string()).collect()).unwrap_or_default();
    Ok(QueryResult { columns, rows: rows.iter().map(row_to_json_map).collect() })
}

pub async fn execute_query_cancelable(
    driver: &PostgresDriver,
    sql: &str,
    cancel_rx: watch::Receiver<bool>,
) -> Result<QueryResult> {
    let rows = driver.query_cancelable(sql, &[], cancel_rx).await?;
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

pub async fn execute_script_cancelable(
    driver: &PostgresDriver,
    sql: &str,
    cancel_rx: watch::Receiver<bool>,
) -> Result<QueryResult> {
    let messages = driver.simple_query_cancelable(sql, cancel_rx).await?;

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
