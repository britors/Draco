//! Row-decoding and SQL-identifier helpers shared by every `queries` submodule.

use tokio_postgres::types::Type;
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

/// Builds the editor's row map from `PostgresDriver::query_text` output. Booleans and integer
/// and float columns keep their JSON types, as with `row_to_json_map`; every other type keeps
/// PostgreSQL's text form, so `numeric` precision and timestamp offsets reach the UI unchanged.
pub(super) fn text_row_to_json_map(
    columns: &[(String, Type)],
    row: &[Option<String>],
) -> serde_json::Map<String, serde_json::Value> {
    columns
        .iter()
        .zip(row)
        .map(|((name, ty), value)| (name.clone(), text_value_to_json(ty, value.as_deref())))
        .collect()
}

fn text_value_to_json(ty: &Type, value: Option<&str>) -> serde_json::Value {
    let Some(text) = value else {
        return serde_json::Value::Null;
    };
    let typed = match *ty {
        Type::BOOL => match text {
            "t" => Some(serde_json::Value::Bool(true)),
            "f" => Some(serde_json::Value::Bool(false)),
            _ => None,
        },
        Type::INT2 | Type::INT4 | Type::INT8 => {
            text.parse::<i64>().ok().map(|v| serde_json::json!(v))
        }
        // NaN and ±Infinity have no JSON number, so they stay as PostgreSQL spells them.
        Type::FLOAT4 | Type::FLOAT8 => text
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|v| serde_json::json!(v)),
        _ => None,
    };
    typed.unwrap_or_else(|| serde_json::Value::String(text.to_string()))
}

fn pg_value_to_json(row: &Row, idx: usize) -> serde_json::Value {
    let ty = row.columns()[idx].type_();
    match *ty {
        Type::BOOL => row
            .try_get::<_, Option<bool>>(idx)
            .ok()
            .flatten()
            .map(serde_json::Value::Bool)
            .unwrap_or(serde_json::Value::Null),
        Type::INT2 => row
            .try_get::<_, Option<i16>>(idx)
            .ok()
            .flatten()
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(idx)
            .ok()
            .flatten()
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(idx)
            .ok()
            .flatten()
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(idx)
            .ok()
            .flatten()
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        Type::FLOAT8 => row
            .try_get::<_, Option<f64>>(idx)
            .ok()
            .flatten()
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        _ => row
            .try_get::<_, Option<String>>(idx)
            .ok()
            .flatten()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn text_values_keep_native_json_types_only_where_lossless() {
        assert_eq!(text_value_to_json(&Type::BOOL, Some("t")), json!(true));
        assert_eq!(text_value_to_json(&Type::BOOL, Some("f")), json!(false));
        assert_eq!(text_value_to_json(&Type::INT4, Some("-42")), json!(-42));
        assert_eq!(text_value_to_json(&Type::FLOAT8, Some("1.5")), json!(1.5));
        assert_eq!(text_value_to_json(&Type::FLOAT8, Some("NaN")), json!("NaN"));
        assert_eq!(
            text_value_to_json(&Type::FLOAT4, Some("-Infinity")),
            json!("-Infinity")
        );
        assert_eq!(
            text_value_to_json(&Type::NUMERIC, Some("12345678901234567890.0123")),
            json!("12345678901234567890.0123")
        );
        assert_eq!(
            text_value_to_json(&Type::TIMESTAMPTZ, Some("2026-03-01 09:30:00-03")),
            json!("2026-03-01 09:30:00-03")
        );
        assert_eq!(text_value_to_json(&Type::NUMERIC, None), Value::Null);
    }

    #[test]
    fn text_rows_map_by_column_name() {
        let columns = vec![
            ("id".to_string(), Type::INT8),
            ("total".to_string(), Type::NUMERIC),
        ];
        let row = text_row_to_json_map(&columns, &[Some("7".into()), None]);
        assert_eq!(row.get("id"), Some(&json!(7)));
        assert_eq!(row.get("total"), Some(&Value::Null));
    }
}
