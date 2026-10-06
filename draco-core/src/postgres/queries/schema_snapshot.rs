//! Reads one schema into a [`SchemaSnapshot`] for the schema diff. Every query runs in one
//! read-only transaction with an empty `search_path` (`PostgresDriver::catalog_snapshot`), so
//! the `pg_get_*def` functions qualify every name. Version-dependent catalog columns are read
//! through `to_jsonb(row) ->> 'column'`, which is NULL where the column does not exist.

use std::collections::BTreeMap;

use super::helpers::*;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use crate::schema_diff::{
    ColumnSnapshot, ConstraintSnapshot, RoutineSnapshot, SchemaSnapshot, SequenceSnapshot,
    TableSnapshot, ViewSnapshot,
};

/// Relations created by an extension are not part of the user's schema.
const NOT_FROM_EXTENSION: &str = "NOT EXISTS (SELECT 1 FROM pg_catalog.pg_depend dep \
     WHERE dep.classid = 'pg_catalog.pg_class'::pg_catalog.regclass AND dep.objid = c.oid AND dep.deptype = 'e')";

const TABLES_SQL: &str = "
SELECT c.relname::text AS table_name,
       CASE WHEN c.relkind = 'p' THEN pg_catalog.pg_get_partkeydef(c.oid) END AS partition_key,
       a.attname::text AS column_name,
       pg_catalog.format_type(a.atttypid, a.atttypmod) AS data_type,
       a.attnotnull AS not_null,
       pg_catalog.pg_get_expr(d.adbin, d.adrelid) AS default_expr,
       COALESCE(pg_catalog.to_jsonb(a) ->> 'attidentity', '') AS identity,
       COALESCE(pg_catalog.to_jsonb(a) ->> 'attgenerated', '') AS generated
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
WHERE n.nspname = $1 AND c.relkind IN ('r', 'p')
  AND COALESCE((pg_catalog.to_jsonb(c) ->> 'relispartition')::boolean, false) = false
  AND NOT_FROM_EXTENSION
ORDER BY c.relname, a.attnum";

const CONSTRAINTS_SQL: &str = "
SELECT c.relname::text AS table_name, con.conname::text AS name, con.contype::text AS kind,
       pg_catalog.pg_get_constraintdef(con.oid) AS definition
FROM pg_catalog.pg_constraint con
JOIN pg_catalog.pg_class c ON c.oid = con.conrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1 AND con.contype IN ('p', 'u', 'f', 'c', 'x') AND con.conislocal
  AND c.relkind IN ('r', 'p')
  AND COALESCE((pg_catalog.to_jsonb(c) ->> 'relispartition')::boolean, false) = false
ORDER BY c.relname, con.conname";

const INDEXES_SQL: &str = "
SELECT c.relname::text AS table_name, i.relname::text AS name,
       pg_catalog.pg_get_indexdef(i.oid) AS definition
FROM pg_catalog.pg_index x
JOIN pg_catalog.pg_class i ON i.oid = x.indexrelid
JOIN pg_catalog.pg_class c ON c.oid = x.indrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1 AND c.relkind IN ('r', 'p')
  AND COALESCE((pg_catalog.to_jsonb(c) ->> 'relispartition')::boolean, false) = false
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_constraint con
                  WHERE con.conindid = i.oid AND con.conrelid = c.oid AND con.contype IN ('p', 'u', 'x'))
ORDER BY c.relname, i.relname";

const TRIGGERS_SQL: &str = "
SELECT c.relname::text AS table_name, t.tgname::text AS name,
       pg_catalog.pg_get_triggerdef(t.oid) AS definition
FROM pg_catalog.pg_trigger t
JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1 AND NOT t.tgisinternal AND c.relkind IN ('r', 'p')
  AND COALESCE(pg_catalog.to_jsonb(t) ->> 'tgparentid', '0') = '0'
ORDER BY c.relname, t.tgname";

const VIEWS_SQL: &str = "
SELECT c.relname::text AS name, c.relkind = 'm' AS materialized,
       pg_catalog.pg_get_viewdef(c.oid) AS definition
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1 AND c.relkind IN ('v', 'm') AND NOT_FROM_EXTENSION
ORDER BY c.relname";

// Aggregates and window functions are filtered out before `pg_get_functiondef`, which rejects them.
const ROUTINES_SQL: &str = "
SELECT p.proname::text AS name,
       pg_catalog.pg_get_function_identity_arguments(p.oid) AS identity_arguments,
       kind.prokind = 'p' AS procedure,
       pg_catalog.pg_get_functiondef(p.oid) AS definition
FROM pg_catalog.pg_proc p
JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
CROSS JOIN LATERAL (SELECT COALESCE(pg_catalog.to_jsonb(p) ->> 'prokind',
         CASE WHEN (pg_catalog.to_jsonb(p) ->> 'proisagg')::boolean THEN 'a'
              WHEN (pg_catalog.to_jsonb(p) ->> 'proiswindow')::boolean THEN 'w'
              ELSE 'f' END) AS prokind) kind
