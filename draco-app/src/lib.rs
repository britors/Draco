//! Application services for Draco.
//!
//! This crate is the boundary between the UI and `draco-core`: callers work only with
//! serializable DTOs and use-case methods. Credentials are accepted only for the duration of a
//! connection attempt and are never part of a DTO or persisted by this layer.

use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Instant;

use draco_core::assistant;
use draco_core::connection::{validate_connection, DbConnection, DbConnectionDraft};
use draco_core::error::CoreError;
use draco_core::manager::{ConnectionManager, ConnectionStatus};
use draco_core::postgres::backup::{
    DumpFormat, DumpOptions, RestoreOptions, ToolConnection, ToolEvent,
};
use draco_core::postgres::{backup, queries, test_connection_with_ssh, PostgresDriver};
use draco_core::secrets;
use draco_core::store;
pub use draco_core::store::{AccentColor, AppTheme};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch, Mutex};

pub type SharedApplication = Arc<Application>;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error("invalid connection: {0}")]
    InvalidConnection(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("connection not found: {0}")]
    ConnectionNotFound(String),
    #[error("connection is not active: {0}")]
    ConnectionNotActive(String),
    #[error("assistant error: {0}")]
    Assistant(String),
    #[error("operation error: {0}")]
    Operation(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}

pub type Result<T> = std::result::Result<T, ApplicationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

impl From<ConnectionStatus> for ConnectionState {
    fn from(status: ConnectionStatus) -> Self {
        match status {
            ConnectionStatus::Disconnected => Self::Disconnected,
            ConnectionStatus::Connecting => Self::Connecting,
            ConnectionStatus::Connected => Self::Connected,
            ConnectionStatus::Error => Self::Error,
        }
    }
}

