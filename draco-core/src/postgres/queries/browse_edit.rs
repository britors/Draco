use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct BrowseResult {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    pub total: i64,
}

pub async fn browse_table_data(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    offset: i64,
    limit: i64,
) -> Result<BrowseResult> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let count_rows = driver.query(&format!("SELECT COUNT(*) AS count FROM {q_schema}.{q_table}"), &[]).await?;
    let rows = driver
        .query(&format!("SELECT * FROM {q_schema}.{q_table} LIMIT $1 OFFSET $2"), &[&limit, &offset])
        .await?;
    let columns = rows.first().map(|r| r.columns().iter().map(|c| c.name().to_string()).collect()).unwrap_or_default();
    Ok(BrowseResult {
        columns,
        rows: rows.iter().map(row_to_json_map).collect(),
        total: count_rows.first().map(|r| get_i64(r, "count")).unwrap_or(0),
    })
}

pub async fn update_table_row(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    pk_cols: &[String],
    pk_vals: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    column: &str,
    new_value: Option<&str>,
) -> Result<()> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let q_col = quote_ident(column);
    let where_clause = pk_cols
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{} = ${}", quote_ident(c), i + 2))
        .collect::<Vec<_>>()
        .join(" AND ");
    let new_value = new_value.filter(|v| !v.is_empty());
    let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&new_value];
    params.extend_from_slice(pk_vals);
    driver
        .query(&format!("UPDATE {q_schema}.{q_table} SET {q_col} = $1 WHERE {where_clause}"), &params)
        .await?;
    Ok(())
}

pub async fn insert_table_row(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    columns: &[String],
    values: &[Option<String>],
) -> Result<()> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let col_list = columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
    let placeholders = (0..columns.len()).map(|i| format!("${}", i + 1)).collect::<Vec<_>>().join(", ");
    let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        values.iter().map(|v| v as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
    driver
        .query(&format!("INSERT INTO {q_schema}.{q_table} ({col_list}) VALUES ({placeholders})"), &params)
        .await?;
    Ok(())
}

pub async fn delete_table_row(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    pk_cols: &[String],
    pk_vals: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<()> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let where_clause = pk_cols
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{} = ${}", quote_ident(c), i + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    driver.query(&format!("DELETE FROM {q_schema}.{q_table} WHERE {where_clause}"), pk_vals).await?;
    Ok(())
}

pub async fn drop_table(driver: &PostgresDriver, schema: &str, table: &str) -> Result<()> {
    driver
        .query(&format!("DROP TABLE {}.{}", quote_ident(schema), quote_ident(table)), &[])
        .await?;
    Ok(())
}

pub async fn truncate_table(driver: &PostgresDriver, schema: &str, table: &str) -> Result<()> {
    driver
        .query(&format!("TRUNCATE TABLE {}.{}", quote_ident(schema), quote_ident(table)), &[])
        .await?;
    Ok(())
}