WHERE n.nspname = $1 AND kind.prokind IN ('f', 'p')
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_depend dep
                  WHERE dep.classid = 'pg_catalog.pg_proc'::pg_catalog.regclass AND dep.objid = p.oid AND dep.deptype = 'e')
ORDER BY p.proname, 2";

// Identity sequences (`deptype = 'i'`) are part of their column, not separate objects.
const SEQUENCES_SQL: &str = "
SELECT c.relname::text AS name, pg_catalog.format_type(s.seqtypid, NULL) AS data_type,
       s.seqstart::text AS start, s.seqincrement::text AS increment, s.seqmin::text AS min,
       s.seqmax::text AS max, s.seqcache::text AS cache, s.seqcycle AS cycle
FROM pg_catalog.pg_sequence s
JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1 AND NOT_FROM_EXTENSION
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_depend dep
                  WHERE dep.classid = 'pg_catalog.pg_class'::pg_catalog.regclass AND dep.objid = c.oid AND dep.deptype = 'i')
ORDER BY c.relname";

pub async fn get_schema_snapshot(driver: &PostgresDriver, schema: &str) -> Result<SchemaSnapshot> {
    let with_filter = |sql: &str| sql.replace("NOT_FROM_EXTENSION", NOT_FROM_EXTENSION);
    let statements = [
        with_filter(TABLES_SQL),
        CONSTRAINTS_SQL.to_string(),
        INDEXES_SQL.to_string(),
        TRIGGERS_SQL.to_string(),
        with_filter(VIEWS_SQL),
        ROUTINES_SQL.to_string(),
        with_filter(SEQUENCES_SQL),
    ];
    let statements = statements.iter().map(String::as_str).collect::<Vec<_>>();
    let mut results = driver
        .catalog_snapshot(&statements, &[&schema])
        .await?
        .into_iter();
    let mut next = || results.next().unwrap_or_default();
    let (tables, constraints, indexes, triggers, views, routines, sequences) =
        (next(), next(), next(), next(), next(), next(), next());

    let mut snapshot = SchemaSnapshot {
        schema: schema.to_string(),
        ..SchemaSnapshot::default()
    };
    for row in &tables {
        let table = snapshot
            .tables
            .entry(get_str(row, "table_name"))
            .or_default();
        table.partition_key = get_opt_str(row, "partition_key");
        let Some(name) = get_opt_str(row, "column_name") else {
            continue;
        };
        let generated = get_str(row, "generated");
        let default = get_opt_str(row, "default_expr");
        table.columns.push(ColumnSnapshot {
            name,
            data_type: get_str(row, "data_type"),
            not_null: get_bool(row, "not_null"),
            identity: get_str(row, "identity").chars().next(),
            generated: if generated.is_empty() {
                None
            } else {
                default.clone()
            },
            default: if generated.is_empty() { default } else { None },
        });
    }
    for row in &constraints {
        if let Some(table) = snapshot.tables.get_mut(&get_str(row, "table_name")) {
            table.constraints.insert(
                get_str(row, "name"),
                ConstraintSnapshot {
                    kind: get_str(row, "kind").chars().next().unwrap_or('c'),
                    definition: get_str(row, "definition"),
                },
            );
        }
    }
    insert_named(&mut snapshot.tables, &indexes, |table| &mut table.indexes);
    insert_named(&mut snapshot.tables, &triggers, |table| &mut table.triggers);
    for row in &views {
        snapshot.views.insert(
            get_str(row, "name"),
            ViewSnapshot {
                materialized: get_bool(row, "materialized"),
                definition: get_str(row, "definition"),
            },
        );
    }
    for row in &routines {
        let name = get_str(row, "name");
        let identity_arguments = get_str(row, "identity_arguments");
        snapshot.routines.insert(
            format!("{name}({identity_arguments})"),
            RoutineSnapshot {
                name,
                identity_arguments,
                procedure: get_bool(row, "procedure"),
                definition: get_str(row, "definition"),
            },
        );
    }
    for row in &sequences {
        snapshot.sequences.insert(
            get_str(row, "name"),
            SequenceSnapshot {
                data_type: get_str(row, "data_type"),
                start: get_str(row, "start"),
                increment: get_str(row, "increment"),
                min: get_str(row, "min"),
                max: get_str(row, "max"),
                cache: get_str(row, "cache"),
                cycle: get_bool(row, "cycle"),
            },
        );
    }
    Ok(snapshot)
}

fn insert_named(
    tables: &mut BTreeMap<String, TableSnapshot>,
    rows: &[tokio_postgres::Row],
    field: impl Fn(&mut TableSnapshot) -> &mut BTreeMap<String, String>,
) {
    for row in rows {
        if let Some(table) = tables.get_mut(&get_str(row, "table_name")) {
            field(table).insert(get_str(row, "name"), get_str(row, "definition"));
        }
    }
}
