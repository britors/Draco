//! Parsing of CSV and JSON files for the table import, and their encoding as `COPY … FROM STDIN`
//! text data. Nothing here touches the database: the app layer validates the column mapping
//! against the live table and `PostgresDriver::copy_in` sends the data in one transaction.
//!
//! Values stay text; PostgreSQL converts each one to its column type while copying, exactly as it
//! would for `COPY` from `psql`, so type errors come back as the server's own messages.

use std::path::Path;

use crate::error::{CoreError, Result};

/// Files above this size are rejected before parsing; the whole file is held in memory.
pub const MAX_IMPORT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Csv,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvOptions {
    pub delimiter: char,
    pub has_header: bool,
    /// An unquoted empty field becomes NULL (as in `COPY … CSV`); a quoted `""` stays empty.
    pub empty_as_null: bool,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: ',',
            has_header: true,
            empty_as_null: true,
        }
    }
}

/// Parsed file: column names (from the header, the JSON keys, or `column_1…`) and rows of text
/// values with `None` for NULL. Every row has exactly `columns.len()` values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedImport {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

/// Why a file could not be parsed. `line` is 1-based in the file (CSV) or the 1-based array
/// position (JSON).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportParseError {
    TooLarge,
    NotUtf8,
    Empty,
    UnterminatedQuote {
        line: usize,
    },
    FieldCount {
        line: usize,
        expected: usize,
        found: usize,
    },
    InvalidJson,
    JsonNotArrayOfObjects {
        position: usize,
    },
    DuplicateColumn {
        name: String,
    },
}

impl std::fmt::Display for ImportParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(formatter, "the file is larger than the import limit"),
            Self::NotUtf8 => write!(formatter, "the file is not UTF-8 text"),
            Self::Empty => write!(formatter, "the file has no rows"),
            Self::UnterminatedQuote { line } => {
                write!(formatter, "unterminated quote starting on line {line}")
            }
            Self::FieldCount {
                line,
                expected,
                found,
            } => write!(
                formatter,
                "line {line} has {found} fields, expected {expected}"
            ),
            Self::InvalidJson => write!(formatter, "the file is not valid JSON"),
            Self::JsonNotArrayOfObjects { position } => {
                write!(formatter, "item {position} is not a JSON object")
            }
            Self::DuplicateColumn { name } => write!(formatter, "column {name} appears twice"),
        }
    }
}

/// Reads and parses a file the user chose. The read and the parse run off the async runtime.
pub async fn load_import_file(
    path: &Path,
    format: ImportFormat,
    options: CsvOptions,
) -> Result<std::result::Result<ParsedImport, ImportParseError>> {
    let metadata = tokio::fs::metadata(path).await?;
    if metadata.len() > MAX_IMPORT_BYTES {
        return Ok(Err(ImportParseError::TooLarge));
    }
    let bytes = tokio::fs::read(path).await?;
    tokio::task::spawn_blocking(move || {
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => return Err(ImportParseError::NotUtf8),
        };
        match format {
            ImportFormat::Csv => parse_csv(&text, options),
            ImportFormat::Json => parse_json(&text),
        }
    })
    .await
    .map_err(|error| CoreError::Other(error.to_string()))
}

