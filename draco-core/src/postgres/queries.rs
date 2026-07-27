//! All introspection, DDL, dashboard/stats and administration queries. Every function takes a
//! `&PostgresDriver` and returns typed rows — no SQL leaks past this module.

use serde::Serialize;
use tokio_postgres::Row;

use crate::error::Result;
use crate::postgres::pool::PostgresDriver;

fn get_str(row: &Row, col: &str) -> String {
    row.try_get::<_, String>(col).unwrap_or_default()
}

fn get_opt_str(row: &Row, col: &str) -> Option<String> {
    row.try_get::<_, Option<String>>(col).ok().flatten()
}

fn get_bool(row: &Row, col: &str) -> bool {
    row.try_get::<_, bool>(col).unwrap_or(false)
}

fn get_i64(row: &Row, col: &str) -> i64 {
    row.try_get::<_, i64>(col).unwrap_or(0)
}

fn get_i32(row: &Row, col: &str) -> i32 {
    row.try_get::<_, i32>(col).unwrap_or(0)
}

// ── Schema / table / column introspection ──────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SchemaInfo {
    pub name: String,
}

pub async fn get_schemas(driver: &PostgresDriver) -> Result<Vec<SchemaInfo>> {
    let rows = driver
        .query(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('pg_catalog','information_schema','pg_toast') \
             ORDER BY schema_name",
            &[],
        )
        .await?;
    Ok(rows.iter().map(|r| SchemaInfo { name: get_str(r, "schema_name") }).collect())
}

