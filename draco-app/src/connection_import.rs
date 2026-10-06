//! Connection URLs and `.pgpass`/`pg_service.conf` imports.
//!
//! A pasted URL only fills the connection form: the UI still runs the mandatory test and the
//! normal save, which puts the password in the credential store. File imports keep every parsed
//! password in this process only, between the preview and the import call, and send the UI just
//! whether a password exists. Each imported connection must pass the same connection test before
//! it is saved.

use std::path::Path;
use std::time::{Duration, Instant};

use draco_core::connection_import::{self as parser, ImportedConnection, UrlError};
use draco_core::store;
use serde::{Deserialize, Serialize};

use crate::{validate_input, Application, ApplicationError, ConnectionInput, Result, Validation};

/// Larger files are not connection files and are not read.
const MAX_IMPORT_FILE_BYTES: u64 = 1024 * 1024;
const PENDING_IMPORT_TTL: Duration = Duration::from_secs(10 * 60);

/// Form fields from a pasted URL. `password` is returned only so the form can hold it in its
/// password field until the normal test-and-save; it is never stored by this call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedConnectionUrlView {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub ssl: bool,
    pub ssl_verification_downgraded: bool,
    pub password: Option<String>,
    pub ignored_parameters: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionImportSource {
    Pgpass,
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionImportSourceView {
    pub source: ConnectionImportSource,
    pub found: bool,
    pub count: usize,
}

/// One importable connection. The password itself stays in the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionImportCandidateView {
    pub key: String,
    pub source: ConnectionImportSource,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub ssl: bool,
    pub ssl_verification_downgraded: bool,
    pub has_password: bool,
    pub already_saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionImportPreviewView {
    pub sources: Vec<ConnectionImportSourceView>,
    pub candidates: Vec<ConnectionImportCandidateView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionImportStatus {
    Saved,
    Invalid,
    TestFailed,
    SaveFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionImportResultView {
    pub key: String,
    pub label: String,
    pub status: ConnectionImportStatus,
    pub connection_id: Option<String>,
}

pub(crate) struct PendingImport {
    created: Instant,
    /// `None` once imported, so a second import of the same key cannot duplicate it.
    candidates: Vec<Option<(ConnectionImportSource, ImportedConnection)>>,
}

impl Application {
    /// Parses a `postgres://`/`postgresql://` URL into form fields. Nothing is saved.
    pub fn parse_connection_url(&self, url: &str) -> Result<ParsedConnectionUrlView> {
        let parsed = parser::parse_connection_url(url)
            .map_err(|error| ApplicationError::InvalidInput(url_validation(error)))?;
        Ok(ParsedConnectionUrlView {
            label: parsed.label,
            host: parsed.host,
            port: parsed.port,
            database: parsed.database,
            user: parsed.user,
            ssl: parsed.ssl,
            ssl_verification_downgraded: parsed.ssl_verification_downgraded,
            password: parsed.password,
            ignored_parameters: parsed.ignored_parameters,
        })
    }

    /// Reads libpq's own password and service files from their standard locations
    /// (`PGPASSFILE`/`~/.pgpass`, `PGSERVICEFILE`/`~/.pg_service.conf`).
    pub async fn preview_connection_imports(&self) -> Result<ConnectionImportPreviewView> {
        let pgpass_text = parser::default_pgpass_path().and_then(|path| read_import_file(&path));
        let service_text = parser::default_service_path().and_then(|path| read_import_file(&path));
        let pgpass = pgpass_text
            .as_deref()
            .map(parser::parse_pgpass)
            .unwrap_or_default();
        let services = service_text
            .as_deref()
            .map(parser::parse_pg_service)
            .unwrap_or_default();
        let from_pgpass = parser::pgpass_connections(&pgpass);
        let from_services =
            parser::service_connections(&services, &pgpass, parser::os_user().as_deref());
        let sources = vec![
            ConnectionImportSourceView {
                source: ConnectionImportSource::Service,
                found: service_text.is_some(),
                count: from_services.len(),
            },
            ConnectionImportSourceView {
                source: ConnectionImportSource::Pgpass,
                found: pgpass_text.is_some(),
                count: from_pgpass.len(),
            },
        ];
        let candidates = from_services
            .into_iter()
            .map(|connection| (ConnectionImportSource::Service, connection))
            .chain(
                from_pgpass
                    .into_iter()
                    .map(|connection| (ConnectionImportSource::Pgpass, connection)),
            )
            .collect();
        Ok(self.replace_pending_import(sources, candidates).await)
    }

    /// Reads a file the user picked with the native dialog as a password or service file.
    /// Services without a password still look it up in the standard `.pgpass`.
    pub async fn preview_connection_import_file(
        &self,
        path: &Path,
        source: ConnectionImportSource,
    ) -> Result<ConnectionImportPreviewView> {
        let text = read_import_file(path).ok_or_else(|| {
            ApplicationError::InvalidInput(Validation::new(
                "validation.connectionImportFileUnreadable",
                "The selected file could not be read as a connection file",
            ))
        })?;
        let candidates = match source {
            ConnectionImportSource::Pgpass => {
                parser::pgpass_connections(&parser::parse_pgpass(&text))
            }
            ConnectionImportSource::Service => {
                let pgpass = parser::default_pgpass_path()
                    .and_then(|path| read_import_file(&path))
                    .map(|text| parser::parse_pgpass(&text))
                    .unwrap_or_default();
                parser::service_connections(
                    &parser::parse_pg_service(&text),
                    &pgpass,
                    parser::os_user().as_deref(),
                )
            }
        };
        let sources = vec![ConnectionImportSourceView {
            source,
            found: true,
            count: candidates.len(),
        }];
        let candidates = candidates
            .into_iter()
            .map(|connection| (source, connection))
            .collect();
        Ok(self.replace_pending_import(sources, candidates).await)
    }

    /// Tests and saves the selected candidates of the last preview. Only connections that pass
    /// the connection test are saved; their passwords go to the credential store.
    pub async fn import_connections(
        &self,
        keys: Vec<String>,
    ) -> Result<Vec<ConnectionImportResultView>> {
        let selected = {
            let pending = self.pending_import.lock().await;
            let pending = pending
                .as_ref()
                .filter(|pending| pending.created.elapsed() <= PENDING_IMPORT_TTL)
                .ok_or_else(import_expired)?;
            let mut seen = std::collections::HashSet::new();
            let mut selected = Vec::new();
            for key in keys {
                if !seen.insert(key.clone()) {
                    continue;
                }
                let index = key.parse::<usize>().map_err(|_| import_expired())?;
                let (_, connection) = pending
                    .candidates
                    .get(index)
                    .and_then(Option::as_ref)
                    .ok_or_else(import_expired)?;
                selected.push((key, index, connection.clone()));
            }
            selected
        };
        if selected.is_empty() {
            return Err(ApplicationError::InvalidInput(Validation::new(
                "validation.connectionImportNothingSelected",
                "Select at least one connection to import",
            )));
        }

        let tests = selected.iter().map(|(_, _, connection)| async move {
            let input = import_input(connection);
            if validate_input(&input).is_err() {
                return ConnectionImportStatus::Invalid;
            }
            let password = connection.password.as_deref().unwrap_or_default();
            match self
                .test_connection(&input, Some(password), None, None)
                .await
            {
                Ok(()) => ConnectionImportStatus::Saved,
                Err(_) => ConnectionImportStatus::TestFailed,
            }
        });
        let outcomes = futures_util::future::join_all(tests).await;

        let mut results = Vec::with_capacity(selected.len());
        for ((key, index, connection), outcome) in selected.into_iter().zip(outcomes) {
            let mut status = outcome;
            let mut connection_id = None;
            if status == ConnectionImportStatus::Saved {
                match self
                    .save_connection_with_secrets(
                        import_input(&connection),
                        connection.password.as_deref(),
                        None,
                        None,
                    )
                    .await
                {
                    Ok(view) => {
                        connection_id = Some(view.id);
                        if let Some(pending) = self.pending_import.lock().await.as_mut() {
                            if let Some(slot) = pending.candidates.get_mut(index) {
                                *slot = None;
                            }
                        }
                    }
                    Err(_) => status = ConnectionImportStatus::SaveFailed,
                }
            }
            results.push(ConnectionImportResultView {
                key,
                label: connection.label,
                status,
                connection_id,
            });
        }
        Ok(results)
    }

    /// Drops parsed passwords kept for an import the user abandoned.
    pub async fn discard_connection_import(&self) {
        self.pending_import.lock().await.take();
    }

    async fn replace_pending_import(
        &self,
        sources: Vec<ConnectionImportSourceView>,
        candidates: Vec<(ConnectionImportSource, ImportedConnection)>,
    ) -> ConnectionImportPreviewView {
        let saved = store::list_connections();
        let views = candidates
            .iter()
            .enumerate()
            .map(
                |(index, (source, connection))| ConnectionImportCandidateView {
                    key: index.to_string(),
                    source: *source,
                    label: connection.label.clone(),
                    host: connection.host.clone(),
                    port: connection.port,
                    database: connection.database.clone(),
                    user: connection.user.clone(),
                    ssl: connection.ssl,
                    ssl_verification_downgraded: connection.ssl_verification_downgraded,
                    has_password: connection.password.is_some(),
                    already_saved: saved.iter().any(|saved| {
                        saved.host.eq_ignore_ascii_case(&connection.host)
                            && saved.port == connection.port
                            && saved.database == connection.database
                            && saved.user == connection.user
                    }),
                },
            )
            .collect();
        *self.pending_import.lock().await = Some(PendingImport {
            created: Instant::now(),
            candidates: candidates.into_iter().map(Some).collect(),
        });
        ConnectionImportPreviewView {
            sources,
            candidates: views,
        }
    }
}

fn import_input(connection: &ImportedConnection) -> ConnectionInput {
    ConnectionInput {
        id: None,
        label: connection.label.clone(),
        host: connection.host.clone(),
        port: u32::from(connection.port),
        database: connection.database.clone(),
        user: connection.user.clone(),
        ssl: connection.ssl,
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
        environment: None,
        read_only: false,
    }
}

/// Reads a small regular file; a missing, oversized or non-UTF-8 file counts as absent.
fn read_import_file(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_IMPORT_FILE_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn import_expired() -> ApplicationError {
    ApplicationError::InvalidInput(Validation::new(
        "validation.connectionImportExpired",
        "The import list expired; read the connection files again",
    ))
}

fn url_validation(error: UrlError) -> Validation {
    match error {
        UrlError::Scheme => Validation::new(
            "validation.connectionUrlScheme",
            "The URL must start with postgres:// or postgresql://",
        ),
        UrlError::Encoding => Validation::new(
            "validation.connectionUrlEncoding",
            "The URL has an invalid percent-encoded character",
        ),
        UrlError::MultipleHosts => Validation::new(
            "validation.connectionUrlMultipleHosts",
            "URLs with several hosts are not supported",
        ),
        UrlError::UnixSocket => Validation::new(
            "validation.connectionUrlUnixSocket",
            "Unix socket connections are not supported; use a TCP host",
        ),
        UrlError::Port => Validation::new(
            "validation.connectionUrlPort",
            "The URL port must be between 1 and 65535",
        ),
        UrlError::SslMode => Validation::new(
            "validation.connectionUrlSslMode",
            "The URL has an unknown sslmode",
        ),
        UrlError::Malformed => Validation::new(
            "validation.connectionUrlMalformed",
            "The connection URL is malformed",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_errors_never_echo_the_url() {
        let app = Application::new();
        let error = app
            .parse_connection_url("postgresql://u:supersecret@h1,h2/db")
            .unwrap_err();
        assert!(!format!("{error:?}").contains("supersecret"));
        match error {
            ApplicationError::InvalidInput(validation) => {
                assert_eq!(validation.key, "validation.connectionUrlMultipleHosts")
            }
            other => panic!("unexpected error {other:?}"),
        }
    }

    #[test]
    fn parsed_url_fills_the_form_fields() {
        let app = Application::new();
        let view = app
            .parse_connection_url("postgresql://u:p%40ss@[::1]:5440/app?sslmode=verify-full")
            .unwrap();
        assert_eq!(view.host, "::1");
        assert_eq!(view.port, 5440);
        assert_eq!(view.password.as_deref(), Some("p@ss"));
        assert!(view.ssl && view.ssl_verification_downgraded);
    }

    #[tokio::test]
    async fn import_requires_a_current_preview() {
        let app = Application::new();
        let error = app.import_connections(vec!["0".into()]).await.unwrap_err();
        assert!(matches!(
            error,
            ApplicationError::InvalidInput(Validation {
                key: "validation.connectionImportExpired",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn previewed_candidates_hide_passwords() {
        let dir = std::env::temp_dir().join(format!("draco-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pgpass");
        std::fs::write(&file, "db.example.com:5432:app:alice:hunter2\n").unwrap();
        let app = Application::new();
        let preview = app
            .preview_connection_import_file(&file, ConnectionImportSource::Pgpass)
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(preview.candidates.len(), 1);
        assert!(preview.candidates[0].has_password);
        let json = serde_json::to_string(&preview).unwrap();
        assert!(!json.contains("hunter2"));
        app.discard_connection_import().await;
        assert!(app.import_connections(vec!["0".into()]).await.is_err());
    }
}
