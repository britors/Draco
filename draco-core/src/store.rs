//! Local persistence for connection metadata, query history, snippets and settings — plain TOML
//! files under the XDG config directory. Passwords never live here; see `secrets`.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::DbConnection;
use crate::error::Result;

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("org", "lyraos", "Draco").expect("no home directory for the current user")
}

fn config_path(file_name: &str) -> PathBuf {
    let dirs = project_dirs();
    let dir = dirs.config_dir();
    let _ = fs::create_dir_all(dir);
    dir.join(file_name)
}

fn read_toml<T: Default + serde::de::DeserializeOwned>(file_name: &str) -> T {
    match fs::read_to_string(config_path(file_name)) {
        Ok(raw) => toml::from_str(&raw).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

fn write_toml<T: Serialize>(file_name: &str, data: &T) -> Result<()> {
    let raw = toml::to_string_pretty(data)?;
    fs::write(config_path(file_name), raw)?;
    Ok(())
}

// ── Connections ──────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConnectionsFile {
    #[serde(default)]
    connections: Vec<DbConnection>,
}

pub fn list_connections() -> Vec<DbConnection> {
    read_toml::<ConnectionsFile>("connections.toml").connections
}

pub fn get_connection(id: &str) -> Option<DbConnection> {
    list_connections().into_iter().find(|c| c.id == id)
}

/// Saves `conn`, assigning a fresh id if it doesn't already have one, and returns the saved
/// connection (with its id filled in).
pub fn save_connection(mut conn: DbConnection) -> Result<DbConnection> {
    if conn.id.is_empty() {
        conn.id = Uuid::new_v4().to_string();
    }
    let mut file = ConnectionsFile { connections: list_connections() };
    match file.connections.iter_mut().find(|c| c.id == conn.id) {
        Some(existing) => *existing = conn.clone(),
        None => file.connections.push(conn.clone()),
    }
    write_toml("connections.toml", &file)?;
    Ok(conn)
}

pub fn delete_connection(id: &str) -> Result<()> {
    let mut file = ConnectionsFile { connections: list_connections() };
    file.connections.retain(|c| c.id != id);
    write_toml("connections.toml", &file)
}

// ── Query history ────────────────────────────────────────────────────────────────

const MAX_HISTORY: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub sql: String,
    pub conn_id: String,
    pub conn_label: String,
    pub timestamp: i64,
    pub duration_ms: i64,
    pub row_count: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    entries: Vec<HistoryEntry>,
}

pub fn list_history() -> Vec<HistoryEntry> {
    read_toml::<HistoryFile>("history.toml").entries
}

pub fn add_history(mut entry: HistoryEntry) -> Result<()> {
    entry.id = Uuid::new_v4().to_string();
    let mut file = HistoryFile { entries: list_history() };
    file.entries.insert(0, entry);
    file.entries.truncate(MAX_HISTORY);
    write_toml("history.toml", &file)
}

pub fn clear_history() -> Result<()> {
    write_toml("history.toml", &HistoryFile::default())
}

// ── Snippets ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub sql: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conn_label: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SnippetsFile {
    #[serde(default)]
    snippets: Vec<Snippet>,
}

pub fn list_snippets() -> Vec<Snippet> {
    read_toml::<SnippetsFile>("snippets.toml").snippets
}

pub fn save_snippet(mut snippet: Snippet) -> Result<Snippet> {
    snippet.id = Uuid::new_v4().to_string();
    snippet.created_at = chrono_now_millis();
    let mut file = SnippetsFile { snippets: list_snippets() };
    file.snippets.insert(0, snippet.clone());
    write_toml("snippets.toml", &file)?;
    Ok(snippet)
}

pub fn delete_snippet(id: &str) -> Result<()> {
    let mut file = SnippetsFile { snippets: list_snippets() };
    file.snippets.retain(|s| s.id != id);
    write_toml("snippets.toml", &file)
}

pub fn rename_snippet(id: &str, name: &str) -> Result<()> {
    let mut file = SnippetsFile { snippets: list_snippets() };
    if let Some(s) = file.snippets.iter_mut().find(|s| s.id == id) {
        s.name = name.to_string();
    }
    write_toml("snippets.toml", &file)
}

fn chrono_now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ── Settings ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub query_timeout: u32,
    pub preview_row_limit: u32,
    pub default_ssl: bool,
    pub default_port: u16,
    pub show_row_count: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            query_timeout: 30_000,
            preview_row_limit: 100,
            default_ssl: false,
            default_port: 5432,
            show_row_count: false,
        }
    }
}

pub fn get_settings() -> AppSettings {
    match fs::read_to_string(config_path("settings.toml")) {
        Ok(raw) => toml::from_str(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn patch_settings(f: impl FnOnce(&mut AppSettings)) -> Result<AppSettings> {
    let mut settings = get_settings();
    f(&mut settings);
    write_toml("settings.toml", &settings)?;
    Ok(settings)
}
