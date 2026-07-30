use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct ColumnStat {
    pub column: String,
    pub null_frac: Option<f32>,
    pub n_distinct: Option<f32>,
    pub most_common_vals: Option<String>,
    pub histogram_bounds: Option<String>,
}

pub async fn get_column_stats(driver: &PostgresDriver, schema: &str, table: &str) -> Result<Vec<ColumnStat>> {
    let rows = driver
        .query(
            "SELECT attname AS column, null_frac, n_distinct, \
                    most_common_vals::text AS most_common_vals, histogram_bounds::text AS histogram_bounds \
             FROM pg_stats \
             WHERE schemaname = $1 AND tablename = $2 \
             ORDER BY ( \
               SELECT ordinal_position FROM information_schema.columns c \
               WHERE c.table_schema = $1 AND c.table_name = $2 AND c.column_name = pg_stats.attname \
             )",
            &[&schema, &table],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| ColumnStat {
            column: get_str(r, "column"),
            null_frac: r.try_get("null_frac").ok(),
            n_distinct: r.try_get("n_distinct").ok(),
            most_common_vals: get_opt_str(r, "most_common_vals"),
            histogram_bounds: get_opt_str(r, "histogram_bounds"),
        })
        .collect())
}

/// Batched INSERT of previously-parsed CSV/JSON rows (parsing itself is the GTK layer's job —
/// picking the file and reading it is a UI concern; turning it into `INSERT`s is not).
pub async fn import_table_rows(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    columns: &[String],
    rows: &[Vec<Option<String>>],
) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let col_list = columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
    let mut inserted = 0usize;
    const CHUNK: usize = 500;
    for chunk in rows.chunks(CHUNK) {
        let placeholders = chunk
            .iter()
            .enumerate()
            .map(|(ri, _)| {
                let cols = (0..columns.len()).map(|ci| format!("${}", ri * columns.len() + ci + 1)).collect::<Vec<_>>();
                format!("({})", cols.join(", "))
            })
            .collect::<Vec<_>>()
            .join(", ");
        let flat: Vec<&Option<String>> = chunk.iter().flatten().collect();
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            flat.iter().map(|v| *v as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
        driver
            .query(&format!("INSERT INTO {q_schema}.{q_table} ({col_list}) VALUES {placeholders}"), &params)
            .await?;
        inserted += chunk.len();
    }
    Ok(inserted)
}
