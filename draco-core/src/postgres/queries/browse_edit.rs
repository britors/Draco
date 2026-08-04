use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::postgres::pool::PostgresDriver;

use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct BrowseResult {
    pub columns: Vec<String>,
    /// Each cell is an exact JSON value encoded as text. Keeping the encoding intact avoids
    /// losing `bigint`/`numeric` precision when the result crosses a JavaScript IPC boundary.
    pub rows: Vec<BTreeMap<String, String>>,
    pub total: i64,
}

pub async fn browse_table_data(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    offset: i64,
    limit: i64,
) -> Result<BrowseResult> {
    browse_table_data_ordered(driver, schema, table, offset, limit, &[]).await
}

pub async fn browse_table_data_ordered(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    offset: i64,
    limit: i64,
    order_by: &[String],
) -> Result<BrowseResult> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let count_rows = driver
        .query(
            &format!("SELECT COUNT(*) AS count FROM {q_schema}.{q_table}"),
            &[],
        )
        .await?;
    let column_rows = driver
        .query(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position",
            &[&schema, &table],
        )
        .await?;
    let order_clause = if order_by.is_empty() {
        String::new()
    } else {
        format!(
            " ORDER BY {}",
            order_by
                .iter()
                .map(|column| quote_ident(column))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let rows = driver
        .query(
            &format!(
                "SELECT (SELECT jsonb_object_agg(cell.key, cell.value::text) \
                         FROM jsonb_each(to_jsonb(source)) AS cell) AS data \
                 FROM {q_schema}.{q_table} AS source{order_clause} LIMIT $1 OFFSET $2"
            ),
            &[&limit, &offset],
        )
        .await?;
    Ok(BrowseResult {
        columns: column_rows
            .iter()
            .map(|row| get_str(row, "column_name"))
            .collect(),
        rows: rows
            .iter()
            .filter_map(|row| row.try_get::<_, Value>("data").ok())
            .filter_map(|value| value.as_object().cloned())
            .map(|row| {
                row.into_iter()
                    .filter_map(|(column, value)| {
                        value.as_str().map(|value| (column, value.to_string()))
                    })
                    .collect()
            })
            .collect(),
        total: count_rows.first().map(|r| get_i64(r, "count")).unwrap_or(0),
    })
}

pub async fn update_table_row_json(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    keys: &[(String, Value)],
    column: &str,
    new_value: &Value,
) -> Result<()> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let q_column = quote_ident(column);
    let new_value_json = serde_json::to_string(new_value)
        .map_err(|error| CoreError::Other(format!("could not encode table value: {error}")))?;
    let (where_clause, key_params) = json_key_predicate(keys, 3)?;
    let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        vec![&column, &new_value_json];
    params.extend(
        key_params
            .iter()
            .map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync)),
    );
    let rows = driver
        .query(
            &format!(
                "UPDATE {q_schema}.{q_table} AS target \
                 SET {q_column} = (jsonb_populate_record(NULL::{q_schema}.{q_table}, \
                    jsonb_build_object($1::text, $2::jsonb))).{q_column} \
                 WHERE {where_clause} RETURNING 1"
            ),
            &params,
        )
        .await?;
    ensure_single_row(rows.len(), "update")
}

pub async fn insert_table_row_json(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    values: &[(String, Value)],
) -> Result<()> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let rows = if values.is_empty() {
        driver
            .query(
                &format!("INSERT INTO {q_schema}.{q_table} DEFAULT VALUES RETURNING 1"),
                &[],
            )
            .await?
    } else {
        let columns = values
            .iter()
            .map(|(column, _)| quote_ident(column))
            .collect::<Vec<_>>();
        let object = values.iter().cloned().collect::<serde_json::Map<_, _>>();
        let object_json = serde_json::to_string(&object)
            .map_err(|error| CoreError::Other(format!("could not encode table row: {error}")))?;
        driver
            .query(
                &format!(
                    "INSERT INTO {q_schema}.{q_table} ({column_list}) \
                     SELECT {column_list} FROM jsonb_populate_record(\
                        NULL::{q_schema}.{q_table}, $1::jsonb) RETURNING 1",
                    column_list = columns.join(", ")
                ),
                &[&object_json],
            )
            .await?
    };
    ensure_single_row(rows.len(), "insert")
}

