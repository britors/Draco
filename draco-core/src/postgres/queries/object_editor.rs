use crate::error::{CoreError, Result};
use crate::postgres::pool::PostgresDriver;

use super::helpers::quote_ident;

pub async fn get_index_ddl(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    name: &str,
) -> Result<Option<(String, Option<String>)>> {
    let rows = driver
        .query(
            "SELECT pg_get_indexdef(i.oid) AS definition, con.conname AS constraint_name \
             FROM pg_class i \
             JOIN pg_namespace n ON n.oid = i.relnamespace \
             JOIN pg_index ix ON ix.indexrelid = i.oid \
             JOIN pg_class t ON t.oid = ix.indrelid \
             LEFT JOIN pg_constraint con ON con.conindid = i.oid \
             WHERE i.relkind = 'i' AND n.nspname = $1 AND t.relname = $2 \
                   AND i.relname = $3",
            &[&schema, &table, &name],
        )
        .await?;
    Ok(rows.first().map(|row| {
        (
            row.try_get::<_, String>("definition").unwrap_or_default(),
            row.try_get::<_, Option<String>>("constraint_name")
                .unwrap_or(None),
        )
    }))
}

pub async fn save_object_definition(driver: &PostgresDriver, ddl: &str) -> Result<()> {
    driver.query(ddl, &[]).await?;
    Ok(())
}

/// PostgreSQL cannot replace an index definition in place. Recreate it atomically so a syntax,
/// permission, or constraint error restores the original index automatically.
pub async fn replace_index_definition(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    name: &str,
    ddl: &str,
) -> Result<()> {
    let Some((_, constraint_name)) = get_index_ddl(driver, schema, table, name).await? else {
        return Err(CoreError::Other("Index was not found".to_string()));
    };
    if let Some(constraint_name) = constraint_name {
        return Err(CoreError::Other(format!(
            "Index is managed by constraint {constraint_name}"
        )));
    }
    let drop = format!("DROP INDEX {}.{}", quote_ident(schema), quote_ident(name));
    driver.execute_transaction(&[&drop, ddl]).await
}
