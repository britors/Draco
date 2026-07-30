//! Row-decoding and SQL-identifier helpers shared by every `queries` submodule.

use tokio_postgres::Row;

pub(super) fn get_str(row: &Row, col: &str) -> String {
    row.try_get::<_, String>(col).unwrap_or_default()
}

pub(super) fn get_opt_str(row: &Row, col: &str) -> Option<String> {
    row.try_get::<_, Option<String>>(col).ok().flatten()
}

pub(super) fn get_bool(row: &Row, col: &str) -> bool {
    row.try_get::<_, bool>(col).unwrap_or(false)
}

pub(super) fn get_i64(row: &Row, col: &str) -> i64 {
    row.try_get::<_, i64>(col).unwrap_or(0)
}

pub(super) fn get_i32(row: &Row, col: &str) -> i32 {
    row.try_get::<_, i32>(col).unwrap_or(0)
}

pub(super) fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

pub(super) fn row_to_json_map(row: &Row) -> serde_json::Map<String, serde_json::Value> {
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