/// RFC 4180 CSV with a configurable delimiter: `"` quotes, `""` escapes a quote, quoted fields
/// may span lines, CRLF and LF both end a record, a UTF-8 BOM and blank lines are ignored.
pub fn parse_csv(
    text: &str,
    options: CsvOptions,
) -> std::result::Result<ParsedImport, ImportParseError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records: Vec<(usize, Vec<Option<String>>)> = Vec::new();
    let mut record: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut line = 1;
    let mut record_line = 1;
    let mut quote_line = 1;
    let mut chars = text.chars().peekable();

    let finish_field = |field: &mut String, quoted: &mut bool, record: &mut Vec<Option<String>>| {
        let value = std::mem::take(field);
        let null = options.empty_as_null && value.is_empty() && !*quoted;
        record.push((!null).then_some(value));
        *quoted = false;
    };

    while let Some(character) = chars.next() {
        if in_quotes {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                other => field.push(other),
            }
            continue;
        }
        match character {
            '"' if field.is_empty() && !quoted => {
                in_quotes = true;
                quoted = true;
                quote_line = line;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                finish_field(&mut field, &mut quoted, &mut record);
                let finished = std::mem::take(&mut record);
                if !(finished.len() == 1 && finished[0].as_deref().is_none_or(str::is_empty)) {
                    records.push((record_line, finished));
                }
                line += 1;
                record_line = line;
            }
            character if character == options.delimiter => {
                finish_field(&mut field, &mut quoted, &mut record);
            }
            other => field.push(other),
        }
    }
    if in_quotes {
        return Err(ImportParseError::UnterminatedQuote { line: quote_line });
    }
    if !field.is_empty() || quoted || !record.is_empty() {
        finish_field(&mut field, &mut quoted, &mut record);
        records.push((record_line, record));
    }

    let mut records = records.into_iter();
    let (columns, expected) = if options.has_header {
        let (_, header) = records.next().ok_or(ImportParseError::Empty)?;
        let columns = header
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                name.map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("column_{}", index + 1))
            })
            .collect::<Vec<_>>();
        let expected = columns.len();
        (Some(columns), expected)
    } else {
        (None, 0)
    };
    let mut rows = Vec::new();
    let mut expected = expected;
    for (line, row) in records {
        if expected == 0 {
            expected = row.len();
        }
        if row.len() != expected {
            return Err(ImportParseError::FieldCount {
                line,
                expected,
                found: row.len(),
            });
        }
        rows.push(row);
    }
    let columns = columns.unwrap_or_else(|| {
        (1..=expected)
            .map(|index| format!("column_{index}"))
            .collect()
    });
    if columns.is_empty() {
        return Err(ImportParseError::Empty);
    }
    reject_duplicate_columns(&columns)?;
    Ok(ParsedImport { columns, rows })
}

/// A JSON array of objects. Columns are the keys in first-seen order; a missing key is NULL.
/// Strings are taken as they are, numbers and booleans in their JSON spelling, and nested
/// objects or arrays as JSON text (suitable for `json`/`jsonb` columns).
pub fn parse_json(text: &str) -> std::result::Result<ParsedImport, ImportParseError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let items: Vec<JsonItem> = serde_json::from_str(text).map_err(|_| {
        // Valid JSON that is not an array (an object, a number…) gets the clearer message.
        if serde_json::from_str::<serde_json::Value>(text).is_ok() {
            ImportParseError::JsonNotArrayOfObjects { position: 0 }
        } else {
            ImportParseError::InvalidJson
        }
    })?;
    let mut columns: Vec<String> = Vec::new();
    let mut index = std::collections::HashMap::new();
    let mut objects = Vec::with_capacity(items.len());
    for (position, item) in items.into_iter().enumerate() {
        let JsonItem::Object(fields) = item else {
            return Err(ImportParseError::JsonNotArrayOfObjects {
                position: position + 1,
            });
        };
        for (key, _) in &fields {
            if !index.contains_key(key) {
                index.insert(key.clone(), columns.len());
                columns.push(key.clone());
            }
        }
        objects.push(fields);
    }
    if columns.is_empty() {
        return Err(ImportParseError::Empty);
    }
    let rows = objects
        .into_iter()
        .map(|fields| {
            let mut row = vec![None; columns.len()];
            for (key, value) in fields {
                row[index[&key]] = json_to_text(&value);
            }
            row
        })
        .collect();
    Ok(ParsedImport { columns, rows })
}

/// One array item. Objects keep their keys in file order (`serde_json::Map` sorts them unless
/// the `preserve_order` feature is on, which would change every map in the workspace).
enum JsonItem {
    Object(Vec<(String, serde_json::Value)>),
    Other,
}

