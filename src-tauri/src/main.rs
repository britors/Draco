#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::{Component, Path, PathBuf};
use std::{fs, io};

use draco_app::{
    AdminView, AlterTableInput, AlterTablePreviewView, Application, ApplicationError,
    AssistantReplyView, BackupFormat, BackupOptionsInput, BrowseTableView, CompletionDataView,
    ConnectionInput, ConnectionView, CreateRoleInput, CreateTableInput, CronJobInput,
    CronJobRunView, CronJobsView, DashboardView, DeleteTableRowInput, ErdView, ExtensionsView,
    FileAuthorizationPurpose, FunctionDefinitionView, GithubBranch, GithubConnection,
    GithubPullRequest, GithubRepository, GithubSettings, Health, HistoryView, InsertTableRowInput, PreferencesView,
    QueryResult, QueryStatsView, RestoreOptionsInput, RoleView, SchemaObjectView, SchemaView,
    SearchResultView, SnippetInput, SnippetView, TableDetailView, TableMaintenanceOperation,
    TableView, ToolResultView, TriggerInput, UpdateRoleInput, UpdateStatusView,
    UpdateTableCellInput,
};
use draco_core::assistant::{AiMessage, Provider, Settings};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize)]
struct CommandError {
    code: &'static str,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ConnectionSecretRequest {
    input: ConnectionInput,
    password: Option<String>,
    ssh_password: Option<String>,
    jump_password: Option<String>,
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::InvalidConnection(message) => Self {
                code: "invalid_input",
                message,
            },
            ApplicationError::InvalidInput(message) => Self {
                code: "invalid_input",
                message,
            },
            ApplicationError::ConnectionNotFound(id) => Self {
                code: "connection_not_found",
                message: format!("Connection '{id}' was not found"),
            },
            ApplicationError::ConnectionNotActive(id) => Self {
                code: "connection_not_active",
                message: format!("Connection '{id}' is not connected"),
            },
            ApplicationError::Assistant(_) | ApplicationError::Operation(_) => Self {
                code: "operation_error",
                message: "The requested operation could not be completed".to_string(),
            },
            ApplicationError::Github(message) => Self {
                code: "github_error",
                message,
            },
            // PostgreSQL's Display implementation is intentionally terse (often just "db error");
            // expose the bounded, user-facing source-chain message so the editor can show the
            // actual SQL diagnostic in its alert.
            ApplicationError::Core(error) => Self {
                code: "backend_error",
                message: error.detailed_message(),
            },
        }
    }
}

#[tauri::command]
async fn health(state: State<'_, Application>) -> Result<Health, CommandError> {
    Ok(state.health())
}

#[tauri::command]
fn preferences(state: State<'_, Application>) -> Result<PreferencesView, CommandError> {
    Ok(state.preferences())
}

#[tauri::command]
fn save_preferences(
    state: State<'_, Application>,
    preferences: PreferencesView,
) -> Result<PreferencesView, CommandError> {
    state.save_preferences(preferences).map_err(Into::into)
}

#[tauri::command]
async fn check_for_updates(
    state: State<'_, Application>,
) -> Result<UpdateStatusView, CommandError> {
    state.check_for_updates().await.map_err(Into::into)
}

#[tauri::command]
async fn list_connections(
    state: State<'_, Application>,
) -> Result<Vec<ConnectionView>, CommandError> {
    Ok(state.list_connections().await)
}

