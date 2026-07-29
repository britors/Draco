//! Local persistence for connection metadata, query history, snippets and settings — plain TOML
//! files under the XDG config directory. Passwords never live here; see `secrets`.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::DbConnection;
use crate::error::{CoreError, Result};

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

pub fn delete_history_entry(id: &str) -> Result<()> {
    let mut file = HistoryFile { entries: list_history() };
    file.entries.retain(|e| e.id != id);
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
    pub show_row_count: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            query_timeout: 30_000,
            preview_row_limit: 100,
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

// ── AI Assistant ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    Anthropic,
    OpenAi,
    Gemini,
}

impl AiProvider {
    pub const ALL: [Self; 3] = [Self::Anthropic, Self::OpenAi, Self::Gemini];

    pub fn id(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::OpenAi => "OpenAI",
            Self::Gemini => "Gemini",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-haiku-4-5",
            Self::OpenAi => "gpt-4.1-mini",
            Self::Gemini => "gemini-2.5-flash",
        }
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or(Self::Anthropic)
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0) as u32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub provider: AiProvider,
    pub anthropic_model: String,
    pub openai_model: String,
    pub gemini_model: String,
    pub max_messages_per_day: u32,
    pub max_rounds_per_message: u32,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::Anthropic,
            anthropic_model: AiProvider::Anthropic.default_model().to_string(),
            openai_model: AiProvider::OpenAi.default_model().to_string(),
            gemini_model: AiProvider::Gemini.default_model().to_string(),
            max_messages_per_day: 200,
            max_rounds_per_message: 8,
        }
    }
}

impl AiSettings {
    pub fn model(&self) -> &str {
        match self.provider {
            AiProvider::Anthropic => &self.anthropic_model,
            AiProvider::OpenAi => &self.openai_model,
            AiProvider::Gemini => &self.gemini_model,
        }
    }

    pub fn set_model(&mut self, value: String) {
        match self.provider {
            AiProvider::Anthropic => self.anthropic_model = value,
            AiProvider::OpenAi => self.openai_model = value,
            AiProvider::Gemini => self.gemini_model = value,
        }
    }
}

pub fn get_ai_settings() -> AiSettings {
    read_toml("ai-settings.toml")
}

pub fn save_ai_settings(settings: &AiSettings) -> Result<()> {
    write_toml("ai-settings.toml", settings)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
    /// Set to the tool's name when this message is a synthesized tool result (always `role:
    /// User` on the wire, same simplification as `assistant::run_tool`'s doc comment describes)
    /// so the chat view can label it "ferramenta: x" instead of misattributing it to the user.
    /// Never read by the provider round-trip itself — display-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_label: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AiHistoryFile {
    #[serde(default)]
    conversations: std::collections::HashMap<String, Vec<AiMessage>>,
}

/// Chat history is keyed by connection id — each connection's AI Assistant tab gets its own
/// independent conversation, same granularity as the per-connection Dashboard/Admin tabs.
pub fn get_ai_history(conn_id: &str) -> Vec<AiMessage> {
    let mut file = read_toml::<AiHistoryFile>("ai-history.toml");
    file.conversations.remove(conn_id).unwrap_or_default()
}

pub fn save_ai_history(conn_id: &str, messages: &[AiMessage]) -> Result<()> {
    let mut file = read_toml::<AiHistoryFile>("ai-history.toml");
    file.conversations.insert(conn_id.to_string(), messages.to_vec());
    write_toml("ai-history.toml", &file)
}

pub fn clear_ai_history(conn_id: &str) -> Result<()> {
    let mut file = read_toml::<AiHistoryFile>("ai-history.toml");
    file.conversations.remove(conn_id);
    write_toml("ai-history.toml", &file)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AiUsage {
    day: u64,
    count: u32,
}

/// Days since the Unix epoch in UTC — a coarse "today" key. Good enough for a soft daily-message
/// cap; it isn't calendar-accurate in every timezone (the boundary can be a few hours off from
/// local midnight), which just means the cap resets a little earlier/later than midnight for the
/// user, never a correctness issue for the feature it protects (accidental runaway API spend).
fn today_index() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

/// Increments and persists the daily message counter, failing once `limit` is reached. Called
/// once per user message sent to the AI Assistant (not once per tool-calling round-trip).
pub fn consume_ai_usage(limit: u32) -> Result<u32> {
    let mut usage: AiUsage = read_toml("ai-usage.toml");
    if usage.day != today_index() {
        usage = AiUsage { day: today_index(), count: 0 };
    }
    if usage.count >= limit.max(1) {
        return Err(CoreError::Other(format!("Limite diário de {limit} mensagens do Assistente de IA atingido.")));
    }
    usage.count += 1;
    write_toml("ai-usage.toml", &usage)?;
    Ok(usage.count)
}
