//! Schema comparison. A [`SchemaSnapshot`] is read from the catalog with an empty `search_path`
//! (see `queries::get_schema_snapshot`), so every definition is fully schema-qualified. Comparing
//! a source schema with a target schema rewrites the source's own qualifier to the target's, then
//! produces, per object, both definitions and the statements that make the target match the
//! source. The script is text for review: nothing here runs SQL.
//!
//! Covered objects: tables (columns, constraints, indexes, triggers), views and materialized
//! views, functions and procedures, and sequences. Objects that belong to an extension, table
//! partitions and identity sequences are left out by the snapshot query.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaSnapshot {
    pub schema: String,
    pub tables: BTreeMap<String, TableSnapshot>,
    pub views: BTreeMap<String, ViewSnapshot>,
    /// Keyed by `name(identity arguments)`.
    pub routines: BTreeMap<String, RoutineSnapshot>,
    pub sequences: BTreeMap<String, SequenceSnapshot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableSnapshot {
    pub columns: Vec<ColumnSnapshot>,
    /// `PARTITION BY …` clause body for a partitioned table.
    pub partition_key: Option<String>,
    /// Keyed by constraint name.
    pub constraints: BTreeMap<String, ConstraintSnapshot>,
    /// Keyed by index name; only indexes that do not back a constraint.
    pub indexes: BTreeMap<String, String>,
    /// Keyed by trigger name; `pg_get_triggerdef` output.
    pub triggers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnSnapshot {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default: Option<String>,
    /// `a` (always) or `d` (by default) for identity columns.
    pub identity: Option<char>,
    /// Expression of a stored generated column.
    pub generated: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSnapshot {
    /// `p`, `u`, `f`, `c` or `x`, as in `pg_constraint.contype`.
    pub kind: char,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewSnapshot {
    pub materialized: bool,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineSnapshot {
    pub name: String,
    pub identity_arguments: String,
    pub procedure: bool,
    /// `pg_get_functiondef` output.
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSnapshot {
    pub data_type: String,
    pub start: String,
    pub increment: String,
    pub min: String,
    pub max: String,
    pub cache: String,
    pub cycle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffObjectKind {
    Sequence,
    Table,
    View,
    MaterializedView,
    Function,
    Procedure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    /// Only in the source: the script creates it in the target.
    Added,
    /// Only in the target: the script drops it.
    Removed,
    Changed,
}

/// One differing object. `source_ddl`/`target_ddl` are the full definitions, both written for the
/// target schema, for a side-by-side view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectDiff {
    pub kind: DiffObjectKind,
    pub name: String,
    pub status: DiffStatus,
    pub source_ddl: Option<String>,
    pub target_ddl: Option<String>,
    pub statements: Vec<String>,
    /// Drops a table, a column or a sequence, or changes a column type.
    pub destructive: bool,
    /// Parts the script cannot express safely, left as `-- ` comments for manual review.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchemaDiff {
    pub objects: Vec<ObjectDiff>,
    /// Every statement, ordered so dependencies exist before they are used.
    pub script: String,
    pub destructive: bool,
}

/// Order of the statements in the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    DropTriggers,
    DropForeignKeys,
    DropViews,
    DropIndexes,
    Sequences,
    Tables,
    Routines,
    Views,
    Indexes,
    ForeignKeys,
    Triggers,
    DropRoutines,
    DropTables,
    DropSequences,
}

#[derive(Default)]
struct Builder {
    statements: Vec<(Phase, String)>,
    destructive: bool,
    notes: Vec<String>,
}

impl Builder {
    fn push(&mut self, phase: Phase, statement: impl Into<String>) {
        self.statements.push((phase, statement.into()));
    }
}

/// Compares `source` (the desired state) with `target` and returns what changes in the target.
pub fn diff_schemas(source: &SchemaSnapshot, target: &SchemaSnapshot) -> SchemaDiff {
    let source = requalify_snapshot(source, &target.schema);
    let schema = quote_ident(&target.schema);
    let mut entries: Vec<(ObjectDiff, Vec<(Phase, String)>)> = Vec::new();
    let mut add = |kind, name: &str, status, source_ddl, target_ddl, builder: Builder| {
        let Builder {
            statements,
            destructive,
            notes,
        } = builder;
        entries.push((
            ObjectDiff {
                kind,
                name: name.to_string(),
                status,
                source_ddl,
                target_ddl,
                statements: statements.iter().map(|(_, sql)| sql.clone()).collect(),
                destructive,
                notes,
            },
            statements,
        ));
    };

    for name in union_keys(&source.sequences, &target.sequences) {
        let qualified = format!("{schema}.{}", quote_ident(name));
        let (from, to) = (source.sequences.get(name), target.sequences.get(name));
        let mut builder = Builder::default();
        let status = match (from, to) {
            (Some(from), None) => {
                builder.push(Phase::Sequences, create_sequence(&qualified, from));
                DiffStatus::Added
            }
            (None, Some(_)) => {
                builder.push(Phase::DropSequences, format!("DROP SEQUENCE {qualified}"));
                builder.destructive = true;
                DiffStatus::Removed
            }
            (Some(from), Some(to)) if from != to => {
                builder.push(
                    Phase::Sequences,
                    format!("ALTER SEQUENCE {qualified} {}", sequence_options(from)),
                );
                DiffStatus::Changed
            }
            _ => continue,
        };
        add(
            DiffObjectKind::Sequence,
            name,
            status,
            from.map(|from| create_sequence(&qualified, from) + ";"),
            to.map(|to| create_sequence(&qualified, to) + ";"),
            builder,
        );
    }

    for name in union_keys(&source.tables, &target.tables) {
        let qualified = format!("{schema}.{}", quote_ident(name));
        let (from, to) = (source.tables.get(name), target.tables.get(name));
        let mut builder = Builder::default();
        let status = match (from, to) {
            (Some(from), None) => {
                create_table(&mut builder, &qualified, from);
                DiffStatus::Added
            }
            (None, Some(to)) => {
                for constraint in to.constraints.iter().filter(|(_, c)| c.kind == 'f') {
                    builder.push(
                        Phase::DropForeignKeys,
                        format!(
                            "ALTER TABLE {qualified} DROP CONSTRAINT {}",
                            quote_ident(constraint.0)
                        ),
                    );
                }
                builder.push(Phase::DropTables, format!("DROP TABLE {qualified}"));
                builder.destructive = true;
                DiffStatus::Removed
            }
            (Some(from), Some(to)) => {
                alter_table(&mut builder, &qualified, from, to);
                if builder.statements.is_empty() && builder.notes.is_empty() {
                    continue;
                }
                DiffStatus::Changed
            }
            (None, None) => continue,
        };
        add(
            DiffObjectKind::Table,
            name,
            status,
            from.map(|from| table_ddl(&qualified, from)),
            to.map(|to| table_ddl(&qualified, to)),
            builder,
        );
    }

    for name in union_keys(&source.views, &target.views) {
        let qualified = format!("{schema}.{}", quote_ident(name));
        let (from, to) = (source.views.get(name), target.views.get(name));
        let materialized = from.or(to).is_some_and(|view| view.materialized);
        let mut builder = Builder::default();
        let status = match (from, to) {
            (Some(from), None) => {
                builder.push(Phase::Views, create_view(&qualified, from, false));
                DiffStatus::Added
            }
            (None, Some(to)) => {
                builder.push(Phase::DropViews, drop_view(&qualified, to));
                DiffStatus::Removed
            }
            (Some(from), Some(to))
                if normalize_body(&from.definition) != normalize_body(&to.definition)
                    || from.materialized != to.materialized =>
            {
                if from.materialized || to.materialized {
                    builder.push(Phase::DropViews, drop_view(&qualified, to));
                    builder.push(Phase::Views, create_view(&qualified, from, false));
                } else {
                    builder.push(Phase::Views, create_view(&qualified, from, true));
                    builder.notes.push(
                        "CREATE OR REPLACE VIEW fails if columns are removed or retyped; drop and recreate the view (and its dependents) in that case".to_string(),
                    );
                }
                DiffStatus::Changed
            }
            _ => continue,
        };
        add(
            if materialized {
                DiffObjectKind::MaterializedView
            } else {
                DiffObjectKind::View
            },
            name,
            status,
            from.map(|from| create_view(&qualified, from, false) + ";"),
            to.map(|to| create_view(&qualified, to, false) + ";"),
            builder,
        );
    }

    for key in union_keys(&source.routines, &target.routines) {
        let (from, to) = (source.routines.get(key), target.routines.get(key));
        let routine = from.or(to).expect("key comes from one of the maps");
        let mut builder = Builder::default();
        let status = match (from, to) {
            (Some(from), None) => {
                builder.push(Phase::Routines, from.definition.trim_end().to_string());
                DiffStatus::Added
            }
            (None, Some(to)) => {
                builder.push(
                    Phase::DropRoutines,
                    format!(
                        "DROP {} {schema}.{}({})",
                        if to.procedure {
                            "PROCEDURE"
                        } else {
                            "FUNCTION"
                        },
                        quote_ident(&to.name),
                        to.identity_arguments
                    ),
                );
                DiffStatus::Removed
            }
            (Some(from), Some(to))
                if normalize_body(&from.definition) != normalize_body(&to.definition) =>
            {
                builder.push(Phase::Routines, from.definition.trim_end().to_string());
                builder.notes.push(
                    "CREATE OR REPLACE cannot change the return type or argument names; drop the routine first in that case".to_string(),
                );
                DiffStatus::Changed
            }
            _ => continue,
        };
        add(
            if routine.procedure {
                DiffObjectKind::Procedure
            } else {
                DiffObjectKind::Function
            },
            key,
            status,
            from.map(|from| from.definition.trim_end().to_string() + ";"),
            to.map(|to| to.definition.trim_end().to_string() + ";"),
            builder,
        );
    }

    let mut ordered: Vec<(Phase, usize, String)> = Vec::new();
    for (index, (_, statements)) in entries.iter().enumerate() {
        for (phase, statement) in statements {
            ordered.push((*phase, index, statement.clone()));
        }
    }
    ordered.sort_by_key(|(phase, index, _)| (*phase, *index));
    let destructive = entries.iter().any(|(object, _)| object.destructive);
    let notes = entries
        .iter()
        .flat_map(|(object, _)| object.notes.iter().map(move |note| (object, note)))
        .map(|(object, note)| format!("-- {}: {note}", object.name))
        .collect::<Vec<_>>();
    let mut script = format!(
        "-- Draco schema diff: makes {} match the source schema.\n-- Review before running. A script run in the SQL editor executes as one transaction.\n",
        quote_ident(&target.schema)
    );
    for note in &notes {
        script.push_str(note);
        script.push('\n');
    }
    if ordered.is_empty() {
        script.push_str("-- No differences\n");
    }
    for (_, _, statement) in &ordered {
        script.push('\n');
        script.push_str(statement);
        script.push_str(";\n");
    }
    SchemaDiff {
        objects: entries.into_iter().map(|(object, _)| object).collect(),
        script,
        destructive,
    }
}

fn union_keys<'a, V>(
    left: &'a BTreeMap<String, V>,
    right: &'a BTreeMap<String, V>,
) -> BTreeSet<&'a String> {
    left.keys().chain(right.keys()).collect()
}

fn create_sequence(qualified: &str, sequence: &SequenceSnapshot) -> String {
    format!("CREATE SEQUENCE {qualified} {}", sequence_options(sequence))
}

fn sequence_options(sequence: &SequenceSnapshot) -> String {
    format!(
        "AS {} INCREMENT BY {} MINVALUE {} MAXVALUE {} START WITH {} CACHE {} {}",
        sequence.data_type,
        sequence.increment,
        sequence.min,
        sequence.max,
        sequence.start,
        sequence.cache,
        if sequence.cycle { "CYCLE" } else { "NO CYCLE" }
    )
}

fn column_definition(column: &ColumnSnapshot) -> String {
    let mut definition = format!("{} {}", quote_ident(&column.name), column.data_type);
    if let Some(expression) = &column.generated {
        definition.push_str(&format!(" GENERATED ALWAYS AS ({expression}) STORED"));
    } else if let Some(identity) = column.identity {
        definition.push_str(if identity == 'a' {
            " GENERATED ALWAYS AS IDENTITY"
        } else {
            " GENERATED BY DEFAULT AS IDENTITY"
        });
    } else if let Some(default) = &column.default {
        definition.push_str(&format!(" DEFAULT {default}"));
    }
    if column.not_null && column.identity.is_none() {
        definition.push_str(" NOT NULL");
    }
    definition
}

/// Full `CREATE TABLE` with every constraint inline, plus indexes and triggers, for display.
fn table_ddl(qualified: &str, table: &TableSnapshot) -> String {
    let mut lines = table
        .columns
        .iter()
        .map(column_definition)
        .collect::<Vec<_>>();
    for (name, constraint) in &table.constraints {
        lines.push(format!(
            "CONSTRAINT {} {}",
            quote_ident(name),
            constraint.definition
        ));
    }
    let mut ddl = format!(
        "CREATE TABLE {qualified} (\n    {}\n)",
        lines.join(",\n    ")
    );
    if let Some(key) = &table.partition_key {
        ddl.push_str(&format!(" PARTITION BY {key}"));
    }
    ddl.push(';');
    for definition in table.indexes.values().chain(table.triggers.values()) {
        ddl.push('\n');
        ddl.push_str(definition);
        ddl.push(';');
    }
    ddl
}

fn create_table(builder: &mut Builder, qualified: &str, table: &TableSnapshot) {
    let mut lines = table
        .columns
        .iter()
        .map(column_definition)
        .collect::<Vec<_>>();
    // Foreign keys are added after every table exists, so creation order does not matter.
    for (name, constraint) in table.constraints.iter().filter(|(_, c)| c.kind != 'f') {
        lines.push(format!(
            "CONSTRAINT {} {}",
            quote_ident(name),
            constraint.definition
        ));
    }
    let mut ddl = format!(
        "CREATE TABLE {qualified} (\n    {}\n)",
        lines.join(",\n    ")
    );
    if let Some(key) = &table.partition_key {
        ddl.push_str(&format!(" PARTITION BY {key}"));
    }
    builder.push(Phase::Tables, ddl);
    for (name, constraint) in table.constraints.iter().filter(|(_, c)| c.kind == 'f') {
        builder.push(
            Phase::ForeignKeys,
            format!(
                "ALTER TABLE {qualified} ADD CONSTRAINT {} {}",
                quote_ident(name),
                constraint.definition
            ),
        );
    }
    for definition in table.indexes.values() {
        builder.push(Phase::Indexes, definition.clone());
    }
    for definition in table.triggers.values() {
        builder.push(Phase::Triggers, definition.clone());
    }
}

fn alter_table(builder: &mut Builder, qualified: &str, from: &TableSnapshot, to: &TableSnapshot) {
    if from.partition_key != to.partition_key {
        builder
            .notes
            .push("the partitioning differs; recreate the table to change it".to_string());
    }
    let target_columns = to
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect::<BTreeMap<_, _>>();
    let source_columns = from
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<BTreeSet<_>>();
    for column in &from.columns {
        let name = quote_ident(&column.name);
        let Some(existing) = target_columns.get(column.name.as_str()) else {
            builder.push(
                Phase::Tables,
                format!(
                    "ALTER TABLE {qualified} ADD COLUMN {}",
                    column_definition(column)
                ),
            );
            continue;
        };
        if existing.generated != column.generated {
            builder.notes.push(format!(
                "column {} changes its generation expression; adjust it manually",
                column.name
            ));
            continue;
        }
        if existing.data_type != column.data_type {
            builder.push(
                Phase::Tables,
                format!(
                    "ALTER TABLE {qualified} ALTER COLUMN {name} TYPE {} USING {name}::{}",
                    column.data_type, column.data_type
                ),
            );
            builder.destructive = true;
        }
        if existing.identity.is_some() && column.identity.is_none() {
            builder.push(
                Phase::Tables,
                format!("ALTER TABLE {qualified} ALTER COLUMN {name} DROP IDENTITY"),
            );
        }
        if existing.default != column.default && column.generated.is_none() {
            builder.push(
                Phase::Tables,
                match &column.default {
                    Some(default) => {
                        format!("ALTER TABLE {qualified} ALTER COLUMN {name} SET DEFAULT {default}")
                    }
                    None => format!("ALTER TABLE {qualified} ALTER COLUMN {name} DROP DEFAULT"),
                },
            );
        }
        if let Some(identity) = column.identity {
            let kind = if identity == 'a' {
                "ALWAYS"
            } else {
                "BY DEFAULT"
            };
            match existing.identity {
                // An identity column has no default, so the block above already dropped any.
                None => {
                    if !existing.not_null {
                        builder.push(
                            Phase::Tables,
                            format!("ALTER TABLE {qualified} ALTER COLUMN {name} SET NOT NULL"),
                        );
                    }
                    builder.push(
                        Phase::Tables,
                        format!(
                            "ALTER TABLE {qualified} ALTER COLUMN {name} ADD GENERATED {kind} AS IDENTITY"
                        ),
                    );
                }
                Some(current) if current != identity => builder.push(
                    Phase::Tables,
                    format!("ALTER TABLE {qualified} ALTER COLUMN {name} SET GENERATED {kind}"),
                ),
                Some(_) => {}
            }
        }
        if existing.not_null != column.not_null && column.identity.is_none() {
            builder.push(
                Phase::Tables,
                format!(
                    "ALTER TABLE {qualified} ALTER COLUMN {name} {} NOT NULL",
                    if column.not_null { "SET" } else { "DROP" }
                ),
            );
        }
    }
    for column in &to.columns {
        if !source_columns.contains(column.name.as_str()) {
            builder.push(
                Phase::Tables,
                format!(
                    "ALTER TABLE {qualified} DROP COLUMN {}",
                    quote_ident(&column.name)
                ),
            );
            builder.destructive = true;
        }
    }

    for name in union_keys(&from.constraints, &to.constraints) {
        let (wanted, existing) = (from.constraints.get(name), to.constraints.get(name));
        if wanted == existing {
            continue;
        }
        let quoted = quote_ident(name);
        if let Some(existing) = existing {
            builder.push(
                if existing.kind == 'f' {
                    Phase::DropForeignKeys
                } else {
                    Phase::Tables
                },
                format!("ALTER TABLE {qualified} DROP CONSTRAINT {quoted}"),
            );
        }
        if let Some(wanted) = wanted {
            builder.push(
                if wanted.kind == 'f' {
                    Phase::ForeignKeys
                } else {
                    Phase::Tables
                },
                format!(
                    "ALTER TABLE {qualified} ADD CONSTRAINT {quoted} {}",
                    wanted.definition
                ),
            );
        }
    }

    let schema = qualified
        .rsplit_once('.')
        .map(|(schema, _)| schema)
        .unwrap_or_default();
    for name in union_keys(&from.indexes, &to.indexes) {
        let (wanted, existing) = (from.indexes.get(name), to.indexes.get(name));
        if wanted == existing {
            continue;
        }
        // Dropped before the column changes: dropping or retyping a column may remove or
        // rebuild its indexes.
        if existing.is_some() {
            builder.push(
                Phase::DropIndexes,
                format!("DROP INDEX {schema}.{}", quote_ident(name)),
            );
        }
        if let Some(wanted) = wanted {
            builder.push(Phase::Indexes, wanted.clone());
        }
    }
    for name in union_keys(&from.triggers, &to.triggers) {
        let (wanted, existing) = (from.triggers.get(name), to.triggers.get(name));
        if wanted == existing {
            continue;
        }
        if existing.is_some() {
            builder.push(
                Phase::DropTriggers,
                format!("DROP TRIGGER {} ON {qualified}", quote_ident(name)),
            );
        }
        if let Some(wanted) = wanted {
            builder.push(Phase::Triggers, wanted.clone());
        }
    }
}

fn create_view(qualified: &str, view: &ViewSnapshot, replace: bool) -> String {
    let body = view.definition.trim().trim_end_matches(';').trim_end();
    match (view.materialized, replace) {
        (true, _) => format!("CREATE MATERIALIZED VIEW {qualified} AS\n{body}"),
        (false, true) => format!("CREATE OR REPLACE VIEW {qualified} AS\n{body}"),
        (false, false) => format!("CREATE VIEW {qualified} AS\n{body}"),
    }
}

fn drop_view(qualified: &str, view: &ViewSnapshot) -> String {
    if view.materialized {
        format!("DROP MATERIALIZED VIEW {qualified}")
    } else {
        format!("DROP VIEW {qualified}")
    }
}

/// Definitions compare without trailing semicolons and surrounding whitespace.
fn normalize_body(definition: &str) -> &str {
    definition.trim().trim_end_matches(';').trim_end()
}

/// Rewrites every reference to the source schema in the source snapshot to the target schema, so
/// two schemas of the same database compare by their content and the script targets the right
/// schema. Same-named schemas (two databases) are left unchanged.
fn requalify_snapshot(source: &SchemaSnapshot, target_schema: &str) -> SchemaSnapshot {
    if source.schema == target_schema {
        return source.clone();
    }
    let from = &source.schema;
    let rewrite = |text: &str| requalify(text, from, target_schema);
    SchemaSnapshot {
        schema: target_schema.to_string(),
        tables: source
            .tables
            .iter()
            .map(|(name, table)| {
                (
                    name.clone(),
                    TableSnapshot {
                        columns: table
                            .columns
                            .iter()
                            .map(|column| ColumnSnapshot {
                                data_type: rewrite(&column.data_type),
                                default: column.default.as_deref().map(rewrite),
                                generated: column.generated.as_deref().map(rewrite),
                                ..column.clone()
                            })
                            .collect(),
                        partition_key: table.partition_key.as_deref().map(rewrite),
                        constraints: table
                            .constraints
                            .iter()
                            .map(|(name, constraint)| {
                                (
                                    name.clone(),
                                    ConstraintSnapshot {
                                        kind: constraint.kind,
                                        definition: rewrite(&constraint.definition),
                                    },
                                )
                            })
                            .collect(),
                        indexes: table
                            .indexes
                            .iter()
                            .map(|(name, definition)| (name.clone(), rewrite(definition)))
                            .collect(),
                        triggers: table
                            .triggers
                            .iter()
                            .map(|(name, definition)| (name.clone(), rewrite(definition)))
                            .collect(),
                    },
                )
            })
            .collect(),
        views: source
            .views
            .iter()
            .map(|(name, view)| {
                (
                    name.clone(),
                    ViewSnapshot {
                        materialized: view.materialized,
                        definition: rewrite(&view.definition),
                    },
                )
            })
            .collect(),
        routines: source
            .routines
            .iter()
            .map(|(key, routine)| {
                (
                    key.clone(),
                    RoutineSnapshot {
                        identity_arguments: rewrite(&routine.identity_arguments),
                        definition: rewrite(&routine.definition),
                        ..routine.clone()
                    },
                )
            })
            .collect(),
        sequences: source.sequences.clone(),
    }
}

/// Replaces the qualifier `from.` with `to.` where it starts an identifier, as PostgreSQL prints it
/// (`quote_ident` form). Inside quoted literals such as `nextval('src.seq'::regclass)` the
/// qualifier is rewritten too, which keeps sequence defaults pointing at the target schema.
pub fn requalify(text: &str, from: &str, to: &str) -> String {
    let needle = format!("{}.", quote_ident(from));
    let replacement = format!("{}.", quote_ident(to));
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find(&needle) {
        output.push_str(&rest[..position]);
        // `mysrc.` or `x.src.` is another name that merely ends with the schema's.
        let inside_other_name = output.chars().last().is_some_and(|character| {
            character.is_alphanumeric() || matches!(character, '_' | '$' | '.')
        });
        output.push_str(if inside_other_name {
            &needle
        } else {
            &replacement
        });
        rest = &rest[position + needle.len()..];
    }
    output.push_str(rest);
    output
}

/// PostgreSQL's `quote_ident`: plain lowercase identifiers stay bare.
pub fn quote_ident(identifier: &str) -> String {
    let plain = identifier
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
        && identifier.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '$'
        })
        && !is_reserved(identifier);
    if plain {
        identifier.to_string()
    } else {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }
}

