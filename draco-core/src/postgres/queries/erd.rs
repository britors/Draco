use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;
use super::introspection::TableKind;

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