pub async fn create_schema(driver: &PostgresDriver, schema_name: &str) -> Result<()> {
    let safe = schema_name.replace('"', "");
    driver.query(&format!("CREATE SCHEMA IF NOT EXISTS \"{safe}\""), &[]).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TableKind {
    Table,
    View,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: TableKind,
}

pub async fn get_tables(driver: &PostgresDriver, schema: &str) -> Result<Vec<TableInfo>> {
    let rows = driver
        .query(
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_schema = $1 ORDER BY table_name",
            &[&schema],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| TableInfo {
            name: get_str(r, "table_name"),
            kind: if get_str(r, "table_type") == "VIEW" { TableKind::View } else { TableKind::Table },
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub has_default: bool,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
}

const COLUMNS_WITH_KEYS_SQL: &str = "
    SELECT
       c.column_name, c.data_type, c.is_nullable, c.column_default,
       COALESCE(pk.is_pk, false) AS is_primary_key,
       COALESCE(fk.is_fk, false) AS is_foreign_key
     FROM information_schema.columns c
     LEFT JOIN (
       SELECT kcu.column_name, true AS is_pk
       FROM information_schema.table_constraints tc
       JOIN information_schema.key_column_usage kcu
         ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
       WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = $1 AND tc.table_name = $2
     ) pk ON c.column_name = pk.column_name
     LEFT JOIN (
       SELECT kcu.column_name, true AS is_fk
       FROM information_schema.table_constraints tc
       JOIN information_schema.key_column_usage kcu
         ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
       WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2
     ) fk ON c.column_name = fk.column_name
     WHERE c.table_schema = $1 AND c.table_name = $2
     ORDER BY c.ordinal_position";

pub async fn get_columns(driver: &PostgresDriver, schema: &str, table: &str) -> Result<Vec<ColumnInfo>> {
    let rows = driver.query(COLUMNS_WITH_KEYS_SQL, &[&schema, &table]).await?;
    Ok(rows
        .iter()
        .map(|r| ColumnInfo {
            name: get_str(r, "column_name"),
            data_type: get_str(r, "data_type"),
            is_nullable: get_str(r, "is_nullable") == "YES",
            has_default: get_opt_str(r, "column_default").is_some(),
            is_primary_key: get_bool(r, "is_primary_key"),
            is_foreign_key: get_bool(r, "is_foreign_key"),
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RoutineKind {
    Function,
    Procedure,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionInfo {
    pub name: String,
    pub kind: RoutineKind,
    pub return_type: String,
    pub specific_name: String,
}

pub async fn get_functions(driver: &PostgresDriver, schema: &str) -> Result<Vec<FunctionInfo>> {
    let rows = driver
        .query(
            "SELECT routine_name, routine_type, data_type, specific_name \
             FROM information_schema.routines \
             WHERE routine_schema = $1 AND routine_type IN ('FUNCTION','PROCEDURE') \
             ORDER BY routine_name",
            &[&schema],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| FunctionInfo {
            name: get_str(r, "routine_name"),
            kind: if get_str(r, "routine_type") == "PROCEDURE" { RoutineKind::Procedure } else { RoutineKind::Function },
            return_type: get_str(r, "data_type"),
            specific_name: get_str(r, "specific_name"),
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionParam {
    pub name: String,
    pub data_type: String,
    pub mode: String,
}

pub async fn get_function_params(
    driver: &PostgresDriver,
    schema: &str,
    specific_name: &str,
) -> Result<Vec<FunctionParam>> {
    let rows = driver
        .query(
            "SELECT parameter_name, data_type, parameter_mode \
             FROM information_schema.parameters \
             WHERE specific_schema = $1 AND specific_name = $2 AND parameter_name IS NOT NULL \
             ORDER BY ordinal_position",
            &[&schema, &specific_name],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| FunctionParam {
            name: get_str(r, "parameter_name"),
            data_type: get_str(r, "data_type"),
            mode: get_str(r, "parameter_mode"),
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionTable {
    pub schema: String,
    pub name: String,
    pub kind: TableKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionFunction {
    pub schema: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CompletionData {
    pub tables: Vec<CompletionTable>,
    pub functions: Vec<CompletionFunction>,
}

pub async fn get_completion_data(driver: &PostgresDriver) -> Result<CompletionData> {
    let tables = driver
        .query(
            "SELECT table_schema AS schema, table_name AS name, table_type \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog','information_schema','pg_toast') \
             ORDER BY table_schema, table_name",
            &[],
        )
        .await?;
    let functions = driver
        .query(
            "SELECT routine_schema AS schema, routine_name AS name \
             FROM information_schema.routines \
             WHERE routine_schema NOT IN ('pg_catalog','information_schema','pg_toast') \
               AND routine_type IN ('FUNCTION','PROCEDURE') \
             ORDER BY routine_schema, routine_name",
            &[],
        )
        .await?;
    Ok(CompletionData {
        tables: tables
            .iter()
            .map(|r| CompletionTable {
                schema: get_str(r, "schema"),
                name: get_str(r, "name"),
                kind: if get_str(r, "table_type") == "VIEW" { TableKind::View } else { TableKind::Table },
            })
            .collect(),
        functions: functions
            .iter()
            .map(|r| CompletionFunction { schema: get_str(r, "schema"), name: get_str(r, "name") })
            .collect(),
    })
}

pub async fn get_table_estimates(driver: &PostgresDriver, schema: &str) -> Result<Vec<(String, i64)>> {
    let rows = driver
        .query(
            "SELECT c.relname, c.reltuples::bigint AS estimate \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relkind IN ('r','v','m')",
            &[&schema],
        )
        .await?;
    Ok(rows.iter().map(|r| (get_str(r, "relname"), get_i64(r, "estimate"))).collect())
}

pub async fn get_table_ddl(driver: &PostgresDriver, schema: &str, table: &str) -> Result<String> {
    let oid_rows = driver
        .query(
            "SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname = $2 AND n.nspname = $1",
            &[&schema, &table],
        )
        .await?;
    let Some(oid_row) = oid_rows.first() else {
        return Ok(format!("-- Table \"{schema}\".\"{table}\" not found"));
    };
    let oid: u32 = oid_row.try_get("oid").unwrap_or(0);

    let columns = driver
        .query(
            "SELECT a.attname, pg_catalog.format_type(a.atttypid, a.atttypmod) AS type, \
                    a.attnotnull AS notnull, pg_get_expr(d.adbin, d.adrelid) AS defval \
             FROM pg_attribute a \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum",
            &[&oid],
        )
        .await?;
    let constraints = driver
        .query(
            "SELECT conname, pg_get_constraintdef(oid) AS condef \
             FROM pg_constraint WHERE conrelid = $1 AND contype IN ('p','f','u','c') \
             ORDER BY contype, conname",
            &[&oid],
        )
        .await?;

    let q_schema = schema.replace('"', "\"\"");
    let q_table = table.replace('"', "\"\"");

    let mut lines: Vec<String> = columns
        .iter()
        .map(|c| {
            let mut line = format!("  \"{}\" {}", get_str(c, "attname"), get_str(c, "type"));
            if let Some(defval) = get_opt_str(c, "defval") {
                line.push_str(&format!(" DEFAULT {defval}"));
            }
            if get_bool(c, "notnull") {
                line.push_str(" NOT NULL");
            }
            line
        })
        .collect();
    lines.extend(
        constraints
            .iter()
            .map(|c| format!("  CONSTRAINT \"{}\" {}", get_str(c, "conname"), get_str(c, "condef"))),
    );
    Ok(format!("CREATE TABLE \"{q_schema}\".\"{q_table}\" (\n{}\n);", lines.join(",\n")))
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub definition: String,
    pub size: String,
    pub is_unique: bool,
    pub is_primary: bool,
}

pub async fn get_indexes(driver: &PostgresDriver, schema: &str, table: &str) -> Result<Vec<IndexInfo>> {
    let rows = driver
        .query(
            "SELECT i.relname AS index_name, pg_get_indexdef(ix.indexrelid) AS index_def, \
                    pg_size_pretty(pg_relation_size(ix.indexrelid)) AS index_size, \
                    ix.indisunique AS is_unique, ix.indisprimary AS is_primary \
             FROM pg_index ix \
             JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_class t ON t.oid = ix.indrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             WHERE n.nspname = $1 AND t.relname = $2 \
             ORDER BY ix.indisprimary DESC, ix.indisunique DESC, i.relname",
            &[&schema, &table],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| IndexInfo {
            name: get_str(r, "index_name"),
            definition: get_str(r, "index_def"),
            size: get_str(r, "index_size"),
            is_unique: get_bool(r, "is_unique"),
            is_primary: get_bool(r, "is_primary"),
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct ConstraintInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub definition: String,
}

async fn table_oid(driver: &PostgresDriver, schema: &str, table: &str) -> Result<Option<u32>> {
    let rows = driver
        .query(
            "SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname = $2 AND n.nspname = $1",
            &[&schema, &table],
        )
        .await?;
    Ok(rows.first().and_then(|r| r.try_get::<_, u32>("oid").ok()))
}

pub async fn get_constraints(driver: &PostgresDriver, schema: &str, table: &str) -> Result<Vec<ConstraintInfo>> {
    let Some(oid) = table_oid(driver, schema, table).await? else { return Ok(vec![]) };
    let rows = driver
        .query(
            "SELECT conname AS name, \
                    CASE contype WHEN 'p' THEN 'PRIMARY KEY' WHEN 'f' THEN 'FOREIGN KEY' \
                                 WHEN 'u' THEN 'UNIQUE' WHEN 'c' THEN 'CHECK' END AS type, \
                    pg_get_constraintdef(oid) AS definition \
             FROM pg_constraint WHERE conrelid = $1 AND contype IN ('p','f','u','c') \
             ORDER BY contype, conname",
            &[&oid],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| ConstraintInfo { name: get_str(r, "name"), kind: get_str(r, "type"), definition: get_str(r, "definition") })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FkDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, Serialize)]
pub struct FkMapEntry {
    pub direction: FkDirection,
    pub constraint_name: String,
    pub column: String,
    pub foreign_schema: String,
    pub foreign_table: String,
    pub foreign_column: String,
}

pub async fn get_fk_map(driver: &PostgresDriver, schema: &str, table: &str) -> Result<Vec<FkMapEntry>> {
    let out = driver
        .query(
            "SELECT kcu.constraint_name, kcu.column_name, \
                    ccu.table_schema AS foreign_schema, ccu.table_name AS foreign_table, ccu.column_name AS foreign_column \
             FROM information_schema.key_column_usage kcu \
             JOIN information_schema.table_constraints tc USING (constraint_name, table_schema, table_name) \
             JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_name = tc.constraint_name \
             WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2 \
             ORDER BY kcu.constraint_name, kcu.ordinal_position",
            &[&schema, &table],
        )
        .await?;
    let inc = driver
        .query(
            "SELECT tc.constraint_name, tc.table_schema AS referencing_schema, tc.table_name AS referencing_table, \
                    kcu.column_name AS referencing_column, ccu.column_name AS referenced_column \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu USING (constraint_name, table_schema, table_name) \
             JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_name = tc.constraint_name \
             WHERE tc.constraint_type = 'FOREIGN KEY' AND ccu.table_schema = $1 AND ccu.table_name = $2 \
             ORDER BY tc.table_schema, tc.table_name, tc.constraint_name",
            &[&schema, &table],
        )
        .await?;

    let mut entries: Vec<FkMapEntry> = out
        .iter()
        .map(|r| FkMapEntry {
            direction: FkDirection::Outgoing,
            constraint_name: get_str(r, "constraint_name"),
            column: get_str(r, "column_name"),
            foreign_schema: get_str(r, "foreign_schema"),
            foreign_table: get_str(r, "foreign_table"),
            foreign_column: get_str(r, "foreign_column"),
        })
        .collect();
    entries.extend(inc.iter().map(|r| FkMapEntry {
        direction: FkDirection::Incoming,
        constraint_name: get_str(r, "constraint_name"),
        column: get_str(r, "referenced_column"),
        foreign_schema: get_str(r, "referencing_schema"),
        foreign_table: get_str(r, "referencing_table"),
        foreign_column: get_str(r, "referencing_column"),
    }));
    Ok(entries)
}

#[derive(Debug, Clone, Serialize)]
pub struct TableDetailColumn {
    pub name: String,
    pub data_type: String,
    pub full_type: String,
    pub is_nullable: bool,
    pub column_default: Option<String>,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
    pub ordinal_position: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableDetail {
    pub columns: Vec<TableDetailColumn>,
    pub constraints: Vec<ConstraintInfo>,
    pub indexes: Vec<IndexInfo>,
    pub fk_map: Vec<FkMapEntry>,
    pub row_estimate: i64,
}

pub async fn get_table_detail(driver: &PostgresDriver, schema: &str, table: &str) -> Result<TableDetail> {
    let raw_cols = driver
        .query(
            "SELECT
               c.column_name, c.data_type, c.udt_name,
               c.character_maximum_length, c.numeric_precision, c.numeric_scale,
               c.is_nullable, c.column_default, c.ordinal_position,
               COALESCE(pk.is_pk, false) AS is_pk,
               COALESCE(fk.is_fk, false) AS is_fk
             FROM information_schema.columns c
             LEFT JOIN (
               SELECT kcu.column_name, true AS is_pk
               FROM information_schema.table_constraints tc
               JOIN information_schema.key_column_usage kcu
                 ON tc.constraint_name = kcu.constraint_name
                 AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
               WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = $1 AND tc.table_name = $2
             ) pk ON c.column_name = pk.column_name
             LEFT JOIN (
               SELECT kcu.column_name, true AS is_fk
               FROM information_schema.table_constraints tc
               JOIN information_schema.key_column_usage kcu
                 ON tc.constraint_name = kcu.constraint_name
                 AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
               WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2
             ) fk ON c.column_name = fk.column_name
             WHERE c.table_schema = $1 AND c.table_name = $2
             ORDER BY c.ordinal_position",
            &[&schema, &table],
        )
        .await?;
    let constraints = get_constraints(driver, schema, table).await?;
    let indexes = get_indexes(driver, schema, table).await?;
    let fk_map = get_fk_map(driver, schema, table).await?;
    let est_rows = driver
        .query(
            "SELECT reltuples::bigint AS estimate \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &table],
        )
        .await?;

    let columns = raw_cols
        .iter()
        .map(|r| {
            let udt_name = get_str(r, "udt_name");
            let char_len: Option<i32> = r.try_get("character_maximum_length").ok().flatten();
            let num_prec: Option<i32> = r.try_get("numeric_precision").ok().flatten();
            let num_scale: Option<i32> = r.try_get("numeric_scale").ok().flatten();
            let full_type = if let Some(len) = char_len {
                format!("{udt_name}({len})")
            } else if let (Some(p), Some(s)) = (num_prec, num_scale) {
                format!("{udt_name}({p},{s})")
            } else if let Some(p) = num_prec {
                format!("{udt_name}({p})")
            } else {
                udt_name
            };
            TableDetailColumn {
                name: get_str(r, "column_name"),
                data_type: get_str(r, "data_type"),
                full_type,
                is_nullable: get_str(r, "is_nullable") == "YES",
                column_default: get_opt_str(r, "column_default"),
                is_primary_key: get_bool(r, "is_pk"),
                is_foreign_key: get_bool(r, "is_fk"),
                ordinal_position: get_i32(r, "ordinal_position"),
            }
        })
        .collect();

    Ok(TableDetail {
        columns,
        constraints,
        indexes,
        fk_map,
        row_estimate: est_rows.first().map(|r| get_i64(r, "estimate")).unwrap_or(0),
    })
}

// ── ERD ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ErdColumn {
    pub name: String,
    pub data_type: String,
    pub is_pk: bool,
    pub is_fk: bool,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErdTable {
    pub name: String,
    pub kind: TableKind,
    pub columns: Vec<ErdColumn>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErdRelation {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    pub constraint_name: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ErdData {
    pub tables: Vec<ErdTable>,
    pub relations: Vec<ErdRelation>,
}

pub async fn get_erd_data(driver: &PostgresDriver, schema: &str) -> Result<ErdData> {
    let table_rows = driver
        .query(
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type IN ('BASE TABLE','VIEW') ORDER BY table_name",
            &[&schema],
        )
        .await?;
    let col_rows = driver
        .query(
            "SELECT table_name, column_name, data_type, udt_name, is_nullable \
             FROM information_schema.columns WHERE table_schema = $1 ORDER BY table_name, ordinal_position",
            &[&schema],
        )
        .await?;
    let pk_rows = driver
        .query(
            "SELECT tc.table_name, kcu.column_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu USING (constraint_name, table_schema, table_name) \
             WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = $1",
            &[&schema],
        )
        .await?;
    let fk_rows = driver
        .query(
            "SELECT kcu.table_name AS from_table, kcu.column_name AS from_column, \
                    ccu.table_name AS to_table, ccu.column_name AS to_column, tc.constraint_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu USING (constraint_name, table_schema, table_name) \
             JOIN information_schema.constraint_column_usage ccu \
               ON ccu.constraint_name = tc.constraint_name AND ccu.table_schema = tc.table_schema \
             WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 \
             ORDER BY kcu.table_name, tc.constraint_name, kcu.ordinal_position",
            &[&schema],
        )
        .await?;

    let pk_set: std::collections::HashSet<String> =
        pk_rows.iter().map(|r| format!("{}.{}", get_str(r, "table_name"), get_str(r, "column_name"))).collect();
    let fk_set: std::collections::HashSet<String> =
        fk_rows.iter().map(|r| format!("{}.{}", get_str(r, "from_table"), get_str(r, "from_column"))).collect();

    let mut cols_by_table: std::collections::HashMap<String, Vec<ErdColumn>> = std::collections::HashMap::new();
    for r in &col_rows {
        let table_name = get_str(r, "table_name");
        let column_name = get_str(r, "column_name");
        let data_type = get_str(r, "data_type");
        let udt_name = get_str(r, "udt_name");
        let type_label = if data_type == "USER-DEFINED" { udt_name } else { data_type };
        let key = format!("{table_name}.{column_name}");
        cols_by_table.entry(table_name).or_default().push(ErdColumn {
            is_pk: pk_set.contains(&key),
            is_fk: fk_set.contains(&key),
            is_nullable: get_str(r, "is_nullable") == "YES",
            name: column_name,
            data_type: type_label,
        });
    }

    let tables = table_rows
        .iter()
        .map(|r| {
            let name = get_str(r, "table_name");
            ErdTable {
                kind: if get_str(r, "table_type") == "VIEW" { TableKind::View } else { TableKind::Table },
                columns: cols_by_table.remove(&name).unwrap_or_default(),
                name,
            }
        })
        .collect();

    let relations = fk_rows
        .iter()
        .map(|r| ErdRelation {
            from_table: get_str(r, "from_table"),
            from_column: get_str(r, "from_column"),
            to_table: get_str(r, "to_table"),
            to_column: get_str(r, "to_column"),
            constraint_name: get_str(r, "constraint_name"),
        })
        .collect();

    Ok(ErdData { tables, relations })
}

// ── Browse / edit table data ────────────────────────────────────────────────────

fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowseResult {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    pub total: i64,
}

fn row_to_json_map(row: &Row) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (i, col) in row.columns().iter().enumerate() {
        map.insert(col.name().to_string(), pg_value_to_json(row, i));
    }
    map
}

fn pg_value_to_json(row: &Row, idx: usize) -> serde_json::Value {
    use tokio_postgres::types::Type;
    let ty = row.columns()[idx].type_();
    match *ty {
        Type::BOOL => row.try_get::<_, Option<bool>>(idx).ok().flatten().map(serde_json::Value::Bool).unwrap_or(serde_json::Value::Null),
        Type::INT2 => row.try_get::<_, Option<i16>>(idx).ok().flatten().map(|v| serde_json::json!(v)).unwrap_or(serde_json::Value::Null),
        Type::INT4 => row.try_get::<_, Option<i32>>(idx).ok().flatten().map(|v| serde_json::json!(v)).unwrap_or(serde_json::Value::Null),
        Type::INT8 => row.try_get::<_, Option<i64>>(idx).ok().flatten().map(|v| serde_json::json!(v)).unwrap_or(serde_json::Value::Null),
        Type::FLOAT4 => row.try_get::<_, Option<f32>>(idx).ok().flatten().map(|v| serde_json::json!(v)).unwrap_or(serde_json::Value::Null),
        Type::FLOAT8 => row.try_get::<_, Option<f64>>(idx).ok().flatten().map(|v| serde_json::json!(v)).unwrap_or(serde_json::Value::Null),
        _ => row
            .try_get::<_, Option<String>>(idx)
            .ok()
            .flatten()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    }
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

const ALLOWED_VACUUM_OPS: &[&str] = &["VACUUM", "ANALYZE", "VACUUM ANALYZE", "VACUUM FULL"];

pub async fn run_vacuum(driver: &PostgresDriver, schema: &str, table: &str, op: &str) -> Result<()> {
    if !ALLOWED_VACUUM_OPS.contains(&op) {
        return Err(crate::error::CoreError::Other(format!("unsupported operation: {op}")));
    }
    driver
        .query(&format!("{op} {}.{}", quote_ident(schema), quote_ident(table)), &[])
        .await?;
    Ok(())
}

// ── Column statistics ───────────────────────────────────────────────────────────

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

// ── Global search ────────────────────────────────────────────────────────────────

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

// ── EXPLAIN ──────────────────────────────────────────────────────────────────────

pub async fn execute_explain(driver: &PostgresDriver, sql: &str) -> Result<serde_json::Value> {
    let rows = driver.query(&format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}"), &[]).await?;
    let raw: String = rows.first().and_then(|r| r.try_get("QUERY PLAN").ok()).unwrap_or_default();
    Ok(serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null))
}

// ── Dashboard ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default)]
pub struct DashboardData {
    pub pg_version: String,
    pub uptime: String,
    pub host: String,
    pub port: String,
    pub db_name: String,
    pub db_size: String,
    pub encoding: String,
    pub collation: String,
    pub total_conn: i64,
    pub active_conn: i64,
    pub idle_conn: i64,
    pub idle_in_tx_conn: i64,
    pub max_conn: i64,
    pub commits: i64,
    pub rollbacks: i64,
    pub deadlocks: i64,
    pub temp_files: i64,
    pub cache_hit: String,
    pub top_tables: Vec<TopTable>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopTable {
    pub schema: String,
    pub table: String,
    pub total_size: String,
    pub raw_bytes: i64,
    pub n_live_tup: Option<i64>,
}

pub async fn get_dashboard(driver: &PostgresDriver) -> Result<DashboardData> {
    let server = driver
        .query(
            "SELECT split_part(version(), ' ', 2) AS pg_version, \
                    EXTRACT(EPOCH FROM (now() - pg_postmaster_start_time()))::bigint AS uptime_sec, \
                    inet_server_addr()::text AS host, inet_server_port()::text AS port",
            &[],
        )
        .await?;
    let db = driver
        .query(
            "SELECT current_database() AS db_name, pg_size_pretty(pg_database_size(current_database())) AS db_size, \
                    pg_encoding_to_char(encoding) AS encoding, datcollate AS collation \
             FROM pg_database WHERE datname = current_database()",
            &[],
        )
        .await?;
    let conns = driver
        .query(
            "SELECT count(*) AS total, \
                    count(*) FILTER (WHERE state = 'active') AS active, \
                    count(*) FILTER (WHERE state = 'idle') AS idle, \
                    count(*) FILTER (WHERE state = 'idle in transaction') AS idle_in_tx, \
                    current_setting('max_connections')::bigint AS max_conn \
             FROM pg_stat_activity WHERE datname = current_database()",
            &[],
        )
        .await?;
    let perf = driver
        .query(
            "SELECT xact_commit AS commits, xact_rollback AS rollbacks, deadlocks, temp_files, \
                    (CASE WHEN blks_hit + blks_read = 0 THEN '100' \
                          ELSE round(blks_hit::numeric / (blks_hit + blks_read) * 100, 2)::text END) AS cache_hit \
             FROM pg_stat_database WHERE datname = current_database()",
            &[],
        )
        .await?;
    let top_tables = driver
        .query(
            "SELECT n.nspname AS schema, c.relname AS table, \
                    pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size, \
                    pg_total_relation_size(c.oid) AS raw_bytes, s.n_live_tup \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid \
             WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema') \
             ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 10",
            &[],
        )
        .await?;

    let sv = server.first();
    let dbr = db.first();
    let cn = conns.first();
    let pf = perf.first();

    let uptime_sec = sv.map(|r| get_i64(r, "uptime_sec")).unwrap_or(0);
    let days = uptime_sec / 86400;
    let hours = (uptime_sec % 86400) / 3600;
    let mins = (uptime_sec % 3600) / 60;
    let uptime = if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    };

    Ok(DashboardData {
        pg_version: sv.map(|r| get_str(r, "pg_version")).unwrap_or_default(),
        uptime,
        host: sv.map(|r| get_str(r, "host")).unwrap_or_default(),
        port: sv.map(|r| get_str(r, "port")).unwrap_or_default(),
        db_name: dbr.map(|r| get_str(r, "db_name")).unwrap_or_default(),
        db_size: dbr.map(|r| get_str(r, "db_size")).unwrap_or_default(),
        encoding: dbr.map(|r| get_str(r, "encoding")).unwrap_or_default(),
        collation: dbr.map(|r| get_str(r, "collation")).unwrap_or_default(),
        total_conn: cn.map(|r| get_i64(r, "total")).unwrap_or(0),
        active_conn: cn.map(|r| get_i64(r, "active")).unwrap_or(0),
        idle_conn: cn.map(|r| get_i64(r, "idle")).unwrap_or(0),
        idle_in_tx_conn: cn.map(|r| get_i64(r, "idle_in_tx")).unwrap_or(0),
        max_conn: cn.map(|r| get_i64(r, "max_conn")).unwrap_or(0),
        commits: pf.map(|r| get_i64(r, "commits")).unwrap_or(0),
        rollbacks: pf.map(|r| get_i64(r, "rollbacks")).unwrap_or(0),
        deadlocks: pf.map(|r| get_i64(r, "deadlocks")).unwrap_or(0),
        temp_files: pf.map(|r| get_i64(r, "temp_files")).unwrap_or(0),
        cache_hit: pf.map(|r| get_str(r, "cache_hit")).unwrap_or_default(),
        top_tables: top_tables
            .iter()
            .map(|r| TopTable {
                schema: get_str(r, "schema"),
                table: get_str(r, "table"),
                total_size: get_str(r, "total_size"),
                raw_bytes: get_i64(r, "raw_bytes"),
                n_live_tup: r.try_get("n_live_tup").ok(),
            })
            .collect(),
    })
}

// ── Database stats dashboard ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DbStatsOverview {
    pub xact_commit: i64,
    pub xact_rollback: i64,
    pub deadlocks: i64,
    pub temp_files: i64,
    pub cache_hit_pct: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableSizeRow {
    pub schema: String,
    pub table: String,
    pub total_size: String,
    pub table_size: String,
    pub index_size: String,
    pub n_live_tup: Option<i64>,
    pub n_dead_tup: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BloatRow {
    pub schema: String,
    pub table: String,
    pub n_live_tup: Option<i64>,
    pub n_dead_tup: Option<i64>,
    pub bloat_pct: String,
    pub last_autovacuum: Option<String>,
    pub last_vacuum: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnusedIndexRow {
    pub schema: String,
    pub table: String,
    pub index: String,
    pub size: String,
    pub idx_scan: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeqScanRow {
    pub schema: String,
    pub table: String,
    pub seq_scan: i64,
    pub n_live_tup: Option<i64>,
    pub size: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DbStats {
    pub db: Option<DbStatsOverview>,
    pub tables: Vec<TableSizeRow>,
    pub bloat: Vec<BloatRow>,
    pub unused_idx: Vec<UnusedIndexRow>,
    pub seq_scans: Vec<SeqScanRow>,
}

pub async fn get_db_stats(driver: &PostgresDriver) -> Result<DbStats> {
    let db_rows = driver
        .query(
            "SELECT xact_commit, xact_rollback, deadlocks, temp_files, \
                    (CASE WHEN blks_hit + blks_read = 0 THEN '100' \
                          ELSE round(blks_hit::numeric / (blks_hit + blks_read) * 100, 2)::text END) AS cache_hit_pct, \
                    pg_size_pretty(pg_database_size(current_database())) AS size \
             FROM pg_stat_database WHERE datname = current_database()",
            &[],
        )
        .await?;
    let table_rows = driver
        .query(
            "SELECT n.nspname AS schema, c.relname AS table, \
                    pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size, \
                    pg_size_pretty(pg_relation_size(c.oid)) AS table_size, \
                    pg_size_pretty(pg_indexes_size(c.oid)) AS index_size, \
                    s.n_live_tup, s.n_dead_tup \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid \
             WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema') \
             ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 30",
            &[],
        )
        .await?;
    let bloat_rows = driver
        .query(
            "SELECT schemaname AS schema, relname AS table, n_live_tup, n_dead_tup, \
                    (CASE WHEN n_live_tup + n_dead_tup = 0 THEN '0' \
                          ELSE round(n_dead_tup::numeric / (n_live_tup + n_dead_tup) * 100, 1)::text END) AS bloat_pct, \
                    to_char(last_autovacuum, 'YYYY-MM-DD HH24:MI') AS last_autovacuum, \
                    to_char(last_vacuum, 'YYYY-MM-DD HH24:MI') AS last_vacuum \
             FROM pg_stat_user_tables WHERE n_dead_tup > 0 \
             ORDER BY n_dead_tup DESC LIMIT 30",
            &[],
        )
        .await?;
    let unused_idx_rows = driver
        .query(
            "SELECT schemaname AS schema, relname AS table, indexrelname AS index, \
                    pg_size_pretty(pg_relation_size(indexrelid)) AS size, idx_scan \
             FROM pg_stat_user_indexes WHERE idx_scan = 0 \
             ORDER BY pg_relation_size(indexrelid) DESC LIMIT 30",
            &[],
        )
        .await?;
    let seq_scan_rows = driver
        .query(
            "SELECT schemaname AS schema, relname AS table, seq_scan, n_live_tup, \
                    pg_size_pretty(pg_relation_size(relid)) AS size \
             FROM pg_stat_user_tables WHERE seq_scan > 50 AND n_live_tup > 1000 \
             ORDER BY seq_scan DESC LIMIT 20",
            &[],
        )
        .await?;

    Ok(DbStats {
        db: db_rows.first().map(|r| DbStatsOverview {
            xact_commit: get_i64(r, "xact_commit"),
            xact_rollback: get_i64(r, "xact_rollback"),
            deadlocks: get_i64(r, "deadlocks"),
            temp_files: get_i64(r, "temp_files"),
            cache_hit_pct: get_str(r, "cache_hit_pct"),
            size: get_str(r, "size"),
        }),
        tables: table_rows
            .iter()
            .map(|r| TableSizeRow {
                schema: get_str(r, "schema"),
                table: get_str(r, "table"),
                total_size: get_str(r, "total_size"),
                table_size: get_str(r, "table_size"),
                index_size: get_str(r, "index_size"),
                n_live_tup: r.try_get("n_live_tup").ok(),
                n_dead_tup: r.try_get("n_dead_tup").ok(),
            })
            .collect(),
        bloat: bloat_rows
            .iter()
            .map(|r| BloatRow {
                schema: get_str(r, "schema"),
                table: get_str(r, "table"),
                n_live_tup: r.try_get("n_live_tup").ok(),
                n_dead_tup: r.try_get("n_dead_tup").ok(),
                bloat_pct: get_str(r, "bloat_pct"),
                last_autovacuum: get_opt_str(r, "last_autovacuum"),
                last_vacuum: get_opt_str(r, "last_vacuum"),
            })
            .collect(),
        unused_idx: unused_idx_rows
            .iter()
            .map(|r| UnusedIndexRow { schema: get_str(r, "schema"), table: get_str(r, "table"), index: get_str(r, "index"), size: get_str(r, "size"), idx_scan: get_i64(r, "idx_scan") })
            .collect(),
        seq_scans: seq_scan_rows
            .iter()
            .map(|r| SeqScanRow { schema: get_str(r, "schema"), table: get_str(r, "table"), seq_scan: get_i64(r, "seq_scan"), n_live_tup: r.try_get("n_live_tup").ok(), size: get_str(r, "size") })
            .collect(),
    })
}

// ── Activity & locks ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ActivityRow {
    pub pid: i32,
    pub usename: Option<String>,
    pub application_name: Option<String>,
    pub state: Option<String>,
    pub wait_event_type: Option<String>,
    pub wait_event: Option<String>,
    pub duration: Option<String>,
    pub query: Option<String>,
}

pub async fn get_activity(driver: &PostgresDriver) -> Result<Vec<ActivityRow>> {
    let rows = driver
        .query(
            "SELECT pid, usename, application_name, state, wait_event_type, wait_event, \
                    EXTRACT(EPOCH FROM (now() - query_start))::numeric(10,1)::text AS duration, \
                    LEFT(query, 200) AS query \
             FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid() \
             ORDER BY duration DESC NULLS LAST",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| ActivityRow {
            pid: r.try_get("pid").unwrap_or(0),
            usename: get_opt_str(r, "usename"),
            application_name: get_opt_str(r, "application_name"),
            state: get_opt_str(r, "state"),
            wait_event_type: get_opt_str(r, "wait_event_type"),
            wait_event: get_opt_str(r, "wait_event"),
            duration: get_opt_str(r, "duration"),
            query: get_opt_str(r, "query"),
        })
        .collect())
}

pub async fn cancel_activity(driver: &PostgresDriver, pid: i32) -> Result<()> {
    driver.query("SELECT pg_cancel_backend($1)", &[&pid]).await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct LockRow {
    pub blocked_pid: i32,
    pub blocked_user: Option<String>,
    pub blocked_query: Option<String>,
    pub blocking_pid: i32,
    pub blocking_user: Option<String>,
    pub blocking_query: Option<String>,
    pub locktype: Option<String>,
    pub wait_sec: Option<String>,
}

pub async fn get_locks(driver: &PostgresDriver) -> Result<Vec<LockRow>> {
    let rows = driver
        .query(
            "SELECT
               blocked.pid AS blocked_pid, blocked_act.usename AS blocked_user, LEFT(blocked_act.query, 120) AS blocked_query,
               blocking.pid AS blocking_pid, blocking_act.usename AS blocking_user, LEFT(blocking_act.query, 120) AS blocking_query,
               blocked.locktype, EXTRACT(EPOCH FROM (now() - blocked_act.query_start))::numeric(10,1)::text AS wait_sec
             FROM pg_locks blocked
             JOIN pg_stat_activity blocked_act ON blocked_act.pid = blocked.pid
             JOIN pg_locks blocking ON blocking.transactionid = blocked.transactionid
                                    AND blocking.pid != blocked.pid AND blocking.granted
             JOIN pg_stat_activity blocking_act ON blocking_act.pid = blocking.pid
             WHERE NOT blocked.granted
             ORDER BY wait_sec DESC NULLS LAST",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| LockRow {
            blocked_pid: r.try_get("blocked_pid").unwrap_or(0),
            blocked_user: get_opt_str(r, "blocked_user"),
            blocked_query: get_opt_str(r, "blocked_query"),
            blocking_pid: r.try_get("blocking_pid").unwrap_or(0),
            blocking_user: get_opt_str(r, "blocking_user"),
            blocking_query: get_opt_str(r, "blocking_query"),
            locktype: get_opt_str(r, "locktype"),
            wait_sec: get_opt_str(r, "wait_sec"),
        })
        .collect())
}

// ── Sequences ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SequenceInfo {
    pub name: String,
}

pub async fn get_sequences(driver: &PostgresDriver, schema: &str) -> Result<Vec<SequenceInfo>> {
    let rows = driver
        .query(
            "SELECT relname AS name FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relkind = 'S' AND n.nspname = $1 ORDER BY relname",
            &[&schema],
        )
        .await?;
    Ok(rows.iter().map(|r| SequenceInfo { name: get_str(r, "name") }).collect())
}

pub async fn seq_next_val(driver: &PostgresDriver, schema: &str, name: &str) -> Result<String> {
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));
    let rows = driver.query("SELECT nextval($1)::text AS v", &[&qualified]).await?;
    Ok(rows.first().map(|r| get_str(r, "v")).unwrap_or_default())
}

pub async fn seq_set_val(driver: &PostgresDriver, schema: &str, name: &str, value: i64) -> Result<()> {
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));
    driver.query("SELECT setval($1, $2)", &[&qualified, &value]).await?;
    Ok(())
}

// ── Roles ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RoleInfo {
    pub rolname: String,
    pub rolsuper: bool,
    pub rolcreatedb: bool,
    pub rolcreaterole: bool,
    pub rolcanlogin: bool,
    pub rolconnlimit: i32,
    pub rolvaliduntil: Option<String>,
}

pub async fn get_roles(driver: &PostgresDriver) -> Result<Vec<RoleInfo>> {
    let rows = driver
        .query(
            "SELECT rolname, rolsuper, rolcreatedb, rolcreaterole, rolcanlogin, rolconnlimit, \
                    to_char(rolvaliduntil, 'YYYY-MM-DD') AS rolvaliduntil \
             FROM pg_roles ORDER BY rolname",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| RoleInfo {
            rolname: get_str(r, "rolname"),
            rolsuper: get_bool(r, "rolsuper"),
            rolcreatedb: get_bool(r, "rolcreatedb"),
            rolcreaterole: get_bool(r, "rolcreaterole"),
            rolcanlogin: get_bool(r, "rolcanlogin"),
            rolconnlimit: get_i32(r, "rolconnlimit"),
            rolvaliduntil: get_opt_str(r, "rolvaliduntil"),
        })
        .collect())
}

#[derive(Debug, Clone, Default)]
pub struct NewRole {
    pub name: String,
    pub password: Option<String>,
    pub login: bool,
    pub createdb: bool,
    pub createrole: bool,
    pub superuser: bool,
}

pub async fn create_role(driver: &PostgresDriver, role: &NewRole) -> Result<()> {
    let safe_name = role.name.replace('"', "\"\"");
    let mut parts = Vec::new();
    if role.login {
        parts.push("LOGIN".to_string());
    }
    if let Some(pw) = &role.password {
        if !pw.is_empty() {
            parts.push(format!("PASSWORD '{}'", pw.replace('\'', "''")));
        }
    }
    if role.createdb {
        parts.push("CREATEDB".to_string());
    }
    if role.createrole {
        parts.push("CREATEROLE".to_string());
    }
    if role.superuser {
        parts.push("SUPERUSER".to_string());
    }
    driver.query(&format!("CREATE ROLE \"{safe_name}\" {}", parts.join(" ")), &[]).await?;
    Ok(())
}

pub async fn drop_role(driver: &PostgresDriver, name: &str) -> Result<()> {
    let safe_name = name.replace('"', "\"\"");
    driver.query(&format!("DROP ROLE IF EXISTS \"{safe_name}\""), &[]).await?;
    Ok(())
}

// ── Extensions ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub default_version: Option<String>,
    pub installed_version: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Extensions {
    pub installed: Vec<ExtensionInfo>,
    pub available: Vec<ExtensionInfo>,
}

pub async fn get_extensions(driver: &PostgresDriver) -> Result<Extensions> {
    let installed = driver
        .query(
            "SELECT name, default_version, installed_version, comment FROM pg_available_extensions \
             WHERE installed_version IS NOT NULL ORDER BY name",
            &[],
        )
        .await?;
    let available = driver
        .query("SELECT name, default_version, comment FROM pg_available_extensions ORDER BY name LIMIT 200", &[])
        .await?;
    let to_ext = |r: &Row, with_installed: bool| ExtensionInfo {
        name: get_str(r, "name"),
        default_version: get_opt_str(r, "default_version"),
        installed_version: if with_installed { get_opt_str(r, "installed_version") } else { None },
        comment: get_opt_str(r, "comment"),
    };
    Ok(Extensions {
        installed: installed.iter().map(|r| to_ext(r, true)).collect(),
        available: available.iter().map(|r| to_ext(r, false)).collect(),
    })
}

pub async fn ext_install(driver: &PostgresDriver, name: &str) -> Result<()> {
    let safe = name.replace('"', "\"\"");
    driver.query(&format!("CREATE EXTENSION IF NOT EXISTS \"{safe}\""), &[]).await?;
    Ok(())
}

pub async fn ext_drop(driver: &PostgresDriver, name: &str) -> Result<()> {
    let safe = name.replace('"', "\"\"");
    driver.query(&format!("DROP EXTENSION IF EXISTS \"{safe}\""), &[]).await?;
    Ok(())
}

// ── Function editor ──────────────────────────────────────────────────────────────

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

// ── pg_cron job manager ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CronJob {
    pub jobid: i64,
    pub jobname: Option<String>,
    pub schedule: String,
    pub command: String,
    pub database: Option<String>,
    pub username: Option<String>,
    pub active: bool,
    pub last_status: Option<String>,
    pub last_run: Option<String>,
    pub last_msg: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CronJobs {
    pub installed: bool,
    pub jobs: Vec<CronJob>,
}

pub async fn get_jobs(driver: &PostgresDriver) -> Result<CronJobs> {
    let ext_rows = driver.query("SELECT extname FROM pg_extension WHERE extname = 'pg_cron'", &[]).await?;
    if ext_rows.is_empty() {
        return Ok(CronJobs { installed: false, jobs: vec![] });
    }
    let rows = driver
        .query(
            "SELECT j.jobid, j.jobname, j.schedule, j.command, j.database, j.username, j.active, \
                    r.status AS last_status, \
                    to_char(r.start_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS last_run, \
                    r.return_message AS last_msg \
             FROM cron.job j \
             LEFT JOIN LATERAL ( \
               SELECT status, start_time, return_message FROM cron.job_run_details \
               WHERE jobid = j.jobid ORDER BY start_time DESC LIMIT 1 \
             ) r ON true \
             ORDER BY j.jobid",
            &[],
        )
        .await?;
    Ok(CronJobs {
        installed: true,
        jobs: rows
            .iter()
            .map(|r| CronJob {
                jobid: r.try_get("jobid").unwrap_or(0),
                jobname: get_opt_str(r, "jobname"),
                schedule: get_str(r, "schedule"),
                command: get_str(r, "command"),
                database: get_opt_str(r, "database"),
                username: get_opt_str(r, "username"),
                active: get_bool(r, "active"),
                last_status: get_opt_str(r, "last_status"),
                last_run: get_opt_str(r, "last_run"),
                last_msg: get_opt_str(r, "last_msg"),
            })
            .collect(),
    })
}

pub async fn create_job(driver: &PostgresDriver, name: Option<&str>, schedule: &str, command: &str) -> Result<()> {
    match name.filter(|n| !n.trim().is_empty()) {
        Some(name) => driver.query("SELECT cron.schedule($1, $2, $3)", &[&name, &schedule, &command]).await?,
        None => driver.query("SELECT cron.schedule($1, $2)", &[&schedule, &command]).await?,
    };
    Ok(())
}

pub async fn update_job(driver: &PostgresDriver, job_id: i64, schedule: &str, command: &str) -> Result<()> {
    driver
        .query("UPDATE cron.job SET schedule = $1, command = $2 WHERE jobid = $3", &[&schedule, &command, &job_id])
        .await?;
    Ok(())
}

pub async fn toggle_job(driver: &PostgresDriver, job_id: i64, active: bool) -> Result<()> {
    driver.query("UPDATE cron.job SET active = $1 WHERE jobid = $2", &[&active, &job_id]).await?;
    Ok(())
}

pub async fn delete_job(driver: &PostgresDriver, job_id: i64) -> Result<()> {
    driver.query("SELECT cron.unschedule($1)", &[&job_id]).await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct JobRun {
    pub runid: i64,
    pub status: Option<String>,
    pub return_message: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub duration_sec: Option<String>,
}

pub async fn get_job_runs(driver: &PostgresDriver, job_id: i64) -> Result<Vec<JobRun>> {
    let rows = driver
        .query(
            "SELECT runid, status, return_message, \
                    to_char(start_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS start_time, \
                    to_char(end_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS end_time, \
                    (CASE WHEN end_time IS NOT NULL AND start_time IS NOT NULL \
                          THEN round(EXTRACT(EPOCH FROM (end_time - start_time))::numeric, 2)::text \
                          ELSE NULL END) AS duration_sec \
             FROM cron.job_run_details WHERE jobid = $1 ORDER BY start_time DESC LIMIT 50",
            &[&job_id],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| JobRun {
            runid: r.try_get("runid").unwrap_or(0),
            status: get_opt_str(r, "status"),
            return_message: get_opt_str(r, "return_message"),
            start_time: get_opt_str(r, "start_time"),
            end_time: get_opt_str(r, "end_time"),
            duration_sec: get_opt_str(r, "duration_sec"),
        })
        .collect())
}
