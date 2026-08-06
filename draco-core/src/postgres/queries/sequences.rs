use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct SequenceInfo {
    pub name: String,
    pub definition: String,
    pub detail: String,
}

pub async fn get_sequences(driver: &PostgresDriver, schema: &str) -> Result<Vec<SequenceInfo>> {
    let rows = driver
        .query(
            "SELECT sequencename AS name, data_type, start_value::text, min_value::text, \
                    max_value::text, increment_by::text, cycle, cache_size::text, \
                    last_value::text \
             FROM pg_sequences WHERE schemaname = $1 ORDER BY sequencename",
            &[&schema],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let name = get_str(r, "name");
            let cycle = get_bool(r, "cycle");
            let last_value = get_opt_str(r, "last_value");
            SequenceInfo {
                definition: format!(
                    "ALTER SEQUENCE {}.{}\n  AS {}\n  INCREMENT BY {}\n  MINVALUE {}\n  MAXVALUE {}\n  START WITH {}\n  CACHE {}\n  {};",
                    quote_ident(schema),
                    quote_ident(&name),
                    get_str(r, "data_type"),
                    get_str(r, "increment_by"),
                    get_str(r, "min_value"),
                    get_str(r, "max_value"),
                    get_str(r, "start_value"),
                    get_str(r, "cache_size"),
                    if cycle { "CYCLE" } else { "NO CYCLE" },
                ),
                detail: last_value
                    .map(|value| format!("last value {value}"))
                    .unwrap_or_else(|| "value unavailable".to_string()),
                name,
            }
        })
        .collect())
}

pub async fn create_sequence(driver: &PostgresDriver, schema: &str, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(crate::error::CoreError::Other("sequence name is required".to_string()));
    }
    let sql = format!("CREATE SEQUENCE {}.{}", quote_ident(schema), quote_ident(name));
    driver.query(&sql, &[]).await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct TriggerInfo {
    pub name: String,
    pub table: String,
    pub definition: String,
    /// True when the trigger was installed by `CREATE EXTENSION` (`pg_depend` records an
    /// unconditional `'e'` dependency on it) rather than authored by a user — it must be
    /// dropped by dropping/altering the extension, never directly.
    pub is_extension: bool,
}

pub async fn get_triggers(driver: &PostgresDriver, schema: &str) -> Result<Vec<TriggerInfo>> {
    let rows = driver
        .query(
            "SELECT t.tgname AS name, c.relname AS table_name, pg_get_triggerdef(t.oid) AS definition, \
                    EXISTS ( \
                        SELECT 1 FROM pg_depend d \
                        WHERE d.classid = 'pg_trigger'::regclass AND d.objid = t.oid AND d.deptype = 'e' \
                    ) AS is_extension \
             FROM pg_trigger t \
             JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE NOT t.tgisinternal AND n.nspname = $1 \
             ORDER BY c.relname, t.tgname",
            &[&schema],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| TriggerInfo {
            name: get_str(r, "name"),
            table: get_str(r, "table_name"),
            definition: get_str(r, "definition"),
            is_extension: get_bool(r, "is_extension"),
        })
        .collect())
}

pub async fn create_trigger(
    driver: &PostgresDriver,
    schema: &str,
    name: &str,
    table: &str,
    timing: &str,
    events: &str,
    function: &str,
) -> Result<()> {
    let name = name.trim();
    let table = table.trim();
    let function = function.trim();
    if name.is_empty() || table.is_empty() || function.is_empty() {
        return Err(crate::error::CoreError::Other(
            "trigger name, table, and function are required".to_string(),
        ));
    }

    let timing = timing.trim().to_ascii_uppercase();
    if !matches!(timing.as_str(), "BEFORE" | "AFTER" | "INSTEAD OF") {
        return Err(crate::error::CoreError::Other("invalid trigger timing".to_string()));
    }

    let mut event_list = Vec::new();
    for event in events.split(|ch: char| ch == ',' || ch.is_whitespace()) {
        if event.is_empty() {
            continue;
        }
        let event = event.to_ascii_uppercase();
        if !matches!(event.as_str(), "INSERT" | "UPDATE" | "DELETE" | "TRUNCATE") {
            return Err(crate::error::CoreError::Other(format!("invalid trigger event: {event}")));
        }
        if !event_list.contains(&event) {
            event_list.push(event);
        }
    }
    if event_list.is_empty() {
        return Err(crate::error::CoreError::Other("at least one trigger event is required".to_string()));
    }

    let function_name = function
        .split('.')
        .filter(|part| !part.trim().is_empty())
        .map(|part| quote_ident(part.trim()))
        .collect::<Vec<_>>();
    if function_name.is_empty() {
        return Err(crate::error::CoreError::Other("trigger function is required".to_string()));
    }
    let sql = format!(
        "CREATE TRIGGER {} {} {} ON {}.{} FOR EACH ROW EXECUTE FUNCTION {}()",
        quote_ident(name),
        timing,
        event_list.join(" OR "),
        quote_ident(schema),
        quote_ident(table),
        function_name.join("."),
    );
    driver.query(&sql, &[]).await?;
    Ok(())
}

/// Looks up the extension (if any) that owns trigger `schema.table.name` via `pg_depend`'s `'e'`
/// (extension member) dependency. A hit means the trigger was installed by `CREATE EXTENSION`,
/// not authored by a user, and must not be dropped directly.
async fn extension_owning_trigger(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    name: &str,
) -> Result<Option<String>> {
    let rows = driver
        .query(
            "SELECT e.extname FROM pg_depend d \
             JOIN pg_extension e ON e.oid = d.refobjid \
             JOIN pg_trigger t ON t.oid = d.objid \
             JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE d.classid = 'pg_trigger'::regclass AND d.deptype = 'e' \
               AND n.nspname = $1 AND c.relname = $2 AND t.tgname = $3",
            &[&schema, &table, &name],
        )
        .await?;
    Ok(rows.first().map(|r| get_str(r, "extname")))
}

pub async fn drop_trigger(driver: &PostgresDriver, schema: &str, table: &str, name: &str) -> Result<()> {
    if let Some(extname) = extension_owning_trigger(driver, schema, table, name).await? {
        return Err(crate::error::CoreError::Other(format!(
            "\"{name}\" was installed by the \"{extname}\" extension and can't be dropped directly. Drop or alter the extension instead."
        )));
    }
    let sql = format!(
        "DROP TRIGGER {} ON {}.{}",
        quote_ident(name),
        quote_ident(schema),
        quote_ident(table),
    );
    driver.query(&sql, &[]).await?;
    Ok(())
}

pub async fn seq_next_val(driver: &PostgresDriver, schema: &str, name: &str) -> Result<String> {
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));
    let rows = driver.query("SELECT nextval($1::text::regclass)::text AS v", &[&qualified]).await?;
    Ok(rows.first().map(|r| get_str(r, "v")).unwrap_or_default())
}

pub async fn seq_set_val(driver: &PostgresDriver, schema: &str, name: &str, value: i64) -> Result<()> {
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));
    driver.query("SELECT setval($1::text::regclass, $2)", &[&qualified, &value]).await?;
    Ok(())
}
