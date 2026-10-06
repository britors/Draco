use super::helpers::*;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use serde::Serialize;

/// A column the table import may write. Generated columns are left out because `COPY` cannot
/// write them.
#[derive(Debug, Clone, Serialize)]
pub struct ImportTargetColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    /// A default or identity fills the column when the file does not map it.
    pub has_default: bool,
}

/// Columns of an ordinary or partitioned table, in table order. Empty when the relation does not
/// exist or is not a table.
pub async fn get_import_target_columns(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
) -> Result<Vec<ImportTargetColumn>> {
    // `attgenerated` exists from PostgreSQL 12; reading it through to_jsonb keeps older servers
    // working.
    let rows = driver
        .query(
            "SELECT a.attname::text AS name, format_type(a.atttypid, a.atttypmod) AS data_type, \
                    NOT a.attnotnull AS nullable, \
                    (a.atthasdef OR COALESCE(to_jsonb(a) ->> 'attidentity', '') <> '') AS has_default \
             FROM pg_attribute a \
             JOIN pg_class c ON c.oid = a.attrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p') \
               AND a.attnum > 0 AND NOT a.attisdropped \
               AND COALESCE(to_jsonb(a) ->> 'attgenerated', '') = '' \
             ORDER BY a.attnum",
            &[&schema, &table],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| ImportTargetColumn {
            name: get_str(row, "name"),
            data_type: get_str(row, "data_type"),
            nullable: get_bool(row, "nullable"),
            has_default: get_bool(row, "has_default"),
        })
        .collect())
}

/// `COPY "schema"."table" ("a", "b") FROM STDIN` in text format, matching
/// `table_import::push_copy_text_row`.
pub fn import_copy_statement(schema: &str, table: &str, columns: &[String]) -> String {
    let columns = columns
        .iter()
        .map(|column| quote_ident(column))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "COPY {}.{} ({columns}) FROM STDIN",
        quote_ident(schema),
        quote_ident(table)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_statement_quotes_every_identifier() {
        assert_eq!(
            import_copy_statement("my schema", "t\"x", &["id".into(), "Na\"me".into()]),
            "COPY \"my schema\".\"t\"\"x\" (\"id\", \"Na\"\"me\") FROM STDIN"
        );
    }
}
