//! CSV/JSON import into an existing table.
//!
//! The file comes only from the native picker (see `FileAuthorizationPurpose::Import`); the
//! preview may reread it while the authorization lasts, and the import consumes it. The import
//! rereads the live table structure, validates the mapping and sends the rows with one
//! `COPY … FROM STDIN` inside a transaction, so a failure or a cancellation leaves the table as
//! it was.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use draco_core::postgres::queries;
use draco_core::table_import::{
    self as importer, CsvOptions, ImportFormat, ImportParseError, ParsedImport,
};
use serde::{Deserialize, Serialize};

use crate::{
    validate_file_path, validate_table_name, Application, ApplicationError,
    FileAuthorizationPurpose, Result, Validation,
};

/// Rows shown in the preview.
const SAMPLE_ROWS: usize = 20;
/// Size of each `COPY` data message.
const COPY_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TableImportFormat {
    Csv,
    Json,
}

impl From<TableImportFormat> for ImportFormat {
    fn from(format: TableImportFormat) -> Self {
        match format {
            TableImportFormat::Csv => ImportFormat::Csv,
            TableImportFormat::Json => ImportFormat::Json,
        }
    }
}

/// A file chosen in the native picker. `format` is guessed from the extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableImportFileView {
    pub path: String,
    pub file_name: String,
    pub format: TableImportFormat,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableImportSourceInput {
    pub path: String,
    pub format: TableImportFormat,
    /// One character; defaults to a comma. Ignored for JSON.
    #[serde(default)]
    pub delimiter: Option<String>,
    #[serde(default = "default_true")]
    pub has_header: bool,
    #[serde(default = "default_true")]
    pub empty_as_null: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableImportColumnView {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub has_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableImportPreviewView {
    pub source_columns: Vec<String>,
    pub sample_rows: Vec<Vec<Option<String>>>,
    pub total_rows: usize,
    pub table_columns: Vec<TableImportColumnView>,
    /// For each source column, the table column with the same name, if any.
    pub suggested_mapping: Vec<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableImportMappingInput {
    pub source_column: usize,
    pub table_column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableImportInput {
    pub source: TableImportSourceInput,
    pub mapping: Vec<TableImportMappingInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableImportResultView {
    pub operation_id: String,
    pub rows_imported: u64,
    pub cancelled: bool,
}

impl Application {
    /// Registers a file picked in the native dialog for preview and one import.
    pub async fn authorize_import_file(&self, path: &str) -> Result<TableImportFileView> {
        self.authorize_file_path(path, FileAuthorizationPurpose::Import)
            .await?;
        let path_ref = Path::new(path);
        let extension = path_ref
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        Ok(TableImportFileView {
            path: path.to_string(),
            file_name: path_ref
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            format: if extension.as_deref() == Some("json") {
                TableImportFormat::Json
            } else {
                TableImportFormat::Csv
            },
        })
    }

    /// Parses the chosen file and lists the importable columns of the target table.
    pub async fn preview_table_import(
        &self,
        id: &str,
        schema: &str,
        table: &str,
        source: TableImportSourceInput,
    ) -> Result<TableImportPreviewView> {
        validate_table_name(schema, table)?;
        validate_file_path(&source.path, "Import file")?;
        self.check_file_authorization(&source.path, FileAuthorizationPurpose::Import)
            .await?;
        let (driver, _) = self.connected_driver(id).await?;
        let table_columns = import_target_columns(&driver, schema, table).await?;
        let parsed = load_source(&source).await?;
        let by_name = table_columns
            .iter()
            .map(|column| (normalize_column_name(&column.name), column.name.clone()))
            .collect::<HashMap<_, _>>();
        let suggested_mapping = parsed
            .columns
            .iter()
            .map(|column| by_name.get(&normalize_column_name(column)).cloned())
            .collect();
        Ok(TableImportPreviewView {
            source_columns: parsed.columns,
            total_rows: parsed.rows.len(),
            sample_rows: parsed.rows.into_iter().take(SAMPLE_ROWS).collect(),
            table_columns,
            suggested_mapping,
        })
    }

    /// Imports every row of the file in one transaction. Cancelling through `operation_id`
    /// rolls the import back and reports `cancelled`.
    pub async fn run_table_import(
        &self,
        id: &str,
        schema: &str,
        table: &str,
        operation_id: &str,
        input: TableImportInput,
    ) -> Result<TableImportResultView> {
        validate_table_name(schema, table)?;
        validate_file_path(&input.source.path, "Import file")?;
        let (driver, _) = self.writable_driver(id).await?;
        let table_columns = import_target_columns(&driver, schema, table).await?;
        self.consume_file_authorization(&input.source.path, FileAuthorizationPurpose::Import)
            .await?;
        let parsed = load_source(&input.source).await?;
        let (source_indexes, target_columns) =
            validate_mapping(&input.mapping, &parsed.columns, &table_columns)?;
        if parsed.rows.is_empty() {
            return Err(ApplicationError::InvalidInput(Validation::new(
                "validation.importFileHasNoRows",
                "The file has no data rows to import",
            )));
        }
        let chunks = encode_rows(&parsed.rows, &source_indexes);
        let copy_sql = queries::import_copy_statement(schema, table, &target_columns);

        let cancel_rx = self.register_operation(operation_id).await?;
        let cancel_flag = cancel_rx.clone();
        let outcome = driver.copy_in(&copy_sql, chunks, cancel_rx).await;
        self.finish_operation(operation_id).await;
        match outcome {
            Ok(rows_imported) => Ok(TableImportResultView {
                operation_id: operation_id.to_string(),
                rows_imported,
                cancelled: false,
            }),
            Err(_) if *cancel_flag.borrow() => Ok(TableImportResultView {
                operation_id: operation_id.to_string(),
                rows_imported: 0,
                cancelled: true,
            }),
            Err(error) => Err(error.into()),
        }
    }
}

async fn import_target_columns(
    driver: &draco_core::postgres::PostgresDriver,
    schema: &str,
    table: &str,
) -> Result<Vec<TableImportColumnView>> {
    let columns = queries::get_import_target_columns(driver, schema, table).await?;
    if columns.is_empty() {
        return Err(ApplicationError::InvalidInput(Validation::new(
            "validation.importTableNotFound",
            "The import target must be an existing table with writable columns",
        )));
    }
    Ok(columns
        .into_iter()
        .map(|column| TableImportColumnView {
            name: column.name,
            data_type: column.data_type,
            nullable: column.nullable,
            has_default: column.has_default,
        })
        .collect())
}

async fn load_source(source: &TableImportSourceInput) -> Result<ParsedImport> {
    let delimiter = match source.delimiter.as_deref() {
        None | Some("") => ',',
        Some("\\t") => '\t',
        Some(value) => {
            let mut chars = value.chars();
            match (chars.next(), chars.next()) {
                (Some(character), None) if !matches!(character, '"' | '\n' | '\r') => character,
                _ => return Err(ApplicationError::InvalidInput(Validation::new(
                    "validation.importDelimiterInvalid",
                    "The delimiter must be a single character other than a quote or a line break",
                ))),
            }
        }
    };
    let options = CsvOptions {
        delimiter,
        has_header: source.has_header,
        empty_as_null: source.empty_as_null,
    };
    let parsed = importer::load_import_file(Path::new(&source.path), source.format.into(), options)
        .await
        .map_err(|_| {
            ApplicationError::InvalidInput(Validation::new(
                "validation.importFileUnreadable",
                "The selected file could not be read",
            ))
        })?;
    parsed.map_err(|error| ApplicationError::InvalidInput(parse_validation(error)))
}

fn parse_validation(error: ImportParseError) -> Validation {
    let message = error.to_string();
    match error {
        ImportParseError::TooLarge => Validation::new("validation.importFileTooLarge", message)
            .param(
                "limit",
                format!("{} MiB", importer::MAX_IMPORT_BYTES / (1024 * 1024)),
            ),
        ImportParseError::NotUtf8 => Validation::new("validation.importFileNotUtf8", message),
        ImportParseError::Empty => Validation::new("validation.importFileEmpty", message),
        ImportParseError::UnterminatedQuote { line } => {
            Validation::new("validation.importUnterminatedQuote", message)
                .param("line", line.to_string())
        }
        ImportParseError::FieldCount {
            line,
            expected,
            found,
        } => Validation::new("validation.importFieldCount", message)
            .param("line", line.to_string())
            .param("expected", expected.to_string())
            .param("found", found.to_string()),
        ImportParseError::InvalidJson => Validation::new("validation.importInvalidJson", message),
        ImportParseError::JsonNotArrayOfObjects { position } => {
            Validation::new("validation.importJsonNotArray", message)
                .param("position", position.to_string())
        }
        ImportParseError::DuplicateColumn { name } => {
            Validation::new("validation.importDuplicateColumn", message).param("name", name)
        }
    }
}

/// Checks the mapping against the file and the live table and returns, in order, the source
/// indexes to read and the table columns they fill.
fn validate_mapping(
    mapping: &[TableImportMappingInput],
    source_columns: &[String],
    table_columns: &[TableImportColumnView],
) -> Result<(Vec<usize>, Vec<String>)> {
    if mapping.is_empty() {
        return Err(ApplicationError::InvalidInput(Validation::new(
            "validation.importMappingEmpty",
            "Map at least one file column to a table column",
        )));
    }
    let known = table_columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<HashSet<_>>();
    let mut used = HashSet::new();
    let mut indexes = Vec::with_capacity(mapping.len());
    let mut targets = Vec::with_capacity(mapping.len());
    for entry in mapping {
        if entry.source_column >= source_columns.len()
            || !known.contains(entry.table_column.as_str())
        {
            return Err(ApplicationError::InvalidInput(Validation::new(
                "validation.importMappingMismatch",
                "The column mapping does not match the file or the table; preview again",
            )));
        }
        if !used.insert(entry.table_column.as_str()) {
            return Err(ApplicationError::InvalidInput(
                Validation::new(
                    "validation.importMappingDuplicate",
                    format!("Table column {} is mapped twice", entry.table_column),
                )
                .param("name", entry.table_column.clone()),
            ));
        }
        indexes.push(entry.source_column);
        targets.push(entry.table_column.clone());
    }
    let missing = table_columns
        .iter()
        .filter(|column| !column.nullable && !column.has_default)
        .filter(|column| !used.contains(column.name.as_str()))
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(ApplicationError::InvalidInput(
            Validation::new(
                "validation.importRequiredColumnsUnmapped",
                format!(
                    "Required columns without a default are not mapped: {}",
                    missing.join(", ")
                ),
            )
            .param("names", missing.join(", ")),
        ));
    }
    Ok((indexes, targets))
}

fn encode_rows(rows: &[Vec<Option<String>>], indexes: &[usize]) -> Vec<bytes::Bytes> {
    let mut chunks = Vec::new();
    let mut buffer = Vec::with_capacity(COPY_CHUNK_BYTES + 4096);
    let mut values = Vec::with_capacity(indexes.len());
    for row in rows {
        values.clear();
        values.extend(indexes.iter().map(|&index| row[index].as_deref()));
        importer::push_copy_text_row(&mut buffer, &values);
        if buffer.len() >= COPY_CHUNK_BYTES {
            chunks.push(bytes::Bytes::from(std::mem::replace(
                &mut buffer,
                Vec::with_capacity(COPY_CHUNK_BYTES + 4096),
            )));
        }
    }
    if !buffer.is_empty() {
        chunks.push(bytes::Bytes::from(buffer));
    }
    chunks
}

/// `Customer ID` and `customer_id` match the same column.
fn normalize_column_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|character| {
            if character.is_whitespace() || character == '-' {
                '_'
            } else {
                character.to_ascii_lowercase()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, nullable: bool, has_default: bool) -> TableImportColumnView {
        TableImportColumnView {
            name: name.into(),
            data_type: "text".into(),
            nullable,
            has_default,
        }
    }

    fn map(source_column: usize, table_column: &str) -> TableImportMappingInput {
        TableImportMappingInput {
            source_column,
            table_column: table_column.into(),
        }
    }

    #[test]
    fn mapping_must_match_the_file_and_the_table() {
        let source = vec!["Name".to_string(), "Age".to_string()];
        let table = vec![
            column("id", false, true),
            column("name", false, false),
            column("age", true, false),
        ];
        let (indexes, targets) =
            validate_mapping(&[map(1, "age"), map(0, "name")], &source, &table).unwrap();
        assert_eq!(indexes, vec![1, 0]);
        assert_eq!(targets, vec!["age", "name"]);

        let key = |result: Result<(Vec<usize>, Vec<String>)>| match result.unwrap_err() {
            ApplicationError::InvalidInput(validation) => validation.key,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(
            key(validate_mapping(&[], &source, &table)),
            "validation.importMappingEmpty"
        );
        assert_eq!(
            key(validate_mapping(&[map(2, "name")], &source, &table)),
            "validation.importMappingMismatch"
        );
        assert_eq!(
            key(validate_mapping(&[map(0, "missing")], &source, &table)),
            "validation.importMappingMismatch"
        );
        assert_eq!(
            key(validate_mapping(
                &[map(0, "name"), map(1, "name")],
                &source,
                &table
            )),
            "validation.importMappingDuplicate"
        );
        assert_eq!(
            key(validate_mapping(&[map(1, "age")], &source, &table)),
            "validation.importRequiredColumnsUnmapped"
        );
    }

    #[test]
    fn rows_are_encoded_in_mapping_order_and_chunked() {
        let rows = vec![
            vec![Some("a".to_string()), None],
            vec![Some("b\tc".to_string()), Some("2".to_string())],
        ];
        let chunks = encode_rows(&rows, &[1, 0]);
        assert_eq!(chunks.len(), 1);
        assert_eq!(&chunks[0][..], b"\\N\ta\n2\tb\\tc\n");
        let many = vec![vec![Some("x".repeat(1000))]; 3000];
        let chunks = encode_rows(&many, &[0]);
        assert!(chunks.len() >= 2);
        assert_eq!(
            chunks.iter().map(|chunk| chunk.len()).sum::<usize>(),
            3000 * 1001
        );
    }

    #[test]
    fn column_names_match_loosely() {
        assert_eq!(normalize_column_name(" Customer ID "), "customer_id");
        assert_eq!(normalize_column_name("order-date"), "order_date");
    }

    #[tokio::test]
    async fn preview_requires_a_file_from_the_picker() {
        let app = Application::new();
        let error = app
            .preview_table_import(
                "missing",
                "public",
                "t",
                TableImportSourceInput {
                    path: "/tmp/never-authorized.csv".into(),
                    format: TableImportFormat::Csv,
                    delimiter: None,
                    has_header: true,
                    empty_as_null: true,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ApplicationError::InvalidInput(Validation {
                key: "validation.chooseTheFileAgainWithThe",
                ..
            })
        ));
    }
}
