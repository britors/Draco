use std::collections::HashSet;

use super::helpers::*;
use super::introspection::TableDetailColumn;
use crate::error::{CoreError, Result};
use crate::postgres::pool::PostgresDriver;

/// Desired end-state of one column as edited in the table editor form. `original_name` is
/// `None` for a column added in this session, and identifies the column being changed
/// otherwise (its current name, even if `name` is being changed to something else).
#[derive(Debug, Clone)]
pub struct ColumnEdit {
    pub original_name: Option<String>,
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub primary_key: bool,
    pub removed: bool,
}

#[derive(Debug, Clone)]
pub struct NewColumnReference {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub on_delete: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewTableColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub default: Option<String>,
    pub references: Option<NewColumnReference>,
}

pub fn build_create_table_statement(
    schema: &str,
    table: &str,
    columns: &[NewTableColumn],
) -> Result<String> {
    validate_identifier(schema, "schema")?;
    validate_identifier(table, "table")?;
    if columns.is_empty() {
        return Err(CoreError::Other(
            "at least one table column is required".to_string(),
        ));
    }
    let mut seen = HashSet::with_capacity(columns.len());
    let mut definitions = Vec::with_capacity(columns.len() + 1);
    let mut primary_key = Vec::new();
    for column in columns {
        validate_identifier(&column.name, "column")?;
        if !seen.insert(column.name.clone()) {
            return Err(CoreError::Other(format!(
                "duplicate table column: {}",
                column.name
            )));
        }
        validate_sql_fragment(&column.data_type, "column type")?;
        let mut definition = format!("{} {}", quote_ident(&column.name), column.data_type.trim());
        if !column.nullable {
            definition.push_str(" NOT NULL");
        }
        if column.unique {
            definition.push_str(" UNIQUE");
        }
        if let Some(default) = column
            .default
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            validate_sql_fragment(default, "column default")?;
            definition.push_str(" DEFAULT ");
            definition.push_str(default.trim());
        }
        if let Some(reference) = &column.references {
            validate_identifier(&reference.schema, "referenced schema")?;
            validate_identifier(&reference.table, "referenced table")?;
            validate_identifier(&reference.column, "referenced column")?;
            definition.push_str(&format!(
                " REFERENCES {}.{}({})",
                quote_ident(&reference.schema),
                quote_ident(&reference.table),
                quote_ident(&reference.column)
            ));
            if let Some(action) = reference
                .on_delete
                .as_deref()
                .map(str::trim)
                .filter(|action| !action.is_empty())
            {
                let action = action.to_ascii_uppercase();
                if !matches!(
                    action.as_str(),
                    "NO ACTION" | "RESTRICT" | "CASCADE" | "SET NULL" | "SET DEFAULT"
                ) {
                    return Err(CoreError::Other(
                        "invalid foreign-key delete action".to_string(),
                    ));
                }
                definition.push_str(" ON DELETE ");
                definition.push_str(&action);
            }
        }
        if column.primary_key {
            primary_key.push(quote_ident(&column.name));
        }
        definitions.push(definition);
    }
    if !primary_key.is_empty() {
        definitions.push(format!("PRIMARY KEY ({})", primary_key.join(", ")));
    }
    Ok(format!(
        "CREATE TABLE {}.{} (\n  {}\n)",
        quote_ident(schema),
        quote_ident(table),
        definitions.join(",\n  ")
    ))
}

pub async fn create_table(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    columns: &[NewTableColumn],
) -> Result<()> {
    let statement = build_create_table_statement(schema, table, columns)?;
    driver.batch_execute(&statement).await
}

fn validate_identifier(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\0') || value.chars().any(char::is_control) {
        return Err(CoreError::Other(format!("{label} name is invalid")));
    }
    if value.len() > 63 {
        return Err(CoreError::Other(format!(
            "{label} name must be at most 63 bytes"
        )));
    }
    Ok(())
}

fn validate_sql_fragment(value: &str, label: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 1000
        || value.contains('\0')
        || value.contains(';')
        || value.contains("--")
        || value.contains("/*")
        || value.contains("*/")
    {
        return Err(CoreError::Other(format!("{label} is invalid")));
    }
    Ok(())
}

