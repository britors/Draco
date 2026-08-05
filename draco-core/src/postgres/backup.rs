//! PostgreSQL backup and restore through the official command-line tools.
//!
//! The process is launched without a shell. Passwords are supplied through `PGPASSWORD`, never
//! through command-line arguments, and an already-open `PostgresDriver` keeps an in-process SSH
//! tunnel alive for the duration of the operation.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;
use tokio::sync::{mpsc, watch};

use super::pool::PostgresDriver;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpFormat {
    Plain,
    Custom,
    Directory,
}

impl DumpFormat {
    pub fn pg_value(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Custom => "custom",
            Self::Directory => "directory",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DumpOptions {
    pub output: PathBuf,
    pub format: DumpFormat,
    pub compression: Option<u8>,
    pub schemas: Vec<String>,
    pub tables: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RestoreOptions {
    pub input: PathBuf,
    pub target_database: String,
    pub clean: bool,
    pub single_transaction: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolConnection<'a> {
    pub password: &'a str,
    pub database: &'a str,
    pub user: &'a str,
    pub ssl: bool,
}

/// Event stream for backup and restore operations. Raw stdout/stderr variants remain in the
/// contract for API compatibility, but the hardened runner never emits them; only `Finished` is sent.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    Stdout(String),
    Stderr(String),
    Finished(ToolResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolResult {
    pub exit_code: Option<i32>,
    pub cancelled: bool,
}

impl ToolResult {
    pub fn succeeded(self) -> bool {
        !self.cancelled && self.exit_code == Some(0)
    }
}

pub async fn run_dump(
    driver: &PostgresDriver,
    connection: ToolConnection<'_>,
    options: DumpOptions,
    cancel: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<ToolEvent>,
) -> Result<ToolResult> {
    let mut args = vec![
        format!("--format={}", options.format.pg_value()),
        format!("--file={}", options.output.display()),
        format!("--dbname={}", connection.database),
    ];
    if let Some(level) = options.compression {
        args.push(format!("--compress={level}"));
    }
    for schema in options
        .schemas
        .into_iter()
        .filter(|value| !value.trim().is_empty())
    {
        args.push(format!("--schema={schema}"));
    }
    for table in options
        .tables
        .into_iter()
        .filter(|value| !value.trim().is_empty())
    {
        args.push(format!("--table={table}"));
    }

    run_tool(driver, "pg_dump", &args, connection, cancel, events).await
}

pub async fn run_restore(
    driver: &PostgresDriver,
    connection: ToolConnection<'_>,
    options: RestoreOptions,
    cancel: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<ToolEvent>,
) -> Result<ToolResult> {
    let is_plain_sql = options
        .input
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "sql" | "txt"))
        .unwrap_or(false);
    let program = if is_plain_sql { "psql" } else { "pg_restore" };
    let mut args = vec![format!("--dbname={}", options.target_database)];

    if is_plain_sql {
        if options.clean {
            // psql has no pg_restore-style --clean. The option is retained in the UI so the
            // same restore dialog can handle custom/directory dumps; for SQL scripts the file
            // itself controls any DROP statements.
            let _ = options.clean;
        }
        if options.single_transaction {
            args.push("--single-transaction".to_string());
        }
        args.push(format!("--file={}", options.input.display()));
    } else {
        if options.clean {
            args.push("--clean".to_string());
            args.push("--if-exists".to_string());
        }
        if options.single_transaction {
            args.push("--single-transaction".to_string());
        }
        args.push(options.input.display().to_string());
    }

    let connection = ToolConnection {
        database: &options.target_database,
        ..connection
    };
    run_tool(driver, program, &args, connection, cancel, events).await
}

async fn run_tool(
    driver: &PostgresDriver,
    program: &str,
    args: &[String],
    connection: ToolConnection<'_>,
    mut cancel: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<ToolEvent>,
) -> Result<ToolResult> {
    let target = driver.external_target();
    let mut command = Command::new(program);
    command
        .args(args)
        .env("PGHOST", target.host)
        .env("PGPORT", target.port.to_string())
        .env("PGDATABASE", connection.database)
        .env("PGUSER", connection.user)
        .env("PGPASSWORD", connection.password)
        .env(
            "PGSSLMODE",
            if connection.ssl { "require" } else { "disable" },
        )
        // Never forward tool output: psql scripts can deliberately or accidentally print SQL
        // values, and PostgreSQL errors can echo object data. The application reports only the
        // exit status/cancellation state.
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = command.spawn()?;

    let result = tokio::select! {
        status = child.wait() => {
            let status = status?;
            ToolResult { exit_code: status.code(), cancelled: false }
        }
        _ = wait_for_cancel(&mut cancel) => {
            let _ = child.kill().await;
            let status = child.wait().await?;
            ToolResult { exit_code: status.code(), cancelled: true }
        }
    };

    let _ = events.send(ToolEvent::Finished(result));
    Ok(result)
}

async fn wait_for_cancel(cancel: &mut watch::Receiver<bool>) {
    if *cancel.borrow() {
        return;
    }
    while cancel.changed().await.is_ok() {
        if *cancel.borrow() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_format_maps_to_pg_dump_values() {
        assert_eq!(DumpFormat::Plain.pg_value(), "plain");
        assert_eq!(DumpFormat::Custom.pg_value(), "custom");
        assert_eq!(DumpFormat::Directory.pg_value(), "directory");
    }

    #[test]
    fn tool_result_only_succeeds_on_zero_exit() {
        assert!(ToolResult {
            exit_code: Some(0),
            cancelled: false
        }
        .succeeded());
        assert!(!ToolResult {
            exit_code: Some(1),
            cancelled: false
        }
        .succeeded());
        assert!(!ToolResult {
            exit_code: Some(0),
            cancelled: true
        }
        .succeeded());
    }
}