impl<'de> serde::Deserialize<'de> for JsonItem {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = JsonItem;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a JSON value")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<JsonItem, A::Error> {
                let mut fields: Vec<(String, serde_json::Value)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, serde_json::Value>()? {
                    match fields.iter_mut().find(|(existing, _)| *existing == key) {
                        Some(field) => field.1 = value,
                        None => fields.push((key, value)),
                    }
                }
                Ok(JsonItem::Object(fields))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<JsonItem, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(JsonItem::Other)
            }
            fn visit_bool<E>(self, _: bool) -> std::result::Result<JsonItem, E> {
                Ok(JsonItem::Other)
            }
            fn visit_i64<E>(self, _: i64) -> std::result::Result<JsonItem, E> {
                Ok(JsonItem::Other)
            }
            fn visit_u64<E>(self, _: u64) -> std::result::Result<JsonItem, E> {
                Ok(JsonItem::Other)
            }
            fn visit_f64<E>(self, _: f64) -> std::result::Result<JsonItem, E> {
                Ok(JsonItem::Other)
            }
            fn visit_str<E>(self, _: &str) -> std::result::Result<JsonItem, E> {
                Ok(JsonItem::Other)
            }
            fn visit_unit<E>(self) -> std::result::Result<JsonItem, E> {
                Ok(JsonItem::Other)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

fn json_to_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        nested => Some(nested.to_string()),
    }
}

fn reject_duplicate_columns(columns: &[String]) -> std::result::Result<(), ImportParseError> {
    let mut seen = std::collections::HashSet::new();
    for column in columns {
        if !seen.insert(column) {
            return Err(ImportParseError::DuplicateColumn {
                name: column.clone(),
            });
        }
    }
    Ok(())
}

