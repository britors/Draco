//! Shared connection bootstrap for GTK tasks.
//!
//! All views use this path before touching a driver. Secret Service failures are deliberately
//! propagated instead of being converted into an empty password, so the UI can explain the real
//! cause of a failed connection.

use draco_core::error::{CoreError, Result};
use draco_core::manager::ConnectionManager;
use draco_core::secrets;
use draco_core::store;

pub async fn ensure_connected(manager: &mut ConnectionManager, connection_id: &str) -> Result<()> {
    if manager.get_driver(connection_id).is_some() {
        return Ok(());
    }

    let password = secrets::get_password(connection_id).await.map_err(|err| {
        CoreError::Other(format!("Secret Service error while reading the database password: {err}"))
    })?;
    let ssh_password = secrets::get_ssh_password(connection_id).await.map_err(|err| {
        CoreError::Other(format!("Secret Service error while reading the SSH password: {err}"))
    })?;
    let jump_password = secrets::get_jump_ssh_password(connection_id).await.map_err(|err| {
        CoreError::Other(format!("Secret Service error while reading the jump-host password: {err}"))
    })?;

    manager
        .connect(
            connection_id,
            &password,
            store::get_settings().query_timeout,
            (!ssh_password.is_empty()).then_some(ssh_password.as_str()),
            (!jump_password.is_empty()).then_some(jump_password.as_str()),
        )
        .await
}