pub fn validate_table_column_definition(
    name: &str,
    data_type: &str,
    default: Option<&str>,
) -> Result<()> {
    validate_identifier(name, "column")?;
    validate_sql_fragment(data_type, "column type")?;
    if let Some(default) = default.filter(|value| !value.trim().is_empty()) {
        validate_sql_fragment(default, "column default")?;
    }
    Ok(())
}

/// Diffs the table's current columns/name against the edited form and returns the ordered
/// `ALTER TABLE` statements needed to get there. Pure and DB-free so it's unit-testable; the
/// caller runs the result through [`alter_table`] inside one transaction.
pub fn build_alter_table_statements(
    schema: &str,
    table: &str,
    new_table_name: &str,
    original: &[TableDetailColumn],
    edits: &[ColumnEdit],
    primary_key_constraint: Option<&str>,
) -> Vec<String> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let mut statements = Vec::new();
    let original_primary_key = original
        .iter()
        .filter(|column| column.is_primary_key)
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    let edited_primary_key = edits
        .iter()
        .filter(|column| column.primary_key && !column.removed)
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    let primary_key_unchanged = original_primary_key.len() == edited_primary_key.len()
        && original_primary_key.iter().zip(&edited_primary_key).all(
            |(original_name, edited_name)| {
                edits.iter().any(|edit| {
                    edit.original_name.as_deref() == Some(*original_name)
                        && edit.name == **edited_name
                        && !edit.removed
                })
            },
        );
    if !primary_key_unchanged {
        if let Some(constraint) = primary_key_constraint {
            statements.push(format!(
                "ALTER TABLE {q_schema}.{q_table} DROP CONSTRAINT {}",
                quote_ident(constraint)
            ));
        }
    }

    for edit in edits {
        let Some(orig_name) = &edit.original_name else {
            if edit.removed {
                continue;
            }
            let mut sql = format!(
                "ALTER TABLE {q_schema}.{q_table} ADD COLUMN {} {}",
                quote_ident(&edit.name),
                edit.data_type
            );
            if !edit.nullable {
                sql.push_str(" NOT NULL");
            }
            if let Some(default) = edit.default.as_deref().filter(|d| !d.trim().is_empty()) {
                sql.push_str(&format!(" DEFAULT {default}"));
            }
            statements.push(sql);
            continue;
        };

        if edit.removed {
            statements.push(format!(
                "ALTER TABLE {q_schema}.{q_table} DROP COLUMN {}",
                quote_ident(orig_name)
            ));
            continue;
        }

        let Some(current) = original.iter().find(|c| &c.name == orig_name) else {
            continue;
        };

        if &edit.name != orig_name {
            statements.push(format!(
                "ALTER TABLE {q_schema}.{q_table} RENAME COLUMN {} TO {}",
                quote_ident(orig_name),
                quote_ident(&edit.name)
            ));
        }
        let q_col = quote_ident(&edit.name);

        if current.full_type != edit.data_type {
            statements.push(format!(
                "ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} TYPE {} USING {q_col}::{}",
                edit.data_type, edit.data_type
            ));
        }

        if current.is_nullable != edit.nullable {
            let clause = if edit.nullable {
                "DROP NOT NULL"
            } else {
                "SET NOT NULL"
            };
            statements.push(format!(
                "ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} {clause}"
            ));
        }

        let current_default = current.column_default.as_deref().unwrap_or("").trim();
        let new_default = edit.default.as_deref().unwrap_or("").trim();
        if current_default != new_default {
            if new_default.is_empty() {
                statements.push(format!(
                    "ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} DROP DEFAULT"
                ));
            } else {
                statements.push(format!("ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} SET DEFAULT {new_default}"));
            }
        }
    }

    if !primary_key_unchanged && !edited_primary_key.is_empty() {
        statements.push(format!(
            "ALTER TABLE {q_schema}.{q_table} ADD PRIMARY KEY ({})",
            edited_primary_key
                .iter()
                .map(|column| quote_ident(column))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if new_table_name.trim() != table {
        statements.push(format!(
            "ALTER TABLE {q_schema}.{q_table} RENAME TO {}",
            quote_ident(new_table_name.trim())
        ));
    }

    statements
}

/// Whether `statements` (as produced by [`build_alter_table_statements`]) contains anything that
/// can lose data (`DROP COLUMN`) or fail/lossily coerce existing rows (`ALTER COLUMN ... TYPE`) —
/// used to decide whether the UI must show an explicit confirmation before applying.
pub fn alter_table_is_destructive(statements: &[String]) -> bool {
    statements.iter().any(|s| {
        s.contains("DROP COLUMN")
            || s.contains(" TYPE ")
            || s.contains("DROP CONSTRAINT")
            || s.contains("ADD PRIMARY KEY")
    })
}

/// Applies `statements` atomically (`BEGIN`/`COMMIT` around a batch execute) so a mid-way
/// failure leaves the table untouched.
pub async fn alter_table(driver: &PostgresDriver, statements: &[String]) -> Result<()> {
    if statements.is_empty() {
        return Ok(());
    }
    let batch = format!("BEGIN;\n{};\nCOMMIT;", statements.join(";\n"));
    driver.batch_execute(&batch).await
}

const ALLOWED_VACUUM_OPS: &[&str] = &["VACUUM", "ANALYZE", "VACUUM ANALYZE", "VACUUM FULL"];

pub async fn run_vacuum(
    driver: &PostgresDriver,
    schema: &str,
    table: &str,
    op: &str,
) -> Result<()> {
    if !ALLOWED_VACUUM_OPS.contains(&op) {
        return Err(crate::error::CoreError::Other(format!(
            "unsupported operation: {op}"
        )));
    }
    driver
        .query(
            &format!("{op} {}.{}", quote_ident(schema), quote_ident(table)),
            &[],
        )
        .await?;
    Ok(())
}

pub async fn reindex_table(driver: &PostgresDriver, schema: &str, table: &str) -> Result<()> {
    driver
        .query(
            &format!(
                "REINDEX TABLE {}.{}",
                quote_ident(schema),
                quote_ident(table)
            ),
            &[],
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod alter_table_tests {
    use super::*;

    fn col(
        name: &str,
        full_type: &str,
        nullable: bool,
        default: Option<&str>,
    ) -> TableDetailColumn {
        TableDetailColumn {
            name: name.to_string(),
            data_type: full_type.to_string(),
            full_type: full_type.to_string(),
            is_nullable: nullable,
            column_default: default.map(str::to_string),
            is_primary_key: false,
            is_foreign_key: false,
            ordinal_position: 1,
        }
    }

    fn unchanged_edit(c: &TableDetailColumn) -> ColumnEdit {
        ColumnEdit {
            original_name: Some(c.name.clone()),
            name: c.name.clone(),
            data_type: c.full_type.clone(),
            nullable: c.is_nullable,
            default: c.column_default.clone(),
            primary_key: c.is_primary_key,
            removed: false,
        }
    }

    #[test]
    fn no_changes_produces_no_statements() {
        let original = vec![col("id", "int4", false, None)];
        let edits = vec![unchanged_edit(&original[0])];
        assert!(
            build_alter_table_statements("public", "t", "t", &original, &edits, None).is_empty()
        );
    }

    #[test]
    fn detects_rename_type_nullable_and_default_changes() {
        let original = vec![col("name", "varchar(50)", false, None)];
        let mut edit = unchanged_edit(&original[0]);
        edit.name = "full_name".to_string();
        edit.data_type = "text".to_string();
        edit.nullable = true;
        edit.default = Some("'unknown'".to_string());

        let statements = build_alter_table_statements("public", "t", "t", &original, &[edit], None);
        assert!(statements
            .iter()
            .any(|s| s.contains("RENAME COLUMN \"name\" TO \"full_name\"")));
        assert!(statements.iter().any(|s| s.contains("TYPE text")));
        assert!(statements.iter().any(|s| s.contains("DROP NOT NULL")));
        assert!(statements
            .iter()
            .any(|s| s.contains("SET DEFAULT 'unknown'")));
    }

    #[test]
    fn new_column_emits_add_column() {
        let edit = ColumnEdit {
            original_name: None,
            name: "created_at".to_string(),
            data_type: "timestamptz".to_string(),
            nullable: false,
            default: Some("now()".to_string()),
            primary_key: false,
            removed: false,
        };
        let statements = build_alter_table_statements("public", "t", "t", &[], &[edit], None);
        assert_eq!(statements, vec!["ALTER TABLE \"public\".\"t\" ADD COLUMN \"created_at\" timestamptz NOT NULL DEFAULT now()".to_string()]);
    }

    #[test]
    fn removed_existing_column_emits_drop_column() {
        let original = vec![col("legacy", "text", true, None)];
        let mut edit = unchanged_edit(&original[0]);
        edit.removed = true;
        let statements = build_alter_table_statements("public", "t", "t", &original, &[edit], None);
        assert_eq!(
            statements,
            vec!["ALTER TABLE \"public\".\"t\" DROP COLUMN \"legacy\"".to_string()]
        );
    }

    #[test]
    fn removed_new_column_emits_nothing() {
        let edit = ColumnEdit {
            original_name: None,
            name: "x".to_string(),
            data_type: "text".to_string(),
            nullable: true,
            default: None,
            primary_key: false,
            removed: true,
        };
        assert!(build_alter_table_statements("public", "t", "t", &[], &[edit], None).is_empty());
    }

    #[test]
    fn table_rename_emits_rename_to() {
        let statements =
            build_alter_table_statements("public", "old_name", "new_name", &[], &[], None);
        assert_eq!(
            statements,
            vec!["ALTER TABLE \"public\".\"old_name\" RENAME TO \"new_name\"".to_string()]
        );
    }

    #[test]
    fn destructive_detection() {
        assert!(alter_table_is_destructive(&[
            "ALTER TABLE \"public\".\"t\" DROP COLUMN \"x\"".to_string()
        ]));
        assert!(alter_table_is_destructive(&[
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"x\" TYPE text USING \"x\"::text"
                .to_string()
        ]));
        assert!(!alter_table_is_destructive(&[
            "ALTER TABLE \"public\".\"t\" RENAME TO \"y\"".to_string()
        ]));
    }

    #[test]
    fn primary_key_can_be_replaced_or_added() {
        let mut original = vec![
            col("id", "int4", false, None),
            col("code", "text", false, None),
        ];
        original[0].is_primary_key = true;
        let mut edits = original.iter().map(unchanged_edit).collect::<Vec<_>>();
        edits[0].primary_key = false;
        edits[1].primary_key = true;
        let statements = build_alter_table_statements(
            "public",
            "items",
            "items",
            &original,
            &edits,
            Some("items_pkey"),
        );
        assert!(statements
            .iter()
            .any(|sql| sql.contains("DROP CONSTRAINT \"items_pkey\"")));
        assert!(statements
            .iter()
            .any(|sql| sql.contains("ADD PRIMARY KEY (\"code\")")));
        assert!(statements[0].contains("DROP CONSTRAINT"));
    }

    #[test]
    fn primary_key_constraint_is_dropped_before_its_column() {
        let mut original = vec![col("id", "int4", false, None)];
        original[0].is_primary_key = true;
        let mut edit = unchanged_edit(&original[0]);
        edit.removed = true;
        edit.primary_key = false;
        let statements = build_alter_table_statements(
            "public",
            "items",
            "items",
            &original,
            &[edit],
            Some("items_pkey"),
        );
        assert!(statements[0].contains("DROP CONSTRAINT"));
        assert!(statements[1].contains("DROP COLUMN"));
    }

    #[test]
    fn create_table_builder_quotes_names_and_supports_inline_foreign_keys() {
        let sql = build_create_table_statement(
            "app",
            "orders",
            &[
                NewTableColumn {
                    name: "id".to_string(),
                    data_type: "bigint".to_string(),
                    nullable: false,
                    primary_key: true,
                    unique: false,
                    default: None,
                    references: None,
                },
                NewTableColumn {
                    name: "account_id".to_string(),
                    data_type: "bigint".to_string(),
                    nullable: false,
                    primary_key: false,
                    unique: false,
                    default: None,
                    references: Some(NewColumnReference {
                        schema: "public".to_string(),
                        table: "accounts".to_string(),
                        column: "id".to_string(),
                        on_delete: Some("cascade".to_string()),
                    }),
                },
            ],
        )
        .expect("create SQL");
        assert!(sql.contains("CREATE TABLE \"app\".\"orders\""));
        assert!(sql.contains("REFERENCES \"public\".\"accounts\"(\"id\") ON DELETE CASCADE"));
        assert!(sql.contains("PRIMARY KEY (\"id\")"));
    }

    #[test]
    fn create_table_builder_rejects_stacked_fragments_and_duplicates() {
        let column = NewTableColumn {
            name: "id".to_string(),
            data_type: "integer; DROP TABLE users".to_string(),
            nullable: false,
            primary_key: false,
            unique: false,
            default: None,
            references: None,
        };
        assert!(build_create_table_statement("public", "items", &[column]).is_err());
    }
}
