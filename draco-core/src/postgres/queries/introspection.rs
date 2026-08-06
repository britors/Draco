use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

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
    driver.query(&format!("CREATE SCHEMA IF NOT EXISTS {}", quote_ident(schema_name)), &[]).await?;
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
    pub identity_arguments: String,
    /// True when the function/procedure was installed by `CREATE EXTENSION` (`pg_depend`
    /// records an unconditional `'e'` dependency on it) rather than authored by a user —
    /// it must be dropped by dropping/altering the extension, never directly.
    pub is_extension: bool,
}

pub async fn get_functions(driver: &PostgresDriver, schema: &str) -> Result<Vec<FunctionInfo>> {
    let rows = driver
        .query(
            "SELECT p.proname AS routine_name, \
                    CASE p.prokind WHEN 'p' THEN 'PROCEDURE' ELSE 'FUNCTION' END AS routine_type, \
                    COALESCE(pg_get_function_result(p.oid), '') AS data_type, \
                    p.oid::text AS specific_name, \
                    pg_get_function_identity_arguments(p.oid) AS identity_arguments, \
                    EXISTS ( \
                        SELECT 1 FROM pg_depend d \
                        WHERE d.classid = 'pg_proc'::regclass AND d.objid = p.oid AND d.deptype = 'e' \
                    ) AS is_extension \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname = $1 AND p.prokind IN ('f', 'p') \
             ORDER BY p.proname, pg_get_function_identity_arguments(p.oid)",
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
            identity_arguments: get_str(r, "identity_arguments"),
            is_extension: get_bool(r, "is_extension"),
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

#[derive(Debug, Clone, Serialize)]
pub struct CompletionColumn {
    pub schema: String,
    pub table: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CompletionData {
    pub schemas: Vec<String>,
    pub tables: Vec<CompletionTable>,
    pub columns: Vec<CompletionColumn>,
    pub functions: Vec<CompletionFunction>,
}

pub async fn get_completion_data(driver: &PostgresDriver) -> Result<CompletionData> {
    let schemas = driver
        .query(
            "SELECT schema_name AS name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('pg_catalog','information_schema','pg_toast') \
             ORDER BY schema_name",
            &[],
        )
        .await?;
    let tables = driver
        .query(
            "SELECT table_schema AS schema, table_name AS name, table_type \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog','information_schema','pg_toast') \
             ORDER BY table_schema, table_name",
            &[],
        )
        .await?;
    let columns = driver
        .query(
            "SELECT table_schema AS schema, table_name AS table_name, column_name AS name \
             FROM information_schema.columns \
             WHERE table_schema NOT IN ('pg_catalog','information_schema','pg_toast') \
             ORDER BY table_schema, table_name, ordinal_position",
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
        schemas: schemas.iter().map(|r| get_str(r, "name")).collect(),
        tables: tables
            .iter()
            .map(|r| CompletionTable {
                schema: get_str(r, "schema"),
                name: get_str(r, "name"),
                kind: if get_str(r, "table_type") == "VIEW" { TableKind::View } else { TableKind::Table },
            })
            .collect(),
        columns: columns
            .iter()
            .map(|r| CompletionColumn {
                schema: get_str(r, "schema"),
                table: get_str(r, "table_name"),
                name: get_str(r, "name"),
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
            "SELECT c.oid, c.relkind::text AS relkind FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname = $2 AND n.nspname = $1",
            &[&schema, &table],
        )
        .await?;
    let Some(oid_row) = oid_rows.first() else {
        return Ok(format!("-- Table \"{schema}\".\"{table}\" not found"));
    };
    let oid: u32 = oid_row.try_get("oid").unwrap_or(0);
    let relkind = get_str(oid_row, "relkind");
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);

    if relkind == "v" {
        let rows = driver
            .query("SELECT pg_get_viewdef($1::oid, true) AS definition", &[&oid])
            .await?;
        let definition = rows
            .first()
            .map(|row| get_str(row, "definition"))
            .unwrap_or_default();
        return Ok(format!(
            "CREATE OR REPLACE VIEW {q_schema}.{q_table} AS\n{};",
            definition.trim_end_matches(';')
        ));
    }

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
    Ok(format!("CREATE TABLE {q_schema}.{q_table} (\n{}\n);", lines.join(",\n")))
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub definition: String,
    pub size: String,
    pub is_unique: bool,
    pub is_primary: bool,
    pub constraint_name: Option<String>,
}

pub async fn get_indexes(driver: &PostgresDriver, schema: &str, table: &str) -> Result<Vec<IndexInfo>> {
    let rows = driver
        .query(
            "SELECT i.relname AS index_name, pg_get_indexdef(ix.indexrelid) AS index_def, \
                    pg_size_pretty(pg_relation_size(ix.indexrelid)) AS index_size, \
                    ix.indisunique AS is_unique, ix.indisprimary AS is_primary, \
                    con.conname AS constraint_name \
             FROM pg_index ix \
             JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_class t ON t.oid = ix.indrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             LEFT JOIN pg_constraint con ON con.conindid = ix.indexrelid \
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
            constraint_name: get_opt_str(r, "constraint_name"),
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
