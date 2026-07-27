//! Connection passwords, SSH passwords and SSH jump-host passwords, stored in the system's
//! Secret Service (GNOME Keyring / KWallet) via `oo7` — never written to disk in plain text,
//! replacing the old Electron driver's `safeStorage`-encrypted JSON file.

use std::collections::HashMap;

use oo7::Keyring;

use crate::error::Result;

const SERVICE: &str = "draco";

fn attributes(id: &str, kind: &str) -> HashMap<&'static str, String> {
    HashMap::from([
        ("service", SERVICE.to_string()),
        ("connection", id.to_string()),
        ("kind", kind.to_string()),
    ])
}

async fn store(id: &str, kind: &str, password: &str) -> Result<()> {
    let keyring = Keyring::new().await?;
    let attrs = attributes(id, kind);
    keyring
        .create_item(&format!("Draco: {kind} password for {id}"), &attrs, password, true)
        .await?;
    Ok(())
}

async fn get(id: &str, kind: &str) -> Result<String> {
    let keyring = Keyring::new().await?;
    let attrs = attributes(id, kind);
    let items = keyring.search_items(&attrs).await?;
    let Some(item) = items.first() else { return Ok(String::new()) };
    let secret = item.secret().await?;
    Ok(String::from_utf8_lossy(secret.as_bytes()).to_string())
}

async fn remove(id: &str, kind: &str) -> Result<()> {
    let keyring = Keyring::new().await?;
    let attrs = attributes(id, kind);
    // Best-effort: deleting a password that was never stored isn't an error from the caller's
    // point of view (mirrors deleting a key that's simply absent from a plain object).
    let _ = keyring.delete(&attrs).await;
    Ok(())
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

pub async fn get_ssh_password(id: &str) -> Result<String> {
    get(id, "ssh").await
}

pub async fn store_jump_ssh_password(id: &str, password: &str) -> Result<()> {
    store(id, "jump", password).await
}

pub async fn get_jump_ssh_password(id: &str) -> Result<String> {
    get(id, "jump").await
}