/// Appends one row in `COPY` text format: tab-separated, `\N` for NULL, and backslash, tab,
/// newline and carriage return escaped.
pub fn push_copy_text_row(buffer: &mut Vec<u8>, values: &[Option<&str>]) {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            buffer.push(b'\t');
        }
        match value {
            None => buffer.extend_from_slice(b"\\N"),
            Some(text) => {
                for byte in text.bytes() {
                    match byte {
                        b'\\' => buffer.extend_from_slice(b"\\\\"),
                        b'\t' => buffer.extend_from_slice(b"\\t"),
                        b'\n' => buffer.extend_from_slice(b"\\n"),
                        b'\r' => buffer.extend_from_slice(b"\\r"),
                        other => buffer.push(other),
                    }
                }
            }
        }
    }
    buffer.push(b'\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv(text: &str) -> ParsedImport {
        parse_csv(text, CsvOptions::default()).unwrap()
    }

    fn some(values: &[&str]) -> Vec<Option<String>> {
        values.iter().map(|value| Some(value.to_string())).collect()
    }

    #[test]
    fn parses_header_quotes_and_line_endings() {
        let parsed = csv("\u{feff}id,name,note\r\n1,\"Smith, Ann\",\"said \"\"hi\"\"\"\r\n2,Bob,\"two\nlines\"\n\n");
        assert_eq!(parsed.columns, vec!["id", "name", "note"]);
        assert_eq!(
            parsed.rows,
            vec![
                some(&["1", "Smith, Ann", "said \"hi\""]),
                some(&["2", "Bob", "two\nlines"])
            ]
        );
    }

    #[test]
    fn empty_fields_are_null_unless_quoted() {
        let parsed = csv("a,b,c\n,\"\",x\n");
        assert_eq!(
            parsed.rows,
            vec![vec![None, Some(String::new()), Some("x".into())]]
        );
        let keep = parse_csv(
            "a,b\n,x\n",
            CsvOptions {
                empty_as_null: false,
                ..CsvOptions::default()
            },
        )
        .unwrap();
        assert_eq!(keep.rows, vec![some(&["", "x"])]);
    }

    #[test]
    fn supports_other_delimiters_and_no_header() {
        let parsed = parse_csv(
            "1;a\n2;b",
            CsvOptions {
                delimiter: ';',
                has_header: false,
                ..CsvOptions::default()
            },
        )
        .unwrap();
        assert_eq!(parsed.columns, vec!["column_1", "column_2"]);
        assert_eq!(parsed.rows, vec![some(&["1", "a"]), some(&["2", "b"])]);
        let tabs = parse_csv(
            "x\ty\n1\t2\n",
            CsvOptions {
                delimiter: '\t',
                ..CsvOptions::default()
            },
        )
        .unwrap();
        assert_eq!(tabs.rows, vec![some(&["1", "2"])]);
    }

    #[test]
    fn reports_csv_errors_with_lines() {
        assert_eq!(
            parse_csv("a,b\n1,2\n3\n", CsvOptions::default()).unwrap_err(),
            ImportParseError::FieldCount {
                line: 3,
                expected: 2,
                found: 1
            }
        );
        assert_eq!(
            parse_csv("a\n\"open\n", CsvOptions::default()).unwrap_err(),
            ImportParseError::UnterminatedQuote { line: 2 }
        );
        assert_eq!(
            parse_csv("", CsvOptions::default()).unwrap_err(),
            ImportParseError::Empty
        );
        assert_eq!(
            parse_csv("a,a\n1,2", CsvOptions::default()).unwrap_err(),
            ImportParseError::DuplicateColumn { name: "a".into() }
        );
        let blank_header = csv(" ,b\n1,2");
        assert_eq!(blank_header.columns, vec!["column_1", "b"]);
    }

    #[test]
    fn header_only_files_have_columns_and_no_rows() {
        let parsed = csv("a,b\n");
        assert_eq!(parsed.columns, vec!["a", "b"]);
        assert!(parsed.rows.is_empty());
    }

    #[test]
    fn parses_json_arrays_of_objects() {
        let parsed = parse_json(
            r#"[{"id": 1, "name": "Ann", "tags": ["a"], "active": true},
                {"name": "Bob", "id": 2.5, "extra": null, "meta": {"k": "v"}}]"#,
        )
        .unwrap();
        assert_eq!(
            parsed.columns,
            vec!["id", "name", "tags", "active", "extra", "meta"]
        );
        assert_eq!(
            parsed.rows[0],
            vec![
                Some("1".into()),
                Some("Ann".into()),
                Some("[\"a\"]".into()),
                Some("true".into()),
                None,
                None
            ]
        );
        assert_eq!(parsed.rows[1][0].as_deref(), Some("2.5"));
        assert_eq!(parsed.rows[1][5].as_deref(), Some("{\"k\":\"v\"}"));
        assert_eq!(parse_json("{").unwrap_err(), ImportParseError::InvalidJson);
        assert_eq!(
            parse_json("{\"a\": 1}").unwrap_err(),
            ImportParseError::JsonNotArrayOfObjects { position: 0 }
        );
        assert_eq!(
            parse_json("[{\"a\": 1}, 2]").unwrap_err(),
            ImportParseError::JsonNotArrayOfObjects { position: 2 }
        );
        assert_eq!(parse_json("[]").unwrap_err(), ImportParseError::Empty);
    }

    #[test]
    fn large_json_numbers_keep_their_digits() {
        let parsed = parse_json(r#"[{"n": 12345678901234567890.123456789}]"#).unwrap();
        assert_eq!(
            parsed.rows[0][0].as_deref(),
            Some("12345678901234567890.123456789")
        );
    }

    #[test]
    fn copy_text_escapes_special_characters() {
        let mut buffer = Vec::new();
        push_copy_text_row(
            &mut buffer,
            &[Some("a\tb"), None, Some("c\\d\ne\rf"), Some("")],
        );
        assert_eq!(buffer, b"a\\tb\t\\N\tc\\\\d\\ne\\rf\t\n");
    }
}