/// Reserved key words that `quote_ident` always quotes (PostgreSQL's reserved list).
fn is_reserved(identifier: &str) -> bool {
    const RESERVED: &[&str] = &[
        "all",
        "analyse",
        "analyze",
        "and",
        "any",
        "array",
        "as",
        "asc",
        "asymmetric",
        "both",
        "case",
        "cast",
        "check",
        "collate",
        "column",
        "constraint",
        "create",
        "current_catalog",
        "current_date",
        "current_role",
        "current_time",
        "current_timestamp",
        "current_user",
        "default",
        "deferrable",
        "desc",
        "distinct",
        "do",
        "else",
        "end",
        "except",
        "false",
        "fetch",
        "for",
        "foreign",
        "from",
        "grant",
        "group",
        "having",
        "in",
        "initially",
        "intersect",
        "into",
        "lateral",
        "leading",
        "limit",
        "localtime",
        "localtimestamp",
        "not",
        "null",
        "offset",
        "on",
        "only",
        "or",
        "order",
        "placing",
        "primary",
        "references",
        "returning",
        "select",
        "session_user",
        "some",
        "symmetric",
        "system_user",
        "table",
        "then",
        "to",
        "trailing",
        "true",
        "union",
        "unique",
        "user",
        "using",
        "variadic",
        "when",
        "where",
        "window",
        "with",
    ];
    RESERVED.contains(&identifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, data_type: &str, not_null: bool) -> ColumnSnapshot {
        ColumnSnapshot {
            name: name.into(),
            data_type: data_type.into(),
            not_null,
            ..ColumnSnapshot::default()
        }
    }

    fn snapshot(schema: &str) -> SchemaSnapshot {
        SchemaSnapshot {
            schema: schema.into(),
            ..SchemaSnapshot::default()
        }
    }

    #[test]
    fn quote_ident_matches_postgresql() {
        assert_eq!(quote_ident("public"), "public");
        assert_eq!(quote_ident("Sales"), "\"Sales\"");
        assert_eq!(quote_ident("my schema"), "\"my schema\"");
        assert_eq!(quote_ident("order"), "\"order\"");
        assert_eq!(quote_ident("1abc"), "\"1abc\"");
        assert_eq!(quote_ident("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn requalify_only_touches_the_schema_qualifier() {
        assert_eq!(
            requalify("REFERENCES src.users(id)", "src", "dst"),
            "REFERENCES dst.users(id)"
        );
        assert_eq!(
            requalify("nextval('src.items_id_seq'::regclass)", "src", "dst"),
            "nextval('dst.items_id_seq'::regclass)"
        );
        assert_eq!(
            requalify("mysrc.t, src_x.t", "src", "dst"),
            "mysrc.t, src_x.t"
        );
        assert_eq!(requalify("ON \"Src\".t", "Src", "public"), "ON public.t");
    }

    #[test]
    fn identical_schemas_have_no_differences() {
        let mut source = snapshot("public");
        source.tables.insert(
            "t".into(),
            TableSnapshot {
                columns: vec![column("id", "integer", true)],
                ..TableSnapshot::default()
            },
        );
        let diff = diff_schemas(&source, &source.clone());
        assert!(diff.objects.is_empty());
        assert!(diff.script.contains("-- No differences"));
        assert!(!diff.destructive);
    }

    #[test]
    fn new_tables_are_created_with_foreign_keys_after_all_tables() {
        let mut source = snapshot("src");
        source.tables.insert(
            "orders".into(),
            TableSnapshot {
                columns: vec![
                    column("id", "integer", true),
                    column("user_id", "integer", false),
                ],
                constraints: BTreeMap::from([
                    (
                        "orders_pkey".into(),
                        ConstraintSnapshot {
                            kind: 'p',
                            definition: "PRIMARY KEY (id)".into(),
                        },
                    ),
                    (
                        "orders_user_fk".into(),
                        ConstraintSnapshot {
                            kind: 'f',
                            definition: "FOREIGN KEY (user_id) REFERENCES src.users(id)".into(),
                        },
                    ),
                ]),
                indexes: BTreeMap::from([(
                    "orders_user_idx".into(),
                    "CREATE INDEX orders_user_idx ON src.orders USING btree (user_id)".into(),
                )]),
                ..TableSnapshot::default()
            },
        );
        source.tables.insert(
            "users".into(),
            TableSnapshot {
                columns: vec![column("id", "integer", true)],
                constraints: BTreeMap::from([(
                    "users_pkey".into(),
                    ConstraintSnapshot {
                        kind: 'p',
                        definition: "PRIMARY KEY (id)".into(),
                    },
                )]),
                ..TableSnapshot::default()
            },
        );
        let diff = diff_schemas(&source, &snapshot("dst"));
        assert_eq!(diff.objects.len(), 2);
        assert!(diff
            .objects
            .iter()
            .all(|object| object.status == DiffStatus::Added));
        let script = &diff.script;
        let users = script.find("CREATE TABLE dst.users").unwrap();
        let fk = script
            .find("ADD CONSTRAINT orders_user_fk FOREIGN KEY (user_id) REFERENCES dst.users(id)")
            .unwrap();
        assert!(users < fk, "foreign keys come after every CREATE TABLE");
        assert!(
            script.contains("CREATE INDEX orders_user_idx ON dst.orders USING btree (user_id);")
        );
        assert!(!script.contains("src."));
    }

    #[test]
    fn changed_tables_alter_columns_constraints_indexes_and_triggers() {
        let mut source = snapshot("public");
        let mut target = snapshot("public");
        source.tables.insert(
            "t".into(),
            TableSnapshot {
                columns: vec![
                    column("id", "integer", true),
                    ColumnSnapshot {
                        default: Some("'x'::text".into()),
                        ..column("name", "text", true)
                    },
                    column("added", "date", false),
                ],
                constraints: BTreeMap::from([(
                    "t_name_check".into(),
                    ConstraintSnapshot { kind: 'c', definition: "CHECK ((length(name) > 1))".into() },
                )]),
                indexes: BTreeMap::from([("t_name_idx".into(), "CREATE INDEX t_name_idx ON public.t USING btree (name)".into())]),
                triggers: BTreeMap::from([("t_audit".into(), "CREATE TRIGGER t_audit AFTER INSERT ON public.t FOR EACH ROW EXECUTE FUNCTION public.audit()".into())]),
                ..TableSnapshot::default()
            },
        );
        target.tables.insert(
            "t".into(),
            TableSnapshot {
                columns: vec![
                    column("id", "bigint", true),
                    column("name", "text", false),
                    column("legacy", "text", false),
                ],
                indexes: BTreeMap::from([(
                    "t_name_idx".into(),
                    "CREATE INDEX t_name_idx ON public.t USING hash (name)".into(),
                )]),
                ..TableSnapshot::default()
            },
        );
        let diff = diff_schemas(&source, &target);
        let table = &diff.objects[0];
        assert_eq!(table.status, DiffStatus::Changed);
        assert!(table.destructive);
        let statements = table.statements.join("\n");
        for expected in [
            "ALTER TABLE public.t ALTER COLUMN id TYPE integer USING id::integer",
            "ALTER TABLE public.t ALTER COLUMN name SET DEFAULT 'x'::text",
            "ALTER TABLE public.t ALTER COLUMN name SET NOT NULL",
            "ALTER TABLE public.t ADD COLUMN added date",
            "ALTER TABLE public.t DROP COLUMN legacy",
            "ALTER TABLE public.t ADD CONSTRAINT t_name_check CHECK ((length(name) > 1))",
            "DROP INDEX public.t_name_idx",
            "CREATE INDEX t_name_idx ON public.t USING btree (name)",
            "CREATE TRIGGER t_audit AFTER INSERT ON public.t",
        ] {
            assert!(
                statements.contains(expected),
                "missing {expected}\n{statements}"
            );
        }
        assert!(table
            .source_ddl
            .as_ref()
            .unwrap()
            .contains("CREATE TABLE public.t ("));
    }

    #[test]
    fn removed_objects_are_dropped_last() {
        let source = snapshot("public");
        let mut target = snapshot("public");
        target.tables.insert(
            "old".into(),
            TableSnapshot {
                columns: vec![column("id", "integer", true)],
                ..TableSnapshot::default()
            },
        );
        target.sequences.insert(
            "s".into(),
            SequenceSnapshot {
                data_type: "bigint".into(),
                start: "1".into(),
                increment: "1".into(),
                min: "1".into(),
                max: "9223372036854775807".into(),
                cache: "1".into(),
                cycle: false,
            },
        );
        target.routines.insert("f(integer)".into(), RoutineSnapshot { name: "f".into(), identity_arguments: "integer".into(), procedure: false, definition: "CREATE OR REPLACE FUNCTION public.f(integer)\n RETURNS integer\n LANGUAGE sql\nAS $function$ SELECT 1 $function$\n".into() });
        target.views.insert(
            "v".into(),
            ViewSnapshot {
                materialized: false,
                definition: " SELECT 1 AS one;".into(),
            },
        );
        let diff = diff_schemas(&source, &target);
        assert!(diff.destructive);
        let script = &diff.script;
        let view = script.find("DROP VIEW public.v;").unwrap();
        let function = script.find("DROP FUNCTION public.f(integer);").unwrap();
        let table = script.find("DROP TABLE public.old;").unwrap();
        let sequence = script.find("DROP SEQUENCE public.s;").unwrap();
        assert!(view < function && function < table && table < sequence);
    }

    #[test]
    fn views_and_routines_are_replaced_and_sequences_altered() {
        let mut source = snapshot("src");
        let mut target = snapshot("dst");
        source.views.insert(
            "v".into(),
            ViewSnapshot {
                materialized: false,
                definition: " SELECT t.id\n   FROM src.t;".into(),
            },
        );
        target.views.insert(
            "v".into(),
            ViewSnapshot {
                materialized: false,
                definition: " SELECT t.id, t.name\n   FROM dst.t;".into(),
            },
        );
        source.routines.insert("f()".into(), RoutineSnapshot { name: "f".into(), identity_arguments: String::new(), procedure: false, definition: "CREATE OR REPLACE FUNCTION src.f()\n RETURNS integer\n LANGUAGE sql\nAS $function$ SELECT 2 $function$\n".into() });
        target.routines.insert("f()".into(), RoutineSnapshot { name: "f".into(), identity_arguments: String::new(), procedure: false, definition: "CREATE OR REPLACE FUNCTION dst.f()\n RETURNS integer\n LANGUAGE sql\nAS $function$ SELECT 1 $function$\n".into() });
        let base = SequenceSnapshot {
            data_type: "bigint".into(),
            start: "1".into(),
            increment: "1".into(),
            min: "1".into(),
            max: "100".into(),
            cache: "1".into(),
            cycle: false,
        };
        source.sequences.insert(
            "s".into(),
            SequenceSnapshot {
                increment: "5".into(),
                ..base.clone()
            },
        );
        target.sequences.insert("s".into(), base);
        // An unchanged view in another schema compares equal once requalified.
        source.views.insert(
            "same".into(),
            ViewSnapshot {
                materialized: false,
                definition: " SELECT 1 FROM src.t;".into(),
            },
        );
        target.views.insert(
            "same".into(),
            ViewSnapshot {
                materialized: false,
                definition: " SELECT 1 FROM dst.t;".into(),
            },
        );
        let diff = diff_schemas(&source, &target);
        assert_eq!(diff.objects.len(), 3, "{:#?}", diff.objects);
        let script = &diff.script;
        assert!(script.contains("CREATE OR REPLACE VIEW dst.v AS\nSELECT t.id\n   FROM dst.t;"));
        assert!(script.contains("CREATE OR REPLACE FUNCTION dst.f()"));
        assert!(script.contains("ALTER SEQUENCE dst.s AS bigint INCREMENT BY 5"));
        assert!(!script.contains("src."));
    }
}
