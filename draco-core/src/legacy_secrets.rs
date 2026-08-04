//! One-time compatibility bridge for credentials written by the former `oo7` backend.
//!
//! `keyring` uses the standard `service` + `username` attributes, while the old Draco build used
//! custom `connection`/`kind` and `provider` attributes. On Linux we copy a legacy value into the
//! new entry, verify the copy, and only then remove the old Secret Service item. Secret bytes and
//! backend diagnostics never leave this module.

use keyring::Entry;

#[cfg(target_os = "linux")]
fn legacy_secret(attributes: &[(&str, &str)]) -> Result<Option<String>, ()> {
    use secret_service::{blocking::SecretService, EncryptionType};
    use std::collections::HashMap;

    let service = SecretService::connect(EncryptionType::Dh).map_err(|_| ())?;
    let items = service
        .search_items(attributes.iter().copied().collect::<HashMap<_, _>>())
        .map_err(|_| ())?;
    let item = if let Some(item) = items.unlocked.first() {
        item
    } else if let Some(item) = items.locked.first() {
        item.unlock().map_err(|_| ())?;
        item
    } else {
        return Ok(None);
    };
    let secret = item.get_secret().map_err(|_| ())?;
    String::from_utf8(secret).map(Some).map_err(|_| ())
}

#[cfg(not(target_os = "linux"))]
fn legacy_secret(_attributes: &[(&str, &str)]) -> Result<Option<String>, ()> {
    Ok(None)
}

#[cfg(target_os = "linux")]
pub(crate) fn remove(attributes: &[(&str, &str)]) {
    use secret_service::{blocking::SecretService, EncryptionType};
    use std::collections::HashMap;

    let Ok(service) = SecretService::connect(EncryptionType::Dh) else {
        return;
    };
    let Ok(items) = service.search_items(attributes.iter().copied().collect::<HashMap<_, _>>())
    else {
        return;
    };
    for item in &items.locked {
        let _ = item.unlock();
    }
    for item in items.unlocked.iter().chain(items.locked.iter()) {
        let _ = item.delete();
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn remove(_attributes: &[(&str, &str)]) {}

pub(crate) fn migrate(
    destination: &Entry,
    attributes: &[(&str, &str)],
) -> Result<Option<String>, &'static str> {
    let Some(secret) = legacy_secret(attributes).map_err(|_| "legacy credential lookup failed")?
    else {
        return Ok(None);
    };
    destination
        .set_password(&secret)
        .map_err(|_| "credential copy failed")?;
    let copied = destination
        .get_password()
        .map_err(|_| "credential copy verification failed")?;
    if copied != secret {
        return Err("credential copy verification failed");
    }
    remove(attributes);
    Ok(Some(secret))
}
