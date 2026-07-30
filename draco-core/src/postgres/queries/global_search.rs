use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchKind {
    Table,
    View,
    Column,
    Function,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub kind: SearchKind,
    pub schema: String,
    pub table: Option<String>,
    pub name: String,
    pub detail: Option<String>,
}

pub async fn global_search(driver: &PostgresDriver, term: &str) -> Result<Vec<SearchResult>> {
    let like = format!("%{}%", term.to_lowercase());
    let tables = driver
        .query(
            "SELECT table_schema AS schema, table_name AS name, table_type \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog','information_schema','pg_toast') \
               AND LOWER(table_name) LIKE $1 \
             ORDER BY table_schema, table_name LIMIT 40",
            &[&like],
        )
        .await?;
    let columns = driver
        .query(
            "SELECT table_schema AS schema, table_name, column_name, data_type \
             FROM information_schema.columns \
             WHERE table_schema NOT IN ('pg_catalog','information_schema','pg_toast') \
               AND LOWER(column_name) LIKE $1 \
             ORDER BY table_schema, table_name, column_name LIMIT 40",
            &[&like],
        )
        .await?;
    let functions = driver
        .query(
            "SELECT routine_schema AS schema, routine_name AS name \
             FROM information_schema.routines \
             WHERE routine_schema NOT IN ('pg_catalog','information_schema','pg_toast') \
               AND routine_type IN ('FUNCTION','PROCEDURE') AND LOWER(routine_name) LIKE $1 \
             ORDER BY routine_schema, routine_name LIMIT 20",
            &[&like],
        )
        .await?;

    let mut results = Vec::new();
    for r in &tables {
        results.push(SearchResult {
            kind: if get_str(r, "table_type") == "VIEW" { SearchKind::View } else { SearchKind::Table },
            schema: get_str(r, "schema"),
            table: None,
            name: get_str(r, "name"),
            detail: None,
        });
    }
    for r in &columns {
        results.push(SearchResult {
            kind: SearchKind::Column,
            schema: get_str(r, "schema"),
            table: Some(get_str(r, "table_name")),
            name: get_str(r, "column_name"),
            detail: Some(get_str(r, "data_type")),
        });
    }
    for r in &functions {
        results.push(SearchResult { kind: SearchKind::Function, schema: get_str(r, "schema"), table: None, name: get_str(r, "name"), detail: None });
    }
    Ok(results)
}
