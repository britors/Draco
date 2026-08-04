use draco_app::{
    AdminView, Application, ApplicationError, AssistantReplyView, BackupOptionsInput,
    ConnectionInput, ConnectionView, DashboardView, ErdView, Health, HistoryView, QueryResult,
    RestoreOptionsInput, SchemaView, SearchResultView, SnippetInput, SnippetView,
    TableDetailView, TableView, ToolResultView,
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
            // Keep driver, filesystem and Secret Service details out of IPC responses. Detailed
            // diagnostics belong in a bounded, redacted backend diagnostic event later (#111).
            ApplicationError::Core(_) => Self {
                code: "backend_error",
                message: "The requested operation could not be completed".to_string(),
            },
        }
    }
}

#[tauri::command]
async fn health(state: State<'_, Application>) -> Result<Health, CommandError> {
    Ok(state.health())
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
        Some(operation_id) => state
            .execute_query_with_operation(&id, operation_id, &sql)
            .await,
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
        Some(operation_id) => state
            .execute_script_with_operation(&id, operation_id, &sql)
            .await,
        None => state.execute_script(&id, &sql).await,
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
            list_connections,
            save_connection,
            test_connection,
            delete_connection,
            connect_stored,
            disconnect,
            execute_query,
            execute_script,
            cancel_query,
            list_history,
            delete_history_entry,
            clear_history,
            list_snippets,
            save_snippet,
            delete_snippet,
            list_schemas,
            list_tables,
            global_search,
            dashboard,
            table_detail,
            erd,
            admin,
            run_backup,
            run_restore,
            cancel_operation,
            assistant_settings,
            save_assistant_settings,
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
    }

    fn draco_core_error_for_test() -> draco_core::error::CoreError {
        draco_core::error::CoreError::Other("password=never-send-this".to_string())
    }
}
