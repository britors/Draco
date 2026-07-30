use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;
use super::introspection::TableDetailColumn;

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
    pub removed: bool,
}

/// Diffs the table's current columns/name against the edited form and returns the ordered
/// `ALTER TABLE` statements needed to get there. Pure and DB-free so it's unit-testable; the
/// caller runs the result through [`alter_table`] inside one transaction.
pub fn build_alter_table_statements(schema: &str, table: &str, new_table_name: &str, original: &[TableDetailColumn], edits: &[ColumnEdit]) -> Vec<String> {
    let q_schema = quote_ident(schema);
    let q_table = quote_ident(table);
    let mut statements = Vec::new();

    for edit in edits {
        let Some(orig_name) = &edit.original_name else {
            if edit.removed {
                continue;
            }
            let mut sql = format!("ALTER TABLE {q_schema}.{q_table} ADD COLUMN {} {}", quote_ident(&edit.name), edit.data_type);
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
            statements.push(format!("ALTER TABLE {q_schema}.{q_table} DROP COLUMN {}", quote_ident(orig_name)));
            continue;
        }

        let Some(current) = original.iter().find(|c| &c.name == orig_name) else { continue };

        if &edit.name != orig_name {
            statements.push(format!("ALTER TABLE {q_schema}.{q_table} RENAME COLUMN {} TO {}", quote_ident(orig_name), quote_ident(&edit.name)));
        }
        let q_col = quote_ident(&edit.name);

        if current.full_type != edit.data_type {
            statements.push(format!("ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} TYPE {} USING {q_col}::{}", edit.data_type, edit.data_type));
        }

        if current.is_nullable != edit.nullable {
            let clause = if edit.nullable { "DROP NOT NULL" } else { "SET NOT NULL" };
            statements.push(format!("ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} {clause}"));
        }

        let current_default = current.column_default.as_deref().unwrap_or("").trim();
        let new_default = edit.default.as_deref().unwrap_or("").trim();
        if current_default != new_default {
            if new_default.is_empty() {
                statements.push(format!("ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} DROP DEFAULT"));
            } else {
                statements.push(format!("ALTER TABLE {q_schema}.{q_table} ALTER COLUMN {q_col} SET DEFAULT {new_default}"));
            }
        }
    }

    if new_table_name.trim() != table {
        statements.push(format!("ALTER TABLE {q_schema}.{q_table} RENAME TO {}", quote_ident(new_table_name.trim())));
    }

    statements
}

/// Whether `statements` (as produced by [`build_alter_table_statements`]) contains anything that
/// can lose data (`DROP COLUMN`) or fail/lossily coerce existing rows (`ALTER COLUMN ... TYPE`) —
/// used to decide whether the UI must show an explicit confirmation before applying.
pub fn alter_table_is_destructive(statements: &[String]) -> bool {
    statements.iter().any(|s| s.contains("DROP COLUMN") || s.contains(" TYPE "))
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

pub async fn run_vacuum(driver: &PostgresDriver, schema: &str, table: &str, op: &str) -> Result<()> {
    if !ALLOWED_VACUUM_OPS.contains(&op) {
        return Err(crate::error::CoreError::Other(format!("unsupported operation: {op}")));
    }
    driver
        .query(&format!("{op} {}.{}", quote_ident(schema), quote_ident(table)), &[])
        .await?;
    Ok(())
}

pub async fn reindex_table(driver: &PostgresDriver, schema: &str, table: &str) -> Result<()> {
    driver
        .query(&format!("REINDEX TABLE {}.{}", quote_ident(schema), quote_ident(table)), &[])
        .await?;
    Ok(())
}


#[cfg(test)]
mod alter_table_tests {
    use super::*;

    fn col(name: &str, full_type: &str, nullable: bool, default: Option<&str>) -> TableDetailColumn {
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
            removed: false,
        }
    }

    #[test]
    fn no_changes_produces_no_statements() {
        let original = vec![col("id", "int4", false, None)];
        let edits = vec![unchanged_edit(&original[0])];
        assert!(build_alter_table_statements("public", "t", "t", &original, &edits).is_empty());
    }

    #[test]
    fn detects_rename_type_nullable_and_default_changes() {
        let original = vec![col("name", "varchar(50)", false, None)];
        let mut edit = unchanged_edit(&original[0]);
        edit.name = "full_name".to_string();
        edit.data_type = "text".to_string();
        edit.nullable = true;
        edit.default = Some("'unknown'".to_string());

        let statements = build_alter_table_statements("public", "t", "t", &original, &[edit]);
        assert!(statements.iter().any(|s| s.contains("RENAME COLUMN \"name\" TO \"full_name\"")));
        assert!(statements.iter().any(|s| s.contains("TYPE text")));
        assert!(statements.iter().any(|s| s.contains("DROP NOT NULL")));
        assert!(statements.iter().any(|s| s.contains("SET DEFAULT 'unknown'")));
    }

    #[test]
    fn new_column_emits_add_column() {
        let edit = ColumnEdit {
            original_name: None,
            name: "created_at".to_string(),
            data_type: "timestamptz".to_string(),
            nullable: false,
            default: Some("now()".to_string()),
            removed: false,
        };
        let statements = build_alter_table_statements("public", "t", "t", &[], &[edit]);
        assert_eq!(statements, vec!["ALTER TABLE \"public\".\"t\" ADD COLUMN \"created_at\" timestamptz NOT NULL DEFAULT now()".to_string()]);
    }

    #[test]
    fn removed_existing_column_emits_drop_column() {
        let original = vec![col("legacy", "text", true, None)];
        let mut edit = unchanged_edit(&original[0]);
        edit.removed = true;
        let statements = build_alter_table_statements("public", "t", "t", &original, &[edit]);
        assert_eq!(statements, vec!["ALTER TABLE \"public\".\"t\" DROP COLUMN \"legacy\"".to_string()]);
    }

    #[test]
    fn removed_new_column_emits_nothing() {
        let edit = ColumnEdit { original_name: None, name: "x".to_string(), data_type: "text".to_string(), nullable: true, default: None, removed: true };
        assert!(build_alter_table_statements("public", "t", "t", &[], &[edit]).is_empty());
    }

    #[test]
    fn table_rename_emits_rename_to() {
        let statements = build_alter_table_statements("public", "old_name", "new_name", &[], &[]);
        assert_eq!(statements, vec!["ALTER TABLE \"public\".\"old_name\" RENAME TO \"new_name\"".to_string()]);
    }

    #[test]
    fn destructive_detection() {
        assert!(alter_table_is_destructive(&["ALTER TABLE \"public\".\"t\" DROP COLUMN \"x\"".to_string()]));
        assert!(alter_table_is_destructive(&["ALTER TABLE \"public\".\"t\" ALTER COLUMN \"x\" TYPE text USING \"x\"::text".to_string()]));
        assert!(!alter_table_is_destructive(&["ALTER TABLE \"public\".\"t\" RENAME TO \"y\"".to_string()]));
    }
}
