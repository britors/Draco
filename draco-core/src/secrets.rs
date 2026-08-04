//! Connection passwords, SSH passwords and SSH jump-host passwords, stored in the platform's
//! credential store via `keyring` — Secret Service (GNOME Keyring / KWallet) on Linux, Credential
//! Manager on Windows — never written to disk in plain text, replacing the old Electron driver's
//! `safeStorage`-encrypted JSON file.
//!
//! `keyring`'s `Entry` is synchronous (it talks to D-Bus or the Windows API directly), so every
//! call runs on `spawn_blocking` to avoid stalling the tokio runtime that also drives Postgres
//! and SSH connections.

use keyring::Entry;

use crate::error::{CoreError, Result};
use crate::legacy_secrets;

const SERVICE: &str = "draco";

fn entry(id: &str, kind: &str) -> Result<Entry> {
    Ok(Entry::new(SERVICE, &format!("{kind}:{id}"))?)
}

async fn store(id: &str, kind: &str, password: &str) -> Result<()> {
    let id = id.to_string();
    let kind = kind.to_string();
    let password = password.to_string();
    tokio::task::spawn_blocking(move || {
        entry(&id, &kind)?
            .set_password(&password)
            .map_err(CoreError::from)?;
        legacy_secrets::remove(&[("service", SERVICE), ("connection", &id), ("kind", &kind)]);
        Ok(())
    })
    .await?
}

async fn get(id: &str, kind: &str) -> Result<String> {
    let id = id.to_string();
    let kind = kind.to_string();
    tokio::task::spawn_blocking(move || match entry(&id, &kind)?.get_password() {
        Ok(password) => Ok(password),
        Err(keyring::Error::NoEntry) => legacy_secrets::migrate(
            &entry(&id, &kind)?,
            &[("service", SERVICE), ("connection", &id), ("kind", &kind)],
        )
        .map(|password| password.unwrap_or_default())
        .map_err(|message| CoreError::Other(message.to_string())),
        Err(err) => Err(CoreError::from(err)),
    })
    .await?
}

async fn remove(id: &str, kind: &str) -> Result<()> {
    let id = id.to_string();
    let kind = kind.to_string();
    tokio::task::spawn_blocking(move || {
        // Best-effort: deleting a password that was never stored isn't an error from the
        // caller's point of view (mirrors deleting a key that's simply absent from a plain
        // object).
        let _ =
            entry(&id, &kind).and_then(|entry| entry.delete_credential().map_err(CoreError::from));
        legacy_secrets::remove(&[("service", SERVICE), ("connection", &id), ("kind", &kind)]);
        Ok(())
    })
    .await?
}

pub async fn store_password(id: &str, password: &str) -> Result<()> {
    store(id, "password", password).await
}

pub async fn get_password(id: &str) -> Result<String> {
    get(id, "password").await
}

pub async fn remove_password(id: &str) -> Result<()> {
    remove(id, "password").await
}

pub async fn store_ssh_password(id: &str, password: &str) -> Result<()> {
    store(id, "ssh", password).await
}

pub async fn remove_ssh_password(id: &str) -> Result<()> {
    remove(id, "ssh").await
}

pub async fn get_ssh_password(id: &str) -> Result<String> {
    get(id, "ssh").await
}

pub async fn store_jump_ssh_password(id: &str, password: &str) -> Result<()> {
    store(id, "jump", password).await
}

pub async fn remove_jump_ssh_password(id: &str) -> Result<()> {
    remove(id, "jump").await
}

pub async fn get_jump_ssh_password(id: &str) -> Result<String> {
    get(id, "jump").await
}
