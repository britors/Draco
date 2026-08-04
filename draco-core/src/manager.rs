use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::connection::DbConnection;
use crate::error::{CoreError, Result};
use crate::postgres::pool::PostgresDriver;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

pub struct ManagedConnection {
    pub conn: DbConnection,
    pub status: ConnectionStatus,
    pub driver: Option<PostgresDriver>,
    pub error: Option<String>,
}

/// Tracks the lifecycle (and, once connected, the live `PostgresDriver`) of every registered
/// connection. Owned by the caller as `Arc<tokio::sync::Mutex<ConnectionManager>>` so it can be
/// shared between the GTK thread (which dispatches work) and the tasks spawned on the tokio
/// runtime (which actually run the async Postgres/SSH I/O) — see `draco-gtk/src/main.rs`.
#[derive(Default)]
pub struct ConnectionManager {
    map: HashMap<String, ManagedConnection>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sync_connections(&mut self, connections: Vec<DbConnection>) {
        let ids: std::collections::HashSet<&str> =
            connections.iter().map(|c| c.id.as_str()).collect();
        self.map.retain(|id, _| ids.contains(id.as_str()));

        for conn in connections {
            match self.map.get_mut(&conn.id) {
                Some(managed) => managed.conn = conn,
                None => {
                    self.map.insert(
                        conn.id.clone(),
                        ManagedConnection {
                            conn,
                            status: ConnectionStatus::Disconnected,
                            driver: None,
                            error: None,
                        },
                    );
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&ManagedConnection> {
        self.map.get(id)
    }

    pub fn get_driver(&self, id: &str) -> Option<&PostgresDriver> {
        self.map.get(id).and_then(|m| m.driver.as_ref())
    }

    /// Returns a cheap clone of the driver's pool/tunnel handle so application queries can run
    /// without holding the manager mutex for their entire duration.
    pub fn get_driver_handle(&self, id: &str) -> Option<PostgresDriver> {
        self.get_driver(id).cloned()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn connect(
        &mut self,
        id: &str,
        password: &str,
        statement_timeout_ms: u32,
        ssh_password: Option<&str>,
        jump_password: Option<&str>,
    ) -> Result<()> {
        let conn = {
            let managed = self.map.get_mut(id).ok_or(CoreError::NotRegistered)?;
            managed.status = ConnectionStatus::Connecting;
            managed.conn.clone()
        };

        let app_name = format!("draco-{id}");
        match PostgresDriver::connect(
            &conn,
            password,
            statement_timeout_ms,
            &app_name,
            ssh_password,
            jump_password,
        )
        .await
        {
            Ok(driver) => {
                let managed = self.map.get_mut(id).ok_or(CoreError::NotRegistered)?;
                managed.driver = Some(driver);
                managed.status = ConnectionStatus::Connected;
                managed.error = None;
                Ok(())
            }
            Err(err) => {
                if let Some(managed) = self.map.get_mut(id) {
                    managed.status = ConnectionStatus::Error;
                    managed.error = Some(err.to_string());
                }
                Err(err)
            }
        }
    }

    pub async fn disconnect(&mut self, id: &str) {
        let Some(managed) = self.map.get_mut(id) else { return };
        let driver = managed.driver.take();
        if let Some(driver) = driver {
            driver.disconnect().await;
        }
        managed.status = ConnectionStatus::Disconnected;
        managed.error = None;
    }

    pub fn get_statuses(&self) -> HashMap<String, ConnectionStatus> {
        self.map.iter().map(|(id, m)| (id.clone(), m.status)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conn(id: &str, label: &str) -> DbConnection {
        DbConnection {
            id: id.to_string(),
            label: label.to_string(),
            host: "localhost".to_string(),
            port: 5432,
            database: "db".to_string(),
            user: "u".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn get_statuses_empty_when_no_connections_registered() {
        let mgr = ConnectionManager::new();
        assert!(mgr.get_statuses().is_empty());
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let mgr = ConnectionManager::new();
        assert!(mgr.get("unknown").is_none());
    }

    #[test]
    fn get_driver_returns_none_for_unknown_id() {
        let mgr = ConnectionManager::new();
        assert!(mgr.get_driver("unknown").is_none());
    }

    #[test]
    fn registers_new_connections_with_disconnected_status() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("a", "a"), make_conn("b", "b")]);
        let s = mgr.get_statuses();
        assert_eq!(s["a"], ConnectionStatus::Disconnected);
        assert_eq!(s["b"], ConnectionStatus::Disconnected);
    }

    #[test]
    fn removes_connections_no_longer_in_the_list() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("a", "a"), make_conn("b", "b")]);
        mgr.sync_connections(vec![make_conn("a", "a")]);
        let s = mgr.get_statuses();
        assert_eq!(s["a"], ConnectionStatus::Disconnected);
        assert!(!s.contains_key("b"));
    }

    #[test]
    fn updates_connection_metadata_for_an_existing_id() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("a", "Old Label")]);
        mgr.sync_connections(vec![make_conn("a", "New Label")]);
        assert_eq!(mgr.get("a").unwrap().conn.label, "New Label");
    }

    #[test]
    fn preserves_status_of_existing_connections_across_sync() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("a", "a")]);
        mgr.map.get_mut("a").unwrap().status = ConnectionStatus::Error;
        mgr.sync_connections(vec![make_conn("a", "a")]);
        assert_eq!(mgr.get("a").unwrap().status, ConnectionStatus::Error);
    }

    #[test]
    fn empty_list_removes_all_connections() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("a", "a"), make_conn("b", "b")]);
        mgr.sync_connections(vec![]);
        assert!(mgr.get_statuses().is_empty());
    }

    #[test]
    fn get_returns_the_managed_connection_after_sync() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("x", "x")]);
        let mc = mgr.get("x").unwrap();
        assert_eq!(mc.conn.id, "x");
        assert_eq!(mc.status, ConnectionStatus::Disconnected);
    }

    #[test]
    fn get_driver_returns_none_for_disconnected_connections() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("a", "a")]);
        assert!(mgr.get_driver("a").is_none());
    }

    #[test]
    fn returns_correct_statuses_for_multiple_connections() {
        let mut mgr = ConnectionManager::new();
        mgr.sync_connections(vec![make_conn("a", "a"), make_conn("b", "b"), make_conn("c", "c")]);
        let s = mgr.get_statuses();
        assert_eq!(s.len(), 3);
        assert!(s.values().all(|v| *v == ConnectionStatus::Disconnected));
    }

    #[tokio::test]
    async fn disconnect_on_unknown_id_does_not_panic() {
        let mut mgr = ConnectionManager::new();
        mgr.disconnect("never-registered").await;
    }
}