pub async fn delete_table_row_json(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    keys: &[(String, Value)],
) -> Result<()> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let (where_clause, key_params) = json_key_predicate(keys, 1)?;
    let params = key_params
        .iter()
        .map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync))
        .collect::<Vec<_>>();
    let rows = driver
        .query(
            &format!(
                "DELETE FROM {q_schema}.{q_table} AS target \
                 WHERE {where_clause} RETURNING 1"
            ),
            &params,
        )
        .await?;
    ensure_single_row(rows.len(), "delete")
}

fn json_key_predicate(
    keys: &[(String, Value)],
    first_parameter: usize,
) -> Result<(String, Vec<String>)> {
    if keys.is_empty() {
        return Err(CoreError::Other(
            "a primary key is required to identify the table row".to_string(),
        ));
    }
    let mut params = Vec::with_capacity(keys.len() * 2);
    let predicates = keys
        .iter()
        .enumerate()
        .map(|(index, (column, value))| {
            let parameter = first_parameter + index * 2;
            params.push(column.clone());
            params.push(serde_json::to_string(value).map_err(|error| {
                CoreError::Other(format!("could not encode primary key value: {error}"))
            })?);
            Ok(format!(
                "to_jsonb(target) -> ${parameter}::text IS NOT DISTINCT FROM ${}::jsonb",
                parameter + 1
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((predicates.join(" AND "), params))
}

fn ensure_single_row(rows: usize, operation: &str) -> Result<()> {
    match rows {
        1 => Ok(()),
        0 => Err(CoreError::Other(format!(
            "table row was not found during {operation}"
        ))),
        _ => Err(CoreError::Other(format!(
            "table row identity was not unique during {operation}"
        ))),
    }
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
        .query(
            &format!("UPDATE {q_schema}.{q_table} SET {q_col} = $1 WHERE {where_clause}"),
            &params,
        )
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
    let col_list = columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (0..columns.len())
        .map(|i| format!("${}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = values
        .iter()
        .map(|v| v as &(dyn tokio_postgres::types::ToSql + Sync))
        .collect();
    driver
        .query(
            &format!("INSERT INTO {q_schema}.{q_table} ({col_list}) VALUES ({placeholders})"),
            &params,
        )
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
    driver
        .query(
            &format!("DELETE FROM {q_schema}.{q_table} WHERE {where_clause}"),
            pk_vals,
        )
        .await?;
    Ok(())
}

pub async fn drop_table(driver: &PostgresDriver, schema: &str, table: &str) -> Result<()> {
    driver
        .query(
            &format!("DROP TABLE {}.{}", quote_ident(schema), quote_ident(table)),
            &[],
        )
        .await?;
    Ok(())
}

pub async fn truncate_table(driver: &PostgresDriver, schema: &str, table: &str) -> Result<()> {
    driver
        .query(
            &format!(
                "TRUNCATE TABLE {}.{}",
                quote_ident(schema),
                quote_ident(table)
            ),
            &[],
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_key_predicate_uses_parameters_for_names_and_values() {
        let keys = vec![("id\" OR true --".to_string(), serde_json::json!(42))];
        let (sql, params) = json_key_predicate(&keys, 3).expect("predicate");
        assert_eq!(
            sql,
            "to_jsonb(target) -> $3::text IS NOT DISTINCT FROM $4::jsonb"
        );
        assert_eq!(params, vec!["id\" OR true --", "42"]);
        assert!(!sql.contains("OR true"));
    }

    #[test]
    fn json_key_predicate_rejects_missing_identity() {
        let error = json_key_predicate(&[], 1).expect_err("empty keys must fail");
        assert!(error.to_string().contains("primary key"));
    }
}
