//! Read-only release discovery against Draco's public GitHub repository.

use serde::Deserialize;

use crate::error::{CoreError, Result};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/britors/Draco/releases/latest";

#[derive(Debug, Clone, Deserialize)]
pub struct LatestRelease {
    pub tag_name: String,
    pub html_url: String,
}

/// True inside a Flatpak sandbox, where the store (e.g. Flathub) delivers updates and the app
/// must not point users to GitHub downloads. Flatpak sets `FLATPAK_ID` and mounts `/.flatpak-info`.
pub fn updates_managed_by_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists()
}

pub async fn latest_release() -> Result<LatestRelease> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| CoreError::Other(error.to_string()))?
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", format!("Draco/{}", crate::VERSION))
        .send()
        .await
        .map_err(|error| CoreError::Other(error.to_string()))?
        .error_for_status()
        .map_err(|error| CoreError::Other(error.to_string()))?
        .json()
        .await
        .map_err(|error| CoreError::Other(error.to_string()))
}