#[tauri::command]
async fn save_connection(
    state: State<'_, Application>,
    request: ConnectionSecretRequest,
) -> Result<ConnectionView, CommandError> {
    let ConnectionSecretRequest {
        input,
        password,
        ssh_password,
        jump_password,
    } = request;
    state
        .save_connection_with_secrets(
            input,
            password.as_deref().filter(|value| !value.is_empty()),
            ssh_password.as_deref().filter(|value| !value.is_empty()),
            jump_password.as_deref().filter(|value| !value.is_empty()),
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn test_connection(
    state: State<'_, Application>,
    request: ConnectionSecretRequest,
) -> Result<(), CommandError> {
    let ConnectionSecretRequest {
        input,
        password,
        ssh_password,
        jump_password,
    } = request;
    state
        .test_connection(
            &input,
            password.as_deref().filter(|value| !value.is_empty()),
            ssh_password.as_deref().filter(|value| !value.is_empty()),
            jump_password.as_deref().filter(|value| !value.is_empty()),
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn delete_connection(state: State<'_, Application>, id: String) -> Result<(), CommandError> {
    state.delete_connection(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn connect_stored(
    state: State<'_, Application>,
    id: String,
    statement_timeout_ms: Option<u32>,
) -> Result<ConnectionView, CommandError> {
    state
        .connect_stored(&id, statement_timeout_ms.unwrap_or(30_000))
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn disconnect(state: State<'_, Application>, id: String) -> Result<(), CommandError> {
    state.disconnect(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn execute_query(
    state: State<'_, Application>,
    id: String,
    sql: String,
    operation_id: Option<String>,
) -> Result<QueryResult, CommandError> {
    match operation_id.as_deref() {
        Some(operation_id) => {
            state
                .execute_query_with_operation(&id, operation_id, &sql)
                .await
        }
        None => state.execute_query(&id, &sql).await,
    }
    .map_err(Into::into)
}

#[tauri::command]
async fn execute_script(
    state: State<'_, Application>,
    id: String,
    sql: String,
    operation_id: Option<String>,
) -> Result<QueryResult, CommandError> {
    match operation_id.as_deref() {
        Some(operation_id) => {
            state
                .execute_script_with_operation(&id, operation_id, &sql)
                .await
        }
        None => state.execute_script(&id, &sql).await,
    }
    .map_err(Into::into)
}

#[tauri::command]
async fn execute_explain(
    state: State<'_, Application>,
    id: String,
    sql: String,
    operation_id: Option<String>,
) -> Result<QueryResult, CommandError> {
    match operation_id.as_deref() {
        Some(operation_id) => {
            state
                .execute_explain_with_operation(&id, operation_id, &sql)
                .await
        }
        None => state.execute_explain(&id, &sql).await,
    }
    .map_err(Into::into)
}

#[tauri::command]
async fn cancel_query(
    state: State<'_, Application>,
    id: String,
    operation_id: Option<String>,
) -> Result<(), CommandError> {
    match operation_id.as_deref() {
        Some(operation_id) => state.cancel_query_operation(operation_id).await,
        None => state.cancel_query(&id).await,
    }
    .map_err(Into::into)
}

#[tauri::command]
fn list_history(state: State<'_, Application>) -> Result<Vec<HistoryView>, CommandError> {
    Ok(state.list_history())
}

#[tauri::command]
fn delete_history_entry(state: State<'_, Application>, id: String) -> Result<(), CommandError> {
    state.delete_history_entry(&id).map_err(Into::into)
}

#[tauri::command]
fn clear_history(state: State<'_, Application>) -> Result<(), CommandError> {
    state.clear_history().map_err(Into::into)
}

#[tauri::command]
fn list_snippets(state: State<'_, Application>) -> Result<Vec<SnippetView>, CommandError> {
    Ok(state.list_snippets())
}

#[tauri::command]
fn save_snippet(
    state: State<'_, Application>,
    input: SnippetInput,
) -> Result<SnippetView, CommandError> {
    state.save_snippet(input).map_err(Into::into)
}

#[tauri::command]
fn delete_snippet(state: State<'_, Application>, id: String) -> Result<(), CommandError> {
    state.delete_snippet(&id).map_err(Into::into)
}

#[tauri::command]
fn rename_snippet(
    state: State<'_, Application>,
    id: String,
    name: String,
) -> Result<(), CommandError> {
    state.rename_snippet(&id, &name).map_err(Into::into)
}

#[tauri::command]
async fn list_schemas(
    state: State<'_, Application>,
    id: String,
) -> Result<Vec<SchemaView>, CommandError> {
    state.list_schemas(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn list_tables(
    state: State<'_, Application>,
    id: String,
    schema: String,
) -> Result<Vec<TableView>, CommandError> {
    state.list_tables(&id, &schema).await.map_err(Into::into)
}

#[tauri::command]
async fn list_schema_objects(
    state: State<'_, Application>,
    id: String,
    schema: String,
) -> Result<Vec<SchemaObjectView>, CommandError> {
    state
        .list_schema_objects(&id, &schema)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn next_sequence_value(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
) -> Result<String, CommandError> {
    state
        .next_sequence_value(&id, &schema, &name)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn set_sequence_value(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
    value: String,
) -> Result<(), CommandError> {
    state
        .set_sequence_value(&id, &schema, &name, &value)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn completion_data(
    state: State<'_, Application>,
    id: String,
) -> Result<CompletionDataView, CommandError> {
    state.completion_data(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn global_search(
    state: State<'_, Application>,
    id: String,
    term: String,
) -> Result<Vec<SearchResultView>, CommandError> {
    state.global_search(&id, &term).await.map_err(Into::into)
}

#[tauri::command]
async fn dashboard(
    state: State<'_, Application>,
    id: String,
) -> Result<DashboardView, CommandError> {
    state.dashboard(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn table_detail(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
) -> Result<TableDetailView, CommandError> {
    state
        .table_detail(&id, &schema, &table)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn browse_table_data(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    offset: u64,
    limit: u32,
) -> Result<BrowseTableView, CommandError> {
    state
        .browse_table_data(&id, &schema, &table, offset, limit)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn update_table_cell(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    input: UpdateTableCellInput,
) -> Result<(), CommandError> {
    state
        .update_table_cell(&id, &schema, &table, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn insert_table_row(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    input: InsertTableRowInput,
) -> Result<(), CommandError> {
    state
        .insert_table_row(&id, &schema, &table, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn delete_table_row(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    input: DeleteTableRowInput,
) -> Result<(), CommandError> {
    state
        .delete_table_row(&id, &schema, &table, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn create_schema(
    state: State<'_, Application>,
    id: String,
    schema: String,
) -> Result<(), CommandError> {
    state.create_schema(&id, &schema).await.map_err(Into::into)
}

#[tauri::command]
async fn create_table(
    state: State<'_, Application>,
    id: String,
    input: CreateTableInput,
) -> Result<(), CommandError> {
    state.create_table(&id, input).await.map_err(Into::into)
}

#[tauri::command]
async fn preview_alter_table(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    input: AlterTableInput,
) -> Result<AlterTablePreviewView, CommandError> {
    state
        .preview_alter_table(&id, &schema, &table, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn alter_table(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    input: AlterTableInput,
) -> Result<(), CommandError> {
    state
        .alter_table(&id, &schema, &table, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn create_sequence(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
) -> Result<(), CommandError> {
    state
        .create_sequence(&id, &schema, &name)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn save_view_definition(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
    ddl: String,
) -> Result<(), CommandError> {
    state
        .save_view_definition(&id, &schema, &name, &ddl)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn save_sequence_definition(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
    ddl: String,
) -> Result<(), CommandError> {
    state
        .save_sequence_definition(&id, &schema, &name, &ddl)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn index_definition(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    name: String,
) -> Result<String, CommandError> {
    state
        .index_definition(&id, &schema, &table, &name)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn save_index_definition(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    name: String,
    ddl: String,
) -> Result<(), CommandError> {
    state
        .save_index_definition(&id, &schema, &table, &name, &ddl)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn create_trigger(
    state: State<'_, Application>,
    id: String,
    schema: String,
    input: TriggerInput,
) -> Result<(), CommandError> {
    state
        .create_trigger(&id, &schema, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn function_definitions(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
) -> Result<Vec<FunctionDefinitionView>, CommandError> {
    state
        .function_definitions(&id, &schema, &name)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn validate_function_definition(
    state: State<'_, Application>,
    id: String,
    ddl: String,
) -> Result<Option<String>, CommandError> {
    state
        .validate_function_definition(&id, &ddl)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn save_function_definition(
    state: State<'_, Application>,
    id: String,
    ddl: String,
) -> Result<(), CommandError> {
    state
        .save_function_definition(&id, &ddl)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn save_trigger_definition(
    state: State<'_, Application>,
    id: String,
    ddl: String,
) -> Result<(), CommandError> {
    state
        .save_trigger_definition(&id, &ddl)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn run_routine(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
    is_procedure: bool,
    params: Vec<Option<String>>,
) -> Result<QueryResult, CommandError> {
    state
        .run_routine(&id, &schema, &name, is_procedure, params)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn delete_routine(
    state: State<'_, Application>,
    id: String,
    schema: String,
    name: String,
    identity_arguments: String,
    is_procedure: bool,
) -> Result<(), CommandError> {
    state
        .delete_routine(&id, &schema, &name, &identity_arguments, is_procedure)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn delete_trigger(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    name: String,
) -> Result<(), CommandError> {
    state
        .delete_trigger(&id, &schema, &table, &name)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn erd(
    state: State<'_, Application>,
    id: String,
    schema: String,
) -> Result<ErdView, CommandError> {
    state.erd(&id, &schema).await.map_err(Into::into)
}

#[tauri::command]
async fn admin(state: State<'_, Application>, id: String) -> Result<AdminView, CommandError> {
    state.admin(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn cancel_activity(
    state: State<'_, Application>,
    id: String,
    pid: i32,
) -> Result<(), CommandError> {
    state.cancel_activity(&id, pid).await.map_err(Into::into)
}

#[tauri::command]
async fn list_cron_jobs(
    state: State<'_, Application>,
    id: String,
) -> Result<CronJobsView, CommandError> {
    state.list_cron_jobs(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn set_cron_job_active(
    state: State<'_, Application>,
    id: String,
    job_id: i64,
    active: bool,
) -> Result<(), CommandError> {
    state
        .set_cron_job_active(&id, job_id, active)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn create_cron_job(
    state: State<'_, Application>,
    id: String,
    input: CronJobInput,
) -> Result<(), CommandError> {
    state.create_cron_job(&id, input).await.map_err(Into::into)
}

#[tauri::command]
async fn update_cron_job(
    state: State<'_, Application>,
    id: String,
    job_id: i64,
    input: CronJobInput,
) -> Result<(), CommandError> {
    state
        .update_cron_job(&id, job_id, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn cron_job_runs(
    state: State<'_, Application>,
    id: String,
    job_id: i64,
) -> Result<Vec<CronJobRunView>, CommandError> {
    state.cron_job_runs(&id, job_id).await.map_err(Into::into)
}

#[tauri::command]
async fn delete_cron_job(
    state: State<'_, Application>,
    id: String,
    job_id: i64,
) -> Result<(), CommandError> {
    state.delete_cron_job(&id, job_id).await.map_err(Into::into)
}

#[tauri::command]
async fn list_roles(
    state: State<'_, Application>,
    id: String,
) -> Result<Vec<RoleView>, CommandError> {
    state.list_roles(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn create_role(
    state: State<'_, Application>,
    id: String,
    input: CreateRoleInput,
) -> Result<(), CommandError> {
    state.create_role(&id, input).await.map_err(Into::into)
}

#[tauri::command]
async fn update_role(
    state: State<'_, Application>,
    id: String,
    name: String,
    input: UpdateRoleInput,
) -> Result<(), CommandError> {
    state
        .update_role(&id, &name, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn delete_role(
    state: State<'_, Application>,
    id: String,
    name: String,
) -> Result<(), CommandError> {
    state.delete_role(&id, &name).await.map_err(Into::into)
}

#[tauri::command]
async fn run_backup(
    state: State<'_, Application>,
    id: String,
    operation_id: String,
    options: BackupOptionsInput,
) -> Result<ToolResultView, CommandError> {
    state
        .run_backup(&id, &operation_id, options)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn choose_backup_output(
    state: State<'_, Application>,
    format: BackupFormat,
) -> Result<Option<String>, CommandError> {
    let selected = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new().set_title("Choose backup destination");
        dialog = match format {
            BackupFormat::Plain => dialog.add_filter("SQL backup", &["sql"]),
            BackupFormat::Custom => {
                dialog.add_filter("PostgreSQL custom backup", &["dump", "backup"])
            }
            BackupFormat::Directory => dialog,
        };
        dialog.save_file()
    })
    .await
    .map_err(|_| CommandError {
        code: "operation_error",
        message: "The native file picker could not be opened".to_string(),
    })?;
    let Some(path) = selected else {
        return Ok(None);
    };
    let path = path.to_string_lossy().into_owned();
    state
        .authorize_file_path(&path, FileAuthorizationPurpose::Backup)
        .await
        .map_err(CommandError::from)?;
    Ok(Some(path))
}

#[tauri::command]
async fn choose_restore_input(
    state: State<'_, Application>,
) -> Result<Option<String>, CommandError> {
    let selected = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Choose PostgreSQL backup")
            .add_filter(
                "PostgreSQL backup",
                &["dump", "backup", "sql", "txt", "tar"],
            )
            .pick_file()
    })
    .await
    .map_err(|_| CommandError {
        code: "operation_error",
        message: "The native file picker could not be opened".to_string(),
    })?;
    let Some(path) = selected else {
        return Ok(None);
    };
    let path = path.to_string_lossy().into_owned();
    state
        .authorize_file_path(&path, FileAuthorizationPurpose::Restore)
        .await
        .map_err(CommandError::from)?;
    Ok(Some(path))
}

#[tauri::command]
async fn list_extensions(
    state: State<'_, Application>,
    id: String,
) -> Result<ExtensionsView, CommandError> {
    state.list_extensions(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn install_extension(
    state: State<'_, Application>,
    id: String,
    name: String,
) -> Result<(), CommandError> {
    state
        .install_extension(&id, &name)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn drop_extension(
    state: State<'_, Application>,
    id: String,
    name: String,
) -> Result<(), CommandError> {
    state.drop_extension(&id, &name).await.map_err(Into::into)
}

#[tauri::command]
async fn query_stats(
    state: State<'_, Application>,
    id: String,
) -> Result<QueryStatsView, CommandError> {
    state.query_stats(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn reset_query_stats(state: State<'_, Application>, id: String) -> Result<(), CommandError> {
    state.reset_query_stats(&id).await.map_err(Into::into)
}

#[tauri::command]
async fn run_table_maintenance(
    state: State<'_, Application>,
    id: String,
    schema: String,
    table: String,
    operation: TableMaintenanceOperation,
) -> Result<(), CommandError> {
    state
        .run_table_maintenance(&id, &schema, &table, operation)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn run_restore(
    state: State<'_, Application>,
    id: String,
    operation_id: String,
    options: RestoreOptionsInput,
) -> Result<ToolResultView, CommandError> {
    state
        .run_restore(&id, &operation_id, options)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn cancel_operation(
    state: State<'_, Application>,
    operation_id: String,
) -> Result<(), CommandError> {
    state
        .cancel_operation(&operation_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn github_status(state: State<'_, Application>) -> Result<GithubConnection, CommandError> {
    state.github_status().await.map_err(Into::into)
}

#[tauri::command]
async fn connect_github(
    state: State<'_, Application>,
    settings: GithubSettings,
    token: String,
) -> Result<GithubConnection, CommandError> {
    state
        .connect_github(settings, &token)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn connect_github_token(
    state: State<'_, Application>,
    token: String,
) -> Result<GithubConnection, CommandError> {
    state
        .connect_github_token(&token)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn disconnect_github(state: State<'_, Application>) -> Result<(), CommandError> {
    state.disconnect_github().await.map_err(Into::into)
}

#[tauri::command]
async fn choose_programming_workspace() -> Option<String> {
    tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Choose Programming workspace")
            .pick_folder()
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
fn save_programming_file(
    workspace: String,
    relative_path: String,
    content: String,
) -> Result<(), CommandError> {
    let relative = Path::new(&relative_path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
        || relative.extension().and_then(|extension| extension.to_str()) != Some("sql")
    {
        return Err(CommandError {
            code: "invalid_input",
            message: "Programming files must be relative .sql paths inside the selected workspace.".into(),
        });
    }
    let workspace = PathBuf::from(workspace);
    if !workspace.is_absolute() {
        return Err(CommandError { code: "invalid_input", message: "Choose a valid local workspace folder.".into() });
    }
    let destination = workspace.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| CommandError { code: "filesystem_error", message: error.to_string() })?;
    }
    fs::write(destination, content).map_err(|error| CommandError { code: "filesystem_error", message: error.to_string() })
}

#[tauri::command]
async fn list_programming_files(workspace: String) -> Result<Vec<String>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || list_programming_files_sync(&workspace))
        .await
        .map_err(|error| CommandError { code: "filesystem_error", message: error.to_string() })?
}

fn list_programming_files_sync(workspace: &str) -> Result<Vec<String>, CommandError> {
    fn walk(root: &Path, current: &Path, files: &mut Vec<String>) -> io::Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, files)?;
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("sql") {
                if let Ok(relative) = path.strip_prefix(root) {
                    files.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        Ok(())
    }
    let root = PathBuf::from(workspace);
    if !root.is_absolute() || !root.is_dir() {
        return Err(CommandError { code: "invalid_input", message: "Choose a valid local workspace folder.".into() });
    }
    let mut files = Vec::new();
    walk(&root, &root, &mut files).map_err(|error| CommandError { code: "filesystem_error", message: error.to_string() })?;
    files.sort();
    Ok(files)
}

#[tauri::command]
fn read_programming_file(workspace: String, relative_path: String) -> Result<String, CommandError> {
    let relative = Path::new(&relative_path);
    if relative.is_absolute() || relative.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(CommandError { code: "invalid_input", message: "Invalid programming file path.".into() });
    }
    fs::read_to_string(PathBuf::from(workspace).join(relative)).map_err(|error| CommandError { code: "filesystem_error", message: error.to_string() })
}

#[tauri::command]
async fn github_branches(state: State<'_, Application>) -> Result<Vec<GithubBranch>, CommandError> {
    state.github_branches().await.map_err(Into::into)
}

#[tauri::command]
async fn github_repositories(
    state: State<'_, Application>,
) -> Result<Vec<GithubRepository>, CommandError> {
    state.github_repositories().await.map_err(Into::into)
}

#[tauri::command]
fn select_github_repository(
    state: State<'_, Application>,
    owner: String,
    repository: String,
) -> Result<GithubConnection, CommandError> {
    state
        .select_github_repository(&owner, &repository)
        .map_err(Into::into)
}

#[tauri::command]
async fn github_file(
    state: State<'_, Application>,
    branch: String,
    path: String,
) -> Result<Option<String>, CommandError> {
    state.github_file(&branch, &path).await.map_err(Into::into)
}

#[tauri::command]
async fn github_commit_file(
    state: State<'_, Application>,
    branch: String,
    path: String,
    content: String,
    message: String,
) -> Result<String, CommandError> {
    state
        .github_commit_file(&branch, &path, &content, &message)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn github_compare(
    state: State<'_, Application>,
    base: String,
    head: String,
) -> Result<String, CommandError> {
    state.github_compare(&base, &head).await.map_err(Into::into)
}

#[tauri::command]
async fn github_create_pull_request(
    state: State<'_, Application>,
    title: String,
    body: String,
    head: String,
    base: String,
) -> Result<GithubPullRequest, CommandError> {
    state
        .github_create_pull_request(&title, &body, &head, &base)
        .await
        .map_err(Into::into)
}

#[tauri::command]
fn assistant_settings(state: State<'_, Application>) -> Result<Settings, CommandError> {
    Ok(state.assistant_settings())
}

#[tauri::command]
fn save_assistant_settings(
    state: State<'_, Application>,
    settings: Settings,
) -> Result<Settings, CommandError> {
    state.save_assistant_settings(settings).map_err(Into::into)
}

#[tauri::command]
async fn assistant_models(
    state: State<'_, Application>,
    provider: Provider,
) -> Result<Vec<String>, CommandError> {
    state.assistant_models(provider).await.map_err(Into::into)
}

#[tauri::command]
async fn save_assistant_key(
    state: State<'_, Application>,
    provider: Provider,
    key: String,
) -> Result<(), CommandError> {
    state
        .save_assistant_key(provider, &key)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn clear_assistant_key(
    state: State<'_, Application>,
    provider: Provider,
) -> Result<(), CommandError> {
    state
        .clear_assistant_key(provider)
        .await
        .map_err(Into::into)
}

#[tauri::command]
fn assistant_history(
    state: State<'_, Application>,
    id: String,
) -> Result<Vec<AiMessage>, CommandError> {
    Ok(state.assistant_history(&id))
}

#[tauri::command]
fn clear_assistant_history(state: State<'_, Application>, id: String) -> Result<(), CommandError> {
    state.clear_assistant_history(&id).map_err(Into::into)
}

#[tauri::command]
async fn assistant_send(
    state: State<'_, Application>,
    id: String,
    message: String,
) -> Result<AssistantReplyView, CommandError> {
    state
        .assistant_send(&id, &message)
        .await
        .map_err(Into::into)
}

fn builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .manage(Application::new())
        .invoke_handler(tauri::generate_handler![
            health,
            preferences,
            save_preferences,
            check_for_updates,
            list_connections,
            save_connection,
            test_connection,
            delete_connection,
            connect_stored,
            disconnect,
            execute_query,
            execute_script,
            execute_explain,
            cancel_query,
            list_history,
            delete_history_entry,
            clear_history,
            list_snippets,
            save_snippet,
            delete_snippet,
            rename_snippet,
            list_schemas,
            list_tables,
            list_schema_objects,
            next_sequence_value,
            set_sequence_value,
            completion_data,
            global_search,
            dashboard,
            table_detail,
            browse_table_data,
            update_table_cell,
            insert_table_row,
            delete_table_row,
            create_schema,
            create_table,
            preview_alter_table,
            alter_table,
            create_sequence,
            save_view_definition,
            save_sequence_definition,
            index_definition,
            save_index_definition,
            create_trigger,
            function_definitions,
            validate_function_definition,
            save_function_definition,
            save_trigger_definition,
            run_routine,
            delete_routine,
            delete_trigger,
            erd,
            admin,
            cancel_activity,
            list_cron_jobs,
            create_cron_job,
            update_cron_job,
            cron_job_runs,
            set_cron_job_active,
            delete_cron_job,
            list_extensions,
            install_extension,
            drop_extension,
            query_stats,
            reset_query_stats,
            run_table_maintenance,
            list_roles,
            create_role,
            update_role,
            delete_role,
            run_backup,
            choose_backup_output,
            choose_restore_input,
            run_restore,
            cancel_operation,
            github_status,
            connect_github,
            connect_github_token,
            disconnect_github,
            choose_programming_workspace,
            save_programming_file,
            list_programming_files,
            read_programming_file,
            github_repositories,
            select_github_repository,
            github_branches,
            github_file,
            github_commit_file,
            github_compare,
            github_create_pull_request,
            assistant_settings,
            save_assistant_settings,
            assistant_models,
            save_assistant_key,
            clear_assistant_key,
            assistant_history,
            clear_assistant_history,
            assistant_send
        ])
}

fn main() {
    builder()
        .run(tauri::generate_context!())
        .expect("error while running Draco Tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Manager;

    #[test]
    fn tauri_builder_registers_the_application_and_health_command() {
        let app = tauri::test::mock_builder()
            .manage(Application::new())
            .invoke_handler(tauri::generate_handler![health])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("Tauri builder should initialize");

        assert!(app.state::<Application>().health().ready);
    }

    #[test]
    fn backend_errors_are_not_serialized_to_ipc() {
        let error = CommandError::from(ApplicationError::Core(draco_core_error_for_test()));
        let json = serde_json::to_string(&error).expect("command error serializes");
        assert!(!json.contains("password"));
        assert!(!json.contains("postgres"));
    }

    #[test]
    fn capability_is_scoped_to_the_main_window_without_plugin_permissions() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json"))
                .expect("capability is valid JSON");
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert_eq!(
            capability["permissions"],
            serde_json::json!([
                "core:window:allow-close",
                "core:window:allow-maximize",
                "core:window:allow-minimize",
                "core:window:allow-start-dragging",
                "core:window:allow-toggle-maximize"
            ])
        );
    }

    #[test]
    fn csp_has_no_remote_resources_or_eval() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("Tauri config is valid JSON");
        let csp = config["app"]["security"]["csp"]
            .as_str()
            .expect("CSP is configured");
        assert!(!csp.contains("unsafe-eval"));
        assert!(!csp.contains("unsafe-inline"));
        assert!(!csp.contains("https://"));
        assert!(!csp.contains("*"));
        assert!(csp.contains("form-action 'none'"));
    }

    fn draco_core_error_for_test() -> draco_core::error::CoreError {
        draco_core::error::CoreError::Other("password=never-send-this".to_string())
    }
}