/// Connection metadata safe to send to a UI. Passwords and passphrases are intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionView {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub ssl: bool,
    pub ssh_enabled: bool,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_user: Option<String>,
    pub ssh_key_path: Option<String>,
    pub ssh_jump_host: Option<String>,
    pub ssh_jump_port: Option<u16>,
    pub ssh_jump_user: Option<String>,
    pub ssh_jump_key_path: Option<String>,
    pub favorite: bool,
    pub state: ConnectionState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInput {
    pub id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: u32,
    pub database: String,
    pub user: String,
    pub ssl: bool,
    pub ssh_enabled: bool,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_user: Option<String>,
    pub ssh_key_path: Option<String>,
    pub ssh_jump_host: Option<String>,
    pub ssh_jump_port: Option<u16>,
    pub ssh_jump_user: Option<String>,
    pub ssh_jump_key_path: Option<String>,
    pub favorite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaView {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ObjectKind {
    Table,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableView {
    pub name: String,
    pub kind: ObjectKind,
    pub estimated_rows: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaObjectKind {
    Function,
    Procedure,
    Sequence,
    Trigger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaObjectView {
    pub name: String,
    pub kind: SchemaObjectKind,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionTableView {
    pub schema: String,
    pub name: String,
    pub kind: ObjectKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionColumnView {
    pub schema: String,
    pub table: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionFunctionView {
    pub schema: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompletionDataView {
    pub schemas: Vec<String>,
    pub tables: Vec<CompletionTableView>,
    pub columns: Vec<CompletionColumnView>,
    pub functions: Vec<CompletionFunctionView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchObjectKind {
    Table,
    View,
    Column,
    Function,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResultView {
    pub kind: SearchObjectKind,
    pub schema: String,
    pub table: Option<String>,
    pub name: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardView {
    pub dashboard: serde_json::Value,
    pub stats: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDetailView {
    pub schema: String,
    pub table: String,
    pub detail: serde_json::Value,
    pub ddl: String,
    pub column_stats: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErdView {
    pub schema: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminView {
    pub activity: serde_json::Value,
    pub locks: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleView {
    pub name: String,
    pub superuser: bool,
    pub create_database: bool,
    pub create_role: bool,
    pub login: bool,
    pub connection_limit: i32,
    pub valid_until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRoleInput {
    pub name: String,
    pub login: bool,
    pub create_database: bool,
    pub create_role: bool,
    pub superuser: bool,
    pub connection_limit: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronJobView {
    pub id: i64,
    pub name: Option<String>,
    pub schedule: String,
    pub command: String,
    pub database: Option<String>,
    pub username: Option<String>,
    pub active: bool,
    pub last_status: Option<String>,
    pub last_run: Option<String>,
    pub last_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronJobsView {
    pub installed: bool,
    pub jobs: Vec<CronJobView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionView {
    pub name: String,
    pub default_version: Option<String>,
    pub installed_version: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionsView {
    pub installed: Vec<ExtensionView>,
    pub available: Vec<ExtensionView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryStatView {
    pub query: String,
    pub calls: i64,
    pub total_exec_ms: f64,
    pub mean_exec_ms: f64,
    pub rows: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryStatsView {
    pub installed: bool,
    pub queries: Vec<QueryStatView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableMaintenanceOperation {
    Vacuum,
    Analyze,
    VacuumAnalyze,
    VacuumFull,
}

impl TableMaintenanceOperation {
    fn sql(self) -> &'static str {
        match self {
            Self::Vacuum => "VACUUM",
            Self::Analyze => "ANALYZE",
            Self::VacuumAnalyze => "VACUUM ANALYZE",
            Self::VacuumFull => "VACUUM FULL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupFormat {
    Plain,
    Custom,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupOptionsInput {
    pub output: String,
    pub format: BackupFormat,
    pub compression: Option<u8>,
    #[serde(default)]
    pub schemas: Vec<String>,
    #[serde(default)]
    pub tables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOptionsInput {
    pub input: String,
    pub target_database: String,
    pub clean: bool,
    pub single_transaction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultView {
    pub operation_id: String,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub succeeded: bool,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantReplyView {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: Option<f64>,
    pub history: Vec<assistant::AiMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryView {
    pub id: String,
    pub sql: String,
    pub conn_id: String,
    pub conn_label: String,
    pub timestamp: i64,
    pub duration_ms: i64,
    pub row_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetInput {
    pub name: String,
    pub sql: String,
    pub conn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetView {
    pub id: String,
    pub name: String,
    pub sql: String,
    pub created_at: i64,
    pub conn_id: Option<String>,
    pub conn_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub service: String,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreferencesView {
    pub version: String,
    pub theme: AppTheme,
    pub accent: AccentColor,
    pub check_updates_on_startup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStatusView {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
}

/// Shared application state. The UI owns an `Arc<Application>` and never needs to know how the
/// core manager or local storage is implemented.
pub struct Application {
    manager: Mutex<ConnectionManager>,
    operations: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl Application {
    pub fn new() -> Self {
        let mut manager = ConnectionManager::new();
        manager.sync_connections(store::list_connections());
        Self {
            manager: Mutex::new(manager),
            operations: Mutex::new(HashMap::new()),
        }
    }

    pub fn shared() -> SharedApplication {
        Arc::new(Self::new())
    }

    pub fn health(&self) -> Health {
        Health {
            service: "draco-app".to_string(),
            ready: true,
        }
    }

    pub fn preferences(&self) -> PreferencesView {
        let settings = store::get_settings();
        PreferencesView {
            version: env!("CARGO_PKG_VERSION").to_string(),
            theme: settings.theme,
            accent: settings.accent,
            check_updates_on_startup: settings.check_updates_on_startup,
        }
    }

    pub fn save_preferences(&self, preferences: PreferencesView) -> Result<PreferencesView> {
        let saved = store::patch_settings(|settings| {
            settings.theme = preferences.theme;
            settings.accent = preferences.accent;
            settings.check_updates_on_startup = preferences.check_updates_on_startup;
        })?;
        Ok(PreferencesView {
            version: env!("CARGO_PKG_VERSION").to_string(),
            theme: saved.theme,
            accent: saved.accent,
            check_updates_on_startup: saved.check_updates_on_startup,
        })
    }

    pub async fn check_for_updates(&self) -> Result<UpdateStatusView> {
        let release = draco_core::updates::latest_release().await?;
        let latest_version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
        Ok(UpdateStatusView {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            update_available: is_newer_version(&latest_version, env!("CARGO_PKG_VERSION")),
            latest_version,
            release_url: release.html_url,
        })
    }

    pub async fn list_connections(&self) -> Vec<ConnectionView> {
        let manager = self.manager.lock().await;
        store::list_connections()
            .into_iter()
            .map(|conn| self.view_for(&manager, conn))
            .collect()
    }

    pub async fn save_connection(&self, input: ConnectionInput) -> Result<ConnectionView> {
        validate_input(&input)?;

        let conn = input.into_core();
        let saved = store::save_connection(conn)?;
        let mut manager = self.manager.lock().await;
        manager.sync_connections(store::list_connections());
        Ok(self.view_for(&manager, saved))
    }

    /// Saves metadata and, when provided, updates credentials in the system Secret Service.
    /// `None` deliberately means "keep the existing secret" when editing a connection.
    pub async fn save_connection_with_secrets(
        &self,
        input: ConnectionInput,
        password: Option<&str>,
        ssh_password: Option<&str>,
        jump_password: Option<&str>,
    ) -> Result<ConnectionView> {
        let view = self.save_connection(input).await?;
        if let Some(password) = password {
            secrets::store_password(&view.id, password).await?;
        }
        if view.ssh_enabled {
            if let Some(password) = ssh_password {
                secrets::store_ssh_password(&view.id, password).await?;
            }
            if view.ssh_jump_host.is_some() {
                if let Some(password) = jump_password {
                    secrets::store_jump_ssh_password(&view.id, password).await?;
                }
            }
        }
        Ok(view)
    }

    /// Tests a draft before it is persisted. Missing credentials are read from the Secret
    /// Service only for an existing connection, which supports a password-free reconnect test.
    pub async fn test_connection(
        &self,
        input: &ConnectionInput,
        password: Option<&str>,
        ssh_password: Option<&str>,
        jump_password: Option<&str>,
    ) -> Result<()> {
        validate_input(input)?;
        let conn = input.clone().into_core();
        let password = match password {
            Some(password) => password.to_string(),
            None => secrets::get_password(&conn.id).await?,
        };
        let ssh_password = if conn.ssh_enabled {
            Some(match ssh_password {
                Some(password) => password.to_string(),
                None => secrets::get_ssh_password(&conn.id).await?,
            })
        } else {
            None
        };
        let jump_password = if conn.ssh_jump_host.is_some() {
            Some(match jump_password {
                Some(password) => password.to_string(),
                None => secrets::get_jump_ssh_password(&conn.id).await?,
            })
        } else {
            None
        };
        test_connection_with_ssh(
            &conn,
            &password,
            ssh_password.as_deref(),
            jump_password.as_deref(),
        )
        .await?;
        Ok(())
    }

    pub async fn delete_connection(&self, id: &str) -> Result<()> {
        {
            let mut manager = self.manager.lock().await;
            if manager.get(id).is_none() {
                return Err(ApplicationError::ConnectionNotFound(id.to_string()));
            }
            manager.disconnect(id).await;
        }
        store::delete_connection(id)?;
        secrets::remove_password(id).await?;
        secrets::remove_ssh_password(id).await?;
        secrets::remove_jump_ssh_password(id).await?;
        let mut manager = self.manager.lock().await;
        manager.sync_connections(store::list_connections());
        Ok(())
    }

    /// The password and optional SSH secrets are not retained after this call returns.
    pub async fn connect(
        &self,
        id: &str,
        password: &str,
        statement_timeout_ms: u32,
        ssh_password: Option<&str>,
        jump_password: Option<&str>,
    ) -> Result<ConnectionView> {
        let mut manager = self.manager.lock().await;
        if manager.get(id).is_none() {
            return Err(ApplicationError::ConnectionNotFound(id.to_string()));
        }
        manager
            .connect(
                id,
                password,
                statement_timeout_ms,
                ssh_password,
                jump_password,
            )
            .await?;
        let conn = manager
            .get(id)
            .map(|managed| managed.conn.clone())
            .ok_or_else(|| ApplicationError::ConnectionNotFound(id.to_string()))?;
        Ok(self.view_for(&manager, conn))
    }

    /// Connects using credentials held by the system Secret Service. The UI never receives or
    /// sends these values; they exist only in this backend call's stack frame.
    pub async fn connect_stored(
        &self,
        id: &str,
        statement_timeout_ms: u32,
    ) -> Result<ConnectionView> {
        let (ssh_enabled, has_jump_host) = {
            let manager = self.manager.lock().await;
            let managed = manager
                .get(id)
                .ok_or_else(|| ApplicationError::ConnectionNotFound(id.to_string()))?;
            (
                managed.conn.ssh_enabled,
                managed.conn.ssh_jump_host.is_some(),
            )
        };
        let password = secrets::get_password(id).await?;
        let ssh_password = if ssh_enabled {
            Some(secrets::get_ssh_password(id).await?)
        } else {
            None
        };
        let jump_password = if has_jump_host {
            Some(secrets::get_jump_ssh_password(id).await?)
        } else {
            None
        };
        self.connect(
            id,
            &password,
            statement_timeout_ms,
            ssh_password.as_deref(),
            jump_password.as_deref(),
        )
        .await
    }

    pub async fn disconnect(&self, id: &str) -> Result<()> {
        let mut manager = self.manager.lock().await;
        if manager.get(id).is_none() {
            return Err(ApplicationError::ConnectionNotFound(id.to_string()));
        }
        manager.disconnect(id).await;
        Ok(())
    }

    pub async fn execute_query(&self, id: &str, sql: &str) -> Result<QueryResult> {
        self.execute(id, sql, false, None).await
    }

    pub async fn execute_script(&self, id: &str, sql: &str) -> Result<QueryResult> {
        self.execute(id, sql, true, None).await
    }

    /// Executes one query under an explicit operation ID so a concurrent UI command can cancel
    /// exactly this logical operation. The legacy `execute_query` method remains available for
    /// non-interactive callers that do not need cancellation.
    pub async fn execute_query_with_operation(
        &self,
        id: &str,
        operation_id: &str,
        sql: &str,
    ) -> Result<QueryResult> {
        self.execute(id, sql, false, Some(operation_id)).await
    }

    /// Same operation-aware contract as `execute_query_with_operation`, for multi-statement
    /// scripts.
    pub async fn execute_script_with_operation(
        &self,
        id: &str,
        operation_id: &str,
        sql: &str,
    ) -> Result<QueryResult> {
        self.execute(id, sql, true, Some(operation_id)).await
    }

    pub async fn execute_explain(&self, id: &str, sql: &str) -> Result<QueryResult> {
        self.explain(id, sql, None).await
    }

    pub async fn execute_explain_with_operation(
        &self,
        id: &str,
        operation_id: &str,
        sql: &str,
    ) -> Result<QueryResult> {
        self.explain(id, sql, Some(operation_id)).await
    }

    pub async fn cancel_query(&self, id: &str) -> Result<()> {
        let (driver, _) = self.connected_driver(id).await?;
        driver.cancel_active().await?;
        Ok(())
    }

    /// Cancels an operation registered by an operation-aware query, backup, or restore call.
    pub async fn cancel_query_operation(&self, operation_id: &str) -> Result<()> {
        self.cancel_operation(operation_id).await
    }

    pub fn list_history(&self) -> Vec<HistoryView> {
        store::list_history()
            .into_iter()
            .map(history_view)
            .collect()
    }

    pub fn delete_history_entry(&self, id: &str) -> Result<()> {
        store::delete_history_entry(id)?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<()> {
        store::clear_history()?;
        Ok(())
    }

    pub fn list_snippets(&self) -> Vec<SnippetView> {
        store::list_snippets()
            .into_iter()
            .map(snippet_view)
            .collect()
    }

    pub fn save_snippet(&self, input: SnippetInput) -> Result<SnippetView> {
        let name = validate_snippet_name(&input.name)?;
        if input.sql.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Snippet SQL must not be empty".to_string(),
            ));
        }
        let conn_label = input
            .conn_id
            .as_deref()
            .and_then(store::get_connection)
            .map(|connection| connection.label);
        let saved = store::save_snippet(store::Snippet {
            id: String::new(),
            name,
            sql: input.sql,
            created_at: 0,
            conn_id: input.conn_id,
            conn_label,
        })?;
        Ok(snippet_view(saved))
    }

    pub fn delete_snippet(&self, id: &str) -> Result<()> {
        store::delete_snippet(id)?;
        Ok(())
    }

    pub fn rename_snippet(&self, id: &str, name: &str) -> Result<()> {
        if id.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Snippet id is required".to_string(),
            ));
        }
        let name = validate_snippet_name(name)?;
        if !store::list_snippets()
            .iter()
            .any(|snippet| snippet.id == id)
        {
            return Err(ApplicationError::InvalidInput(
                "Snippet not found".to_string(),
            ));
        }
        store::rename_snippet(id, &name)?;
        Ok(())
    }

    pub async fn list_schemas(&self, id: &str) -> Result<Vec<SchemaView>> {
        let (driver, _) = self.connected_driver(id).await?;
        Ok(queries::get_schemas(&driver)
            .await?
            .into_iter()
            .map(|schema| SchemaView { name: schema.name })
            .collect())
    }

    pub async fn list_tables(&self, id: &str, schema: &str) -> Result<Vec<TableView>> {
        let (driver, _) = self.connected_driver(id).await?;
        let (tables, estimates) = tokio::join!(
            queries::get_tables(&driver, schema),
            queries::get_table_estimates(&driver, schema),
        );
        let estimates = estimates?.into_iter().collect::<HashMap<_, _>>();
        Ok(tables?
            .into_iter()
            .map(|table| {
                let name = table.name;
                TableView {
                    estimated_rows: estimates.get(&name).copied().unwrap_or_default(),
                    name,
                    kind: match table.kind {
                        queries::TableKind::Table => ObjectKind::Table,
                        queries::TableKind::View => ObjectKind::View,
                    },
                }
            })
            .collect())
    }

    pub async fn list_schema_objects(
        &self,
        id: &str,
        schema: &str,
    ) -> Result<Vec<SchemaObjectView>> {
        let (driver, _) = self.connected_driver(id).await?;
        let (functions, sequences, triggers) = tokio::join!(
            queries::get_functions(&driver, schema),
            queries::get_sequences(&driver, schema),
            queries::get_triggers(&driver, schema),
        );
        let mut objects = functions?
            .into_iter()
            .map(|routine| SchemaObjectView {
                name: routine.name,
                kind: match routine.kind {
                    queries::RoutineKind::Function => SchemaObjectKind::Function,
                    queries::RoutineKind::Procedure => SchemaObjectKind::Procedure,
                },
                detail: (!routine.return_type.is_empty()).then_some(routine.return_type),
            })
            .collect::<Vec<_>>();
        objects.extend(sequences?.into_iter().map(|sequence| SchemaObjectView {
            name: sequence.name,
            kind: SchemaObjectKind::Sequence,
            detail: None,
        }));
        objects.extend(triggers?.into_iter().map(|trigger| SchemaObjectView {
            name: trigger.name,
            kind: SchemaObjectKind::Trigger,
            detail: Some(format!("on {}", trigger.table)),
        }));
        Ok(objects)
    }

    /// Whole-database completion data (schemas, tables, columns, functions) for the SQL editor's
    /// autocomplete. One round trip per connection; the frontend caches the result.
    pub async fn completion_data(&self, id: &str) -> Result<CompletionDataView> {
        let (driver, _) = self.connected_driver(id).await?;
        let data = queries::get_completion_data(&driver).await?;
        Ok(CompletionDataView {
            schemas: data.schemas,
            tables: data
                .tables
                .into_iter()
                .map(|table| CompletionTableView {
                    schema: table.schema,
                    name: table.name,
                    kind: match table.kind {
                        queries::TableKind::Table => ObjectKind::Table,
                        queries::TableKind::View => ObjectKind::View,
                    },
                })
                .collect(),
            columns: data
                .columns
                .into_iter()
                .map(|column| CompletionColumnView {
                    schema: column.schema,
                    table: column.table,
                    name: column.name,
                })
                .collect(),
            functions: data
                .functions
                .into_iter()
                .map(|function| CompletionFunctionView {
                    schema: function.schema,
                    name: function.name,
                })
                .collect(),
        })
    }

    pub async fn global_search(&self, id: &str, term: &str) -> Result<Vec<SearchResultView>> {
        let term = term.trim();
        if term.is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Search term is required".to_string(),
            ));
        }
        let (driver, _) = self.connected_driver(id).await?;
        Ok(queries::global_search(&driver, term)
            .await?
            .into_iter()
            .map(|result| SearchResultView {
                kind: match result.kind {
                    queries::SearchKind::Table => SearchObjectKind::Table,
                    queries::SearchKind::View => SearchObjectKind::View,
                    queries::SearchKind::Column => SearchObjectKind::Column,
                    queries::SearchKind::Function => SearchObjectKind::Function,
                },
                schema: result.schema,
                table: result.table,
                name: result.name,
                detail: result.detail,
            })
            .collect())
    }

    pub async fn dashboard(&self, id: &str) -> Result<DashboardView> {
        let (driver, _) = self.connected_driver(id).await?;
        let (dashboard, stats) = tokio::join!(
            queries::get_dashboard(&driver),
            queries::get_db_stats(&driver),
        );
        Ok(DashboardView {
            dashboard: serde_json::to_value(dashboard?).unwrap_or(serde_json::Value::Null),
            stats: serde_json::to_value(stats?).unwrap_or(serde_json::Value::Null),
        })
    }

    pub async fn table_detail(
        &self,
        id: &str,
        schema: &str,
        table: &str,
    ) -> Result<TableDetailView> {
        let (driver, _) = self.connected_driver(id).await?;
        let (detail, ddl, column_stats) = tokio::join!(
            queries::get_table_detail(&driver, schema, table),
            queries::get_table_ddl(&driver, schema, table),
            queries::get_column_stats(&driver, schema, table),
        );
        Ok(TableDetailView {
            schema: schema.to_string(),
            table: table.to_string(),
            detail: serde_json::to_value(detail?).unwrap_or(serde_json::Value::Null),
            ddl: ddl?,
            column_stats: serde_json::to_value(column_stats?).unwrap_or(serde_json::Value::Null),
        })
    }

    pub async fn erd(&self, id: &str, schema: &str) -> Result<ErdView> {
        let (driver, _) = self.connected_driver(id).await?;
        let data = queries::get_erd_data(&driver, schema).await?;
        Ok(ErdView {
            schema: schema.to_string(),
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
        })
    }

    pub async fn admin(&self, id: &str) -> Result<AdminView> {
        let (driver, _) = self.connected_driver(id).await?;
        let (activity, locks) =
            tokio::join!(queries::get_activity(&driver), queries::get_locks(&driver),);
        Ok(AdminView {
            activity: serde_json::to_value(activity?).unwrap_or(serde_json::Value::Null),
            locks: serde_json::to_value(locks?).unwrap_or(serde_json::Value::Null),
        })
    }

    pub async fn cancel_activity(&self, id: &str, pid: i32) -> Result<()> {
        if pid <= 0 {
            return Err(ApplicationError::InvalidInput(
                "Activity PID must be positive".to_string(),
            ));
        }
        let (driver, _) = self.connected_driver(id).await?;
        queries::cancel_activity(&driver, pid).await?;
        Ok(())
    }

    pub async fn list_cron_jobs(&self, id: &str) -> Result<CronJobsView> {
        let (driver, _) = self.connected_driver(id).await?;
        let jobs = queries::get_jobs(&driver).await?;
        Ok(CronJobsView {
            installed: jobs.installed,
            jobs: jobs
                .jobs
                .into_iter()
                .map(|job| CronJobView {
                    id: job.jobid,
                    name: job.jobname,
                    schedule: job.schedule,
                    command: job.command,
                    database: job.database,
                    username: job.username,
                    active: job.active,
                    last_status: job.last_status,
                    last_run: job.last_run,
                    last_message: job.last_msg,
                })
                .collect(),
        })
    }

    pub async fn set_cron_job_active(&self, id: &str, job_id: i64, active: bool) -> Result<()> {
        validate_job_id(job_id)?;
        let (driver, _) = self.connected_driver(id).await?;
        queries::toggle_job(&driver, job_id, active).await?;
        Ok(())
    }

    pub async fn delete_cron_job(&self, id: &str, job_id: i64) -> Result<()> {
        validate_job_id(job_id)?;
        let (driver, _) = self.connected_driver(id).await?;
        queries::delete_job(&driver, job_id).await?;
        Ok(())
    }

    pub async fn list_extensions(&self, id: &str) -> Result<ExtensionsView> {
        let (driver, _) = self.connected_driver(id).await?;
        let extensions = queries::get_extensions(&driver).await?;
        let to_view = |extension: queries::ExtensionInfo| ExtensionView {
            name: extension.name,
            default_version: extension.default_version,
            installed_version: extension.installed_version,
            comment: extension.comment,
        };
        Ok(ExtensionsView {
            installed: extensions.installed.into_iter().map(to_view).collect(),
            available: extensions.available.into_iter().map(to_view).collect(),
        })
    }

    pub async fn install_extension(&self, id: &str, name: &str) -> Result<()> {
        let name = validate_extension_name(name)?;
        let (driver, _) = self.connected_driver(id).await?;
        queries::ext_install(&driver, &name).await?;
        Ok(())
    }

    pub async fn drop_extension(&self, id: &str, name: &str) -> Result<()> {
        let name = validate_extension_name(name)?;
        if name.eq_ignore_ascii_case("plpgsql") {
            return Err(ApplicationError::InvalidInput(
                "The built-in plpgsql extension is protected".to_string(),
            ));
        }
        let (driver, _) = self.connected_driver(id).await?;
        queries::ext_drop(&driver, &name).await?;
        Ok(())
    }

    pub async fn query_stats(&self, id: &str) -> Result<QueryStatsView> {
        let (driver, _) = self.connected_driver(id).await?;
        let stats = queries::get_query_stats(&driver).await?;
        Ok(QueryStatsView {
            installed: stats.installed,
            queries: stats
                .queries
                .into_iter()
                .map(|row| QueryStatView {
                    query: row.query,
                    calls: row.calls,
                    total_exec_ms: row.total_exec_ms,
                    mean_exec_ms: row.mean_exec_ms,
                    rows: row.rows,
                })
                .collect(),
        })
    }

    pub async fn reset_query_stats(&self, id: &str) -> Result<()> {
        let (driver, _) = self.connected_driver(id).await?;
        queries::reset_query_stats(&driver).await?;
        Ok(())
    }

    pub async fn run_table_maintenance(
        &self,
        id: &str,
        schema: &str,
        table: &str,
        operation: TableMaintenanceOperation,
    ) -> Result<()> {
        if schema.is_empty() || table.is_empty() || schema.contains('\0') || table.contains('\0') {
            return Err(ApplicationError::InvalidInput(
                "Schema and table are required for maintenance".to_string(),
            ));
        }
        let (driver, _) = self.connected_driver(id).await?;
        queries::run_vacuum(&driver, schema, table, operation.sql()).await?;
        Ok(())
    }

    pub async fn list_roles(&self, id: &str) -> Result<Vec<RoleView>> {
        let (driver, _) = self.connected_driver(id).await?;
        Ok(queries::get_roles(&driver)
            .await?
            .into_iter()
            .map(|role| RoleView {
                name: role.rolname,
                superuser: role.rolsuper,
                create_database: role.rolcreatedb,
                create_role: role.rolcreaterole,
                login: role.rolcanlogin,
                connection_limit: role.rolconnlimit,
                valid_until: role.rolvaliduntil,
            })
            .collect())
    }

    pub async fn create_role(&self, id: &str, input: CreateRoleInput) -> Result<()> {
        let name = validate_role_name(&input.name)?;
        if input.connection_limit < -1 {
            return Err(ApplicationError::InvalidInput(
                "Role connection limit must be -1 or greater".to_string(),
            ));
        }
        let (driver, _) = self.connected_driver(id).await?;
        queries::create_role(
            &driver,
            &queries::NewRole {
                name,
                login: input.login,
                createdb: input.create_database,
                createrole: input.create_role,
                superuser: input.superuser,
                conn_limit: input.connection_limit,
                ..Default::default()
            },
        )
        .await?;
        Ok(())
    }

    pub async fn delete_role(&self, id: &str, name: &str) -> Result<()> {
        let name = validate_role_name(name)?;
        let (driver, _) = self.connected_driver(id).await?;
        queries::drop_role(&driver, &name).await?;
        Ok(())
    }

    /// Runs `pg_dump` with credentials supplied through Secret Service and returns bounded logs.
    /// The caller chooses the operation id up front so a second IPC call can cancel it while it
    /// is running.
    pub async fn run_backup(
        &self,
        id: &str,
        operation_id: &str,
        options: BackupOptionsInput,
    ) -> Result<ToolResultView> {
        if options.output.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Backup output is required".to_string(),
            ));
        }
        validate_file_path(&options.output, "Backup output")?;
        if options.compression.is_some_and(|level| level > 9) {
            return Err(ApplicationError::InvalidInput(
                "Backup compression must be between 0 and 9".to_string(),
            ));
        }
        let format = match options.format {
            BackupFormat::Plain => DumpFormat::Plain,
            BackupFormat::Custom => DumpFormat::Custom,
            BackupFormat::Directory => DumpFormat::Directory,
        };
        let core_options = DumpOptions {
            output: options.output.into(),
            format,
            compression: options.compression,
            schemas: options.schemas,
            tables: options.tables,
        };
        self.run_tool_operation(id, operation_id, ToolOperation::Dump(core_options))
            .await
    }

    /// Runs `pg_restore`/`psql` with the connection's stored credentials.
    pub async fn run_restore(
        &self,
        id: &str,
        operation_id: &str,
        options: RestoreOptionsInput,
    ) -> Result<ToolResultView> {
        if options.input.trim().is_empty() || options.target_database.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Restore input and target database are required".to_string(),
            ));
        }
        validate_file_path(&options.input, "Restore input")?;
        let core_options = RestoreOptions {
            input: options.input.into(),
            target_database: options.target_database,
            clean: options.clean,
            single_transaction: options.single_transaction,
        };
        self.run_tool_operation(id, operation_id, ToolOperation::Restore(core_options))
            .await
    }

    pub async fn cancel_operation(&self, operation_id: &str) -> Result<()> {
        let sender = self.operations.lock().await.get(operation_id).cloned();
        match sender {
            Some(sender) => sender.send(true).map_err(|_| {
                ApplicationError::Operation("Operation has already finished".to_string())
            }),
            None => Err(ApplicationError::Operation(
                "Operation not found".to_string(),
            )),
        }
    }

    pub fn assistant_settings(&self) -> assistant::Settings {
        store::get_ai_settings()
    }

    pub fn save_assistant_settings(
        &self,
        settings: assistant::Settings,
    ) -> Result<assistant::Settings> {
        store::save_ai_settings(&settings)?;
        Ok(settings)
    }

    pub async fn save_assistant_key(&self, provider: assistant::Provider, key: &str) -> Result<()> {
        assistant::save_key(provider, key)
            .await
            .map_err(|error| ApplicationError::Assistant(error.to_string()))
    }

    pub async fn clear_assistant_key(&self, provider: assistant::Provider) -> Result<()> {
        assistant::clear_key(provider)
            .await
            .map_err(|error| ApplicationError::Assistant(error.to_string()))
    }

    pub fn assistant_history(&self, id: &str) -> Vec<assistant::AiMessage> {
        store::get_ai_history(id)
    }

    pub fn clear_assistant_history(&self, id: &str) -> Result<()> {
        store::clear_ai_history(id)?;
        Ok(())
    }

    pub async fn assistant_send(&self, id: &str, message: &str) -> Result<AssistantReplyView> {
        let message = message.trim();
        if message.is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Assistant message is required".to_string(),
            ));
        }
        let settings = store::get_ai_settings();
        let mut history = store::get_ai_history(id);
        history.push(assistant::AiMessage {
            role: assistant::AiRole::User,
            content: message.to_string(),
            tool_label: None,
        });
        store::save_ai_history(id, &history)?;

        let max_rounds = settings.max_rounds_per_message.clamp(1, 20);
        let mut final_reply = None;
        let mut input_tokens = 0;
        let mut output_tokens = 0;
        let mut estimated_cost_usd = 0.0;
        let mut has_cost = false;
        for round in 0..max_rounds {
            let reply = if round == 0 {
                assistant::send(&settings, &history).await
            } else {
                assistant::continue_after_tool(&settings, &history).await
            }
            .map_err(|error| ApplicationError::Assistant(error.to_string()))?;
            input_tokens += reply.input_tokens;
            output_tokens += reply.output_tokens;
            if let Some(cost) = reply.estimated_cost_usd {
                estimated_cost_usd += cost;
                has_cost = true;
            }
            if !reply.text.is_empty() {
                final_reply = Some(reply.text.clone());
                history.push(assistant::AiMessage {
                    role: assistant::AiRole::Assistant,
                    content: reply.text,
                    tool_label: None,
                });
            }
            let has_tools = !reply.tool_calls.is_empty();
            for call in reply.tool_calls {
                let (driver, _) = self.connected_driver(id).await?;
                let outcome = assistant::run_tool(&driver, &call)
                    .await
                    .map_err(|error| error.to_string());
                let outcome = outcome.unwrap_or_else(|error| format!("ERRO: {error}"));
                history.push(assistant::AiMessage {
                    role: assistant::AiRole::User,
                    content: format!(
                        "<dado_nao_confiavel origem=\"tool:{}\">\n{}\n</dado_nao_confiavel>\nContinue a resposta usando este resultado (ou corrija a chamada, se for um erro).",
                        call.name, outcome
                    ),
                    tool_label: Some(call.name),
                });
            }
            store::save_ai_history(id, &history)?;
            if !has_tools {
                break;
            }
        }
        Ok(AssistantReplyView {
            text: final_reply.unwrap_or_default(),
            input_tokens,
            output_tokens,
            estimated_cost_usd: has_cost.then_some(estimated_cost_usd),
            history,
        })
    }

    async fn run_tool_operation(
        &self,
        id: &str,
        operation_id: &str,
        operation: ToolOperation,
    ) -> Result<ToolResultView> {
        if operation_id.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Operation id is required".to_string(),
            ));
        }
        let (driver, conn) = self.connected_context(id).await?;
        let password = secrets::get_password(id).await?;
        let redacted_paths = operation.paths_for_redaction();
        let cancel_rx = self.register_operation(operation_id).await?;
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let operation_id_owned = operation_id.to_string();
        let task = tokio::spawn(async move {
            let connection = ToolConnection {
                password: &password,
                database: &conn.database,
                user: &conn.user,
                ssl: conn.ssl,
            };
            match operation {
                ToolOperation::Dump(options) => {
                    backup::run_dump(&driver, connection, options, cancel_rx, events_tx).await
                }
                ToolOperation::Restore(options) => {
                    backup::run_restore(&driver, connection, options, cancel_rx, events_tx).await
                }
            }
        });
        let mut logs = Vec::new();
        while let Some(event) = events_rx.recv().await {
            match event {
                ToolEvent::Stdout(line) => push_log(
                    &mut logs,
                    redact_paths(format!("stdout: {line}"), &redacted_paths),
                ),
                ToolEvent::Stderr(line) => push_log(
                    &mut logs,
                    redact_paths(format!("stderr: {line}"), &redacted_paths),
                ),
                ToolEvent::Finished(_) => {}
            }
        }
        let result = match task.await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                self.finish_operation(operation_id).await;
                return Err(error.into());
            }
            Err(error) => {
                self.finish_operation(operation_id).await;
                return Err(ApplicationError::Operation(error.to_string()));
            }
        };
        self.finish_operation(operation_id).await;
        Ok(ToolResultView {
            operation_id: operation_id_owned,
            exit_code: result.exit_code,
            cancelled: result.cancelled,
            succeeded: result.succeeded(),
            logs,
        })
    }

    async fn execute(
        &self,
        id: &str,
        sql: &str,
        script: bool,
        operation_id: Option<&str>,
    ) -> Result<QueryResult> {
        if sql.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "SQL must not be empty".to_string(),
            ));
        }
        let (driver, conn_label) = self.connected_driver(id).await?;
        let cancel_rx = match operation_id {
            Some(operation_id) => Some(self.register_operation(operation_id).await?),
            None => None,
        };
        let started = Instant::now();
        let core_result = if let Some(cancel_rx) = cancel_rx {
            if script {
                queries::execute_script_cancelable(&driver, sql, cancel_rx).await
            } else {
                queries::execute_query_cancelable(&driver, sql, cancel_rx).await
            }
        } else {
            if script {
                queries::execute_script(&driver, sql).await
            } else {
                queries::execute_query(&driver, sql).await
            }
        };
        if let Some(operation_id) = operation_id {
            self.finish_operation(operation_id).await;
        }
        let result = core_result?;
        let duration_ms = started.elapsed().as_millis() as i64;
        let _ = store::add_history(store::HistoryEntry {
            id: String::new(),
            sql: sql.to_string(),
            conn_id: id.to_string(),
            conn_label,
            timestamp: now_millis(),
            duration_ms,
            row_count: result.rows.len() as i64,
        });
        Ok(QueryResult {
            columns: result.columns,
            rows: result.rows,
            duration_ms: duration_ms as u64,
        })
    }

    async fn explain(
        &self,
        id: &str,
        sql: &str,
        operation_id: Option<&str>,
    ) -> Result<QueryResult> {
        if sql.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "SQL must not be empty".to_string(),
            ));
        }
        let (driver, conn_label) = self.connected_driver(id).await?;
        let cancel_rx = match operation_id {
            Some(operation_id) => Some(self.register_operation(operation_id).await?),
            None => None,
        };
        let started = Instant::now();
        let core_result = match cancel_rx {
            Some(cancel_rx) => queries::execute_explain_cancelable(&driver, sql, cancel_rx).await,
            None => queries::execute_explain(&driver, sql).await,
        };
        if let Some(operation_id) = operation_id {
            self.finish_operation(operation_id).await;
        }
        let plan = core_result?;
        let duration_ms = started.elapsed().as_millis() as i64;
        let _ = store::add_history(store::HistoryEntry {
            id: String::new(),
            sql: sql.to_string(),
            conn_id: id.to_string(),
            conn_label,
            timestamp: now_millis(),
            duration_ms,
            row_count: 1,
        });
        let mut row = serde_json::Map::new();
        row.insert("QUERY PLAN".to_string(), plan);
        Ok(QueryResult {
            columns: vec!["QUERY PLAN".to_string()],
            rows: vec![row],
            duration_ms: duration_ms as u64,
        })
    }

    async fn register_operation(&self, operation_id: &str) -> Result<watch::Receiver<bool>> {
        if operation_id.trim().is_empty() {
            return Err(ApplicationError::InvalidInput(
                "Operation id is required".to_string(),
            ));
        }
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let mut operations = self.operations.lock().await;
        if operations.contains_key(operation_id) {
            return Err(ApplicationError::Operation(
                "Operation id is already in use".to_string(),
            ));
        }
        operations.insert(operation_id.to_string(), cancel_tx);
        Ok(cancel_rx)
    }

    async fn finish_operation(&self, operation_id: &str) {
        self.operations.lock().await.remove(operation_id);
    }

    async fn connected_driver(&self, id: &str) -> Result<(PostgresDriver, String)> {
        let manager = self.manager.lock().await;
        let managed = manager
            .get(id)
            .ok_or_else(|| ApplicationError::ConnectionNotFound(id.to_string()))?;
        if managed.status != ConnectionStatus::Connected {
            return Err(ApplicationError::ConnectionNotActive(id.to_string()));
        }
        let driver = manager
            .get_driver_handle(id)
            .ok_or_else(|| ApplicationError::ConnectionNotActive(id.to_string()))?;
        Ok((driver, managed.conn.label.clone()))
    }

    async fn connected_context(&self, id: &str) -> Result<(PostgresDriver, DbConnection)> {
        let manager = self.manager.lock().await;
        let managed = manager
            .get(id)
            .ok_or_else(|| ApplicationError::ConnectionNotFound(id.to_string()))?;
        if managed.status != ConnectionStatus::Connected {
            return Err(ApplicationError::ConnectionNotActive(id.to_string()));
        }
        let driver = manager
            .get_driver_handle(id)
            .ok_or_else(|| ApplicationError::ConnectionNotActive(id.to_string()))?;
        Ok((driver, managed.conn.clone()))
    }

    fn view_for(&self, manager: &ConnectionManager, conn: DbConnection) -> ConnectionView {
        let managed = manager.get(&conn.id);
        ConnectionView {
            id: conn.id,
            label: conn.label,
            host: conn.host,
            port: conn.port,
            database: conn.database,
            user: conn.user,
            ssl: conn.ssl,
            ssh_enabled: conn.ssh_enabled,
            ssh_host: conn.ssh_host,
            ssh_port: conn.ssh_port,
            ssh_user: conn.ssh_user,
            ssh_key_path: conn.ssh_key_path,
            ssh_jump_host: conn.ssh_jump_host,
            ssh_jump_port: conn.ssh_jump_port,
            ssh_jump_user: conn.ssh_jump_user,
            ssh_jump_key_path: conn.ssh_jump_key_path,
            favorite: conn.favorite,
            state: managed
                .map(|m| m.status.into())
                .unwrap_or(ConnectionState::Disconnected),
            error: managed.and_then(|m| m.error.as_ref().map(|_| "Connection failed".to_string())),
        }
    }
}

enum ToolOperation {
    Dump(DumpOptions),
    Restore(RestoreOptions),
}

impl ToolOperation {
    fn paths_for_redaction(&self) -> Vec<String> {
        match self {
            Self::Dump(options) => vec![options.output.display().to_string()],
            Self::Restore(options) => vec![options.input.display().to_string()],
        }
    }
}

fn push_log(logs: &mut Vec<String>, line: String) {
    const MAX_LOG_LINES: usize = 2_000;
    if logs.len() < MAX_LOG_LINES {
        logs.push(line);
    }
}

fn redact_paths(mut line: String, paths: &[String]) -> String {
    for path in paths.iter().filter(|path| !path.is_empty()) {
        line = line.replace(path, "<selected-path>");
    }
    line
}

/// File operations are intentionally limited to an explicit absolute path supplied by the user
/// (normally from a future native file picker). Relative paths and parent traversal are rejected;
/// no generic filesystem command is exposed to the webview.
fn validate_file_path(value: &str, label: &str) -> Result<()> {
    let path = Path::new(value.trim());
    if !path.is_absolute()
        || value.contains('\0')
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(ApplicationError::InvalidInput(format!(
            "{label} must be an absolute path without parent traversal"
        )));
    }
    Ok(())
}

fn validate_role_name(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(ApplicationError::InvalidInput(
            "Role name is required".to_string(),
        ));
    }
    if name.len() > 63 {
        return Err(ApplicationError::InvalidInput(
            "Role name must be at most 63 bytes".to_string(),
        ));
    }
    if name.contains('\0') || name.chars().any(char::is_control) {
        return Err(ApplicationError::InvalidInput(
            "Role name contains invalid characters".to_string(),
        ));
    }
    if name.to_ascii_lowercase().starts_with("pg_") {
        return Err(ApplicationError::InvalidInput(
            "Roles beginning with pg_ are reserved by PostgreSQL".to_string(),
        ));
    }
    Ok(name.to_string())
}

fn validate_snippet_name(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(ApplicationError::InvalidInput(
            "Snippet name is required".to_string(),
        ));
    }
    if name.chars().count() > 120 {
        return Err(ApplicationError::InvalidInput(
            "Snippet name must be at most 120 characters".to_string(),
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(ApplicationError::InvalidInput(
            "Snippet name contains invalid characters".to_string(),
        ));
    }
    Ok(name.to_string())
}

fn validate_job_id(job_id: i64) -> Result<()> {
    if job_id <= 0 {
        Err(ApplicationError::InvalidInput(
            "Scheduled job id must be positive".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn validate_extension_name(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(ApplicationError::InvalidInput(
            "Extension name is required".to_string(),
        ));
    }
    if name.len() > 63 || name.chars().any(char::is_control) {
        return Err(ApplicationError::InvalidInput(
            "Extension name contains invalid characters".to_string(),
        ));
    }
    Ok(name.to_string())
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionInput {
    fn into_core(self) -> DbConnection {
        DbConnection {
            id: self.id.unwrap_or_default(),
            label: self.label,
            host: self.host,
            port: self.port as u16,
            database: self.database,
            user: self.user,
            ssl: self.ssl,
            ssh_enabled: self.ssh_enabled,
            ssh_host: self.ssh_host,
            ssh_port: self.ssh_port,
            ssh_user: self.ssh_user,
            ssh_key_path: self.ssh_key_path,
            ssh_jump_host: self.ssh_jump_host,
            ssh_jump_port: self.ssh_jump_port,
            ssh_jump_user: self.ssh_jump_user,
            ssh_jump_key_path: self.ssh_jump_key_path,
            favorite: self.favorite,
        }
    }
}

fn validate_input(input: &ConnectionInput) -> Result<()> {
    let draft = DbConnectionDraft {
        label: input.label.clone(),
        host: input.host.clone(),
        port: input.port,
        database: input.database.clone(),
        user: input.user.clone(),
        ssl: input.ssl,
    };
    let errors = validate_connection(&draft);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApplicationError::InvalidConnection(errors.join("; ")))
    }
}

fn history_view(entry: store::HistoryEntry) -> HistoryView {
    HistoryView {
        id: entry.id,
        sql: entry.sql,
        conn_id: entry.conn_id,
        conn_label: entry.conn_label,
        timestamp: entry.timestamp,
        duration_ms: entry.duration_ms,
        row_count: entry.row_count,
    }
}

fn snippet_view(snippet: store::Snippet) -> SnippetView {
    SnippetView {
        id: snippet.id,
        name: snippet.name,
        sql: snippet.sql,
        created_at: snippet.created_at,
        conn_id: snippet.conn_id,
        conn_label: snippet.conn_label,
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    fn parse(value: &str) -> Option<Vec<u64>> {
        value
            .trim_start_matches(['v', 'V'])
            .split_once('-')
            .map_or(value.trim_start_matches(['v', 'V']), |(stable, _)| stable)
            .split('.')
            .map(str::parse::<u64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()
    }

    match (parse(candidate), parse(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> ConnectionInput {
        ConnectionInput {
            id: Some("local".to_string()),
            label: "Local".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            database: "draco".to_string(),
            user: "postgres".to_string(),
            ssl: false,
            ssh_enabled: false,
            ssh_host: None,
            ssh_port: None,
            ssh_user: None,
            ssh_key_path: None,
            ssh_jump_host: None,
            ssh_jump_port: None,
            ssh_jump_user: None,
            ssh_jump_key_path: None,
            favorite: false,
        }
    }

    #[test]
    fn health_is_ready_without_a_database_connection() {
        assert!(Application::new().health().ready);
    }

    #[test]
    fn input_does_not_have_a_password_field() {
        let core = valid_input().into_core();
        let json = serde_json::to_string(&core).expect("connection serializes");
        assert!(!json.contains("password"));
        assert!(!json.contains("passphrase"));
    }

    #[test]
    fn schema_object_kinds_use_stable_lowercase_values() {
        assert_eq!(
            serde_json::to_string(&SchemaObjectKind::Procedure).unwrap(),
            "\"procedure\""
        );
        assert_eq!(
            serde_json::to_string(&SchemaObjectKind::Sequence).unwrap(),
            "\"sequence\""
        );
    }

    #[test]
    fn invalid_input_is_rejected_before_storage() {
        let mut input = valid_input();
        input.host.clear();
        let errors = validate_connection(&DbConnectionDraft {
            label: input.label,
            host: input.host,
            port: input.port,
            database: input.database,
            user: input.user,
            ssl: input.ssl,
        });
        assert_eq!(errors, vec!["Host is required"]);
    }

    #[test]
    fn file_operations_reject_relative_and_traversal_paths() {
        assert!(validate_file_path("backup.dump", "Backup output").is_err());
        assert!(validate_file_path("/tmp/../backup.dump", "Backup output").is_err());
        assert!(validate_file_path("/tmp/backup.dump", "Backup output").is_ok());
    }

    #[test]
    fn role_names_are_normalized_and_reserved_names_are_rejected() {
        assert_eq!(validate_role_name("  analyst  ").unwrap(), "analyst");
        assert!(validate_role_name("").is_err());
        assert!(validate_role_name("pg_read_all_data").is_err());
        assert!(validate_role_name(&"a".repeat(64)).is_err());
        assert!(validate_role_name("line\nbreak").is_err());
    }

    #[test]
    fn role_input_has_no_password_or_secret_field() {
        let input = CreateRoleInput {
            name: "analyst".to_string(),
            login: true,
            create_database: false,
            create_role: false,
            superuser: false,
            connection_limit: -1,
        };
        let json = serde_json::to_string(&input).expect("role input serializes");
        assert!(!json.contains("password"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn snippet_names_are_normalized_and_bounded() {
        assert_eq!(
            validate_snippet_name("  Daily report  ").unwrap(),
            "Daily report"
        );
        assert!(validate_snippet_name("  ").is_err());
        assert!(validate_snippet_name("line\nbreak").is_err());
        assert!(validate_snippet_name(&"a".repeat(121)).is_err());
    }

    #[tokio::test]
    async fn invalid_activity_pid_is_rejected_before_connection_lookup() {
        let error = Application::new()
            .cancel_activity("missing", 0)
            .await
            .expect_err("zero PID must be rejected");
        assert!(matches!(error, ApplicationError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn invalid_cron_job_id_is_rejected_before_connection_lookup() {
        let app = Application::new();
        assert!(matches!(
            app.set_cron_job_active("missing", 0, false).await,
            Err(ApplicationError::InvalidInput(_))
        ));
        assert!(matches!(
            app.delete_cron_job("missing", -1).await,
            Err(ApplicationError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn completion_data_reports_connection_not_found_for_unknown_id() {
        let error = Application::new()
            .completion_data("missing")
            .await
            .expect_err("unknown connection must be rejected");
        assert!(matches!(error, ApplicationError::ConnectionNotFound(_)));
    }

    #[test]
    fn completion_data_view_defaults_to_empty_collections() {
        let data = CompletionDataView::default();
        assert!(data.schemas.is_empty());
        assert!(data.tables.is_empty());
        assert!(data.columns.is_empty());
        assert!(data.functions.is_empty());
    }

    #[test]
    fn extension_names_are_normalized_and_bounded() {
        assert_eq!(validate_extension_name("  pg_trgm  ").unwrap(), "pg_trgm");
        assert!(validate_extension_name("").is_err());
        assert!(validate_extension_name("line\nbreak").is_err());
        assert!(validate_extension_name(&"a".repeat(64)).is_err());
    }

    #[tokio::test]
    async fn built_in_plpgsql_is_protected_before_connection_lookup() {
        let error = Application::new()
            .drop_extension("missing", "PLPGSQL")
            .await
            .expect_err("plpgsql must be protected");
        assert!(matches!(error, ApplicationError::InvalidInput(_)));
    }

    #[test]
    fn maintenance_operations_have_stable_values_and_allowlisted_sql() {
        assert_eq!(
            serde_json::to_string(&TableMaintenanceOperation::VacuumAnalyze).unwrap(),
            "\"vacuum_analyze\""
        );
        assert_eq!(TableMaintenanceOperation::Vacuum.sql(), "VACUUM");
        assert_eq!(TableMaintenanceOperation::Analyze.sql(), "ANALYZE");
        assert_eq!(TableMaintenanceOperation::VacuumFull.sql(), "VACUUM FULL");
    }

    #[test]
    fn file_operation_errors_do_not_echo_private_paths() {
        let error = validate_file_path("relative/private-secret.dump", "Backup output")
            .expect_err("relative paths must be rejected");
        assert!(!error.to_string().contains("private-secret"));
    }

    #[test]
    fn backup_logs_redact_selected_paths() {
        let line = redact_paths(
            "could not open /home/user/private.dump".to_string(),
            &["/home/user/private.dump".to_string()],
        );
        assert_eq!(line, "could not open <selected-path>");
    }

    #[test]
    fn appearance_defaults_are_dark_and_legacy_settings_still_load() {
        let settings: store::AppSettings = serde_json::from_str(
            r#"{"query_timeout":10000,"preview_row_limit":50,"show_row_count":true}"#,
        )
        .expect("legacy settings deserialize");
        assert_eq!(settings.theme, AppTheme::Dark);
        assert_eq!(settings.accent, AccentColor::Coral);
        assert!(settings.check_updates_on_startup);
        assert_eq!(
            serde_json::to_string(&AppTheme::Light).unwrap(),
            "\"light\""
        );
        assert_eq!(
            serde_json::to_string(&AccentColor::Blue).unwrap(),
            "\"blue\""
        );
    }

    #[test]
    fn update_versions_are_compared_numerically() {
        assert!(is_newer_version("v1.2.0", "1.1.9"));
        assert!(is_newer_version("2.0.0", "1.99.99"));
        assert!(!is_newer_version("1.1.3", "1.1.3"));
        assert!(!is_newer_version("1.1.2", "1.1.3"));
        assert!(!is_newer_version("not-a-version", "1.1.3"));
    }

    #[tokio::test]
    async fn operation_registry_is_unique_cancelable_and_reusable() {
        let app = Application::new();
        let cancellation = app
            .register_operation("query-1")
            .await
            .expect("first operation should register");
        assert!(app.register_operation("query-1").await.is_err());

        app.cancel_operation("query-1")
            .await
            .expect("registered operation should be cancelable");
        assert!(*cancellation.borrow());

        app.finish_operation("query-1").await;
        assert!(app.cancel_operation("query-1").await.is_err());
        assert!(app.register_operation("query-1").await.is_ok());
    }
}
