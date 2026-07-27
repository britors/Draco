use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbConnection {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub ssh_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
    /// Jump host (`ProxyJump`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump_key_path: Option<String>,
    #[serde(default)]
    pub favorite: bool,
}

/// A connection being edited/created, before it has an id or has passed validation.
#[derive(Debug, Clone, Default)]
pub struct DbConnectionDraft {
    pub label: String,
    pub host: String,
    pub port: u32,
    pub database: String,
    pub user: String,
    pub ssl: bool,
}

impl Default for DbConnection {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            host: "localhost".to_string(),
            port: 5432,
            database: String::new(),
            user: String::new(),
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
}

/// Validates a connection draft, returning one error message per violated rule (empty when
/// the draft is valid). `port` is `u32` so that a missing/unparsed port (represented as `0`
/// by callers) is rejected the same way an out-of-range port is.
pub fn validate_connection(draft: &DbConnectionDraft) -> Vec<String> {
    let mut errors = Vec::new();
    if draft.label.trim().is_empty() {
        errors.push("Label is required".to_string());
    }
    if draft.host.trim().is_empty() {
        errors.push("Host is required".to_string());
    }
    if draft.database.trim().is_empty() {
        errors.push("Database is required".to_string());
    }
    if draft.user.trim().is_empty() {
        errors.push("User is required".to_string());
    }
    if draft.port < 1 || draft.port > 65535 {
        errors.push("Port must be between 1 and 65535".to_string());
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_draft() -> DbConnectionDraft {
        DbConnectionDraft {
            label: "Local".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            database: "mydb".to_string(),
            user: "postgres".to_string(),
            ssl: false,
        }
    }

    #[test]
    fn no_errors_for_fully_valid_connection() {
        assert!(validate_connection(&valid_draft()).is_empty());
    }

    #[test]
    fn requires_label() {
        let mut d = valid_draft();
        d.label = String::new();
        assert!(validate_connection(&d).iter().any(|e| e.contains("Label")));
    }

    #[test]
    fn treats_whitespace_only_label_as_missing() {
        let mut d = valid_draft();
        d.label = "   ".to_string();
        assert!(validate_connection(&d).iter().any(|e| e.contains("Label")));
    }

    #[test]
    fn requires_host() {
        let mut d = valid_draft();
        d.host = String::new();
        assert!(validate_connection(&d).iter().any(|e| e.contains("Host")));
    }

    #[test]
    fn requires_database() {
        let mut d = valid_draft();
        d.database = String::new();
        assert!(validate_connection(&d).iter().any(|e| e.contains("Database")));
    }

    #[test]
    fn requires_user() {
        let mut d = valid_draft();
        d.user = String::new();
        assert!(validate_connection(&d).iter().any(|e| e.contains("User")));
    }

    #[test]
    fn rejects_port_0() {
        let mut d = valid_draft();
        d.port = 0;
        assert!(validate_connection(&d).iter().any(|e| e.contains("Port")));
    }

    #[test]
    fn rejects_port_over_65535() {
        let mut d = valid_draft();
        d.port = 65536;
        assert!(validate_connection(&d).iter().any(|e| e.contains("Port")));
    }

    #[test]
    fn accepts_port_boundary_1() {
        let mut d = valid_draft();
        d.port = 1;
        assert!(!validate_connection(&d).iter().any(|e| e.contains("Port")));
    }

    #[test]
    fn accepts_port_boundary_65535() {
        let mut d = valid_draft();
        d.port = 65535;
        assert!(!validate_connection(&d).iter().any(|e| e.contains("Port")));
    }

    #[test]
    fn accumulates_multiple_errors() {
        let d = DbConnectionDraft::default();
        assert!(validate_connection(&d).len() >= 4);
    }

    #[test]
    fn default_connection_has_port_5432() {
        assert_eq!(DbConnection::default().port, 5432);
    }

    #[test]
    fn default_connection_has_localhost_host() {
        assert_eq!(DbConnection::default().host, "localhost");
    }

    #[test]
    fn default_connection_has_ssl_false() {
        assert!(!DbConnection::default().ssl);
    }

    #[test]
    fn default_connection_has_empty_label_database_user() {
        let d = DbConnection::default();
        assert_eq!(d.label, "");
        assert_eq!(d.database, "");
        assert_eq!(d.user, "");
    }
}
