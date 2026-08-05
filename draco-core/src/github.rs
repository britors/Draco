//! Native GitHub repository integration for database definitions.

use std::time::Duration;

use base64::Engine;
use keyring::Entry;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::store::GithubSettings;

const SERVICE: &str = "draco-github";
const ACCOUNT: &str = "github.com";
const API_VERSION: &str = "2022-11-28";

#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Core(#[from] crate::error::CoreError),
    #[error(transparent)]
    Secret(#[from] keyring::Error),
    #[error("secret store task failed: {0}")]
    SecretTask(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, GithubError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubConnection {
    pub owner: String,
    pub repository: String,
    pub root_path: String,
    pub default_branch: String,
    pub connected: bool,
    pub account_login: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubBranch {
    pub name: String,
    pub protected: bool,
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPullRequest {
    pub number: u64,
    pub html_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct BranchResponse {
    name: String,
    protected: bool,
    commit: CommitReference,
}

#[derive(Debug, Deserialize)]
struct CommitReference {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct ContentResponse {
    sha: String,
}

fn client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| GithubError::Message("Could not initialize GitHub integration.".into()))
}

fn validate_settings(settings: &GithubSettings) -> Result<()> {
    let valid_name = |value: &str| {
        !value.trim().is_empty()
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    };
    if !valid_name(&settings.owner) || !valid_name(&settings.repository) {
        return Err(GithubError::Message(
            "Enter a valid GitHub owner and repository name.".into(),
        ));
    }
    if settings.root_path.split('/').any(|part| part == "..") {
        return Err(GithubError::Message(
            "GitHub root path cannot contain '..'.".into(),
        ));
    }
    Ok(())
}

fn api_url(segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse("https://api.github.com")
        .map_err(|_| GithubError::Message("Invalid GitHub API URL.".into()))?;
    url.path_segments_mut()
        .map_err(|_| GithubError::Message("Invalid GitHub API URL.".into()))?
        .extend(segments);
    Ok(url)
}

fn repo_url(settings: &GithubSettings, suffix: &[&str]) -> Result<Url> {
    validate_settings(settings)?;
    let mut segments = vec![
        "repos",
        settings.owner.as_str(),
        settings.repository.as_str(),
    ];
    segments.extend_from_slice(suffix);
    api_url(&segments)
}

fn request(
    client: &Client,
    method: reqwest::Method,
    url: Url,
    token: &str,
) -> reqwest::RequestBuilder {
    client
        .request(method, url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", format!("Draco/{}", crate::VERSION))
}

async fn checked(
    response: reqwest::Response,
    missing_ok: bool,
) -> Result<Option<reqwest::Response>> {
    let status = response.status();
    if status.is_success() {
        return Ok(Some(response));
    }
    if missing_ok && status == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let message = match status {
        StatusCode::UNAUTHORIZED => {
            "GitHub rejected the token. Check the credential and try again."
        }
        StatusCode::FORBIDDEN => "The GitHub token does not have permission for this operation.",
        StatusCode::NOT_FOUND => "The GitHub repository or resource was not found.",
        StatusCode::UNPROCESSABLE_ENTITY => {
            "GitHub rejected the request. Check branches, file path and pull request fields."
        }
        _ => "GitHub could not complete the request.",
    };
    Err(GithubError::Message(format!(
        "{message} (HTTP {})",
        status.as_u16()
    )))
}

pub async fn load_token() -> Result<String> {
    tokio::task::spawn_blocking(|| match Entry::new(SERVICE, ACCOUNT)?.get_password() {
        Ok(token) => Ok(token),
        Err(keyring::Error::NoEntry) => Err(GithubError::Message(
            "Connect GitHub in Preferences before using repository features.".into(),
        )),
        Err(error) => Err(GithubError::Secret(error)),
    })
    .await?
}

pub async fn save_token(token: &str) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(GithubError::Message("GitHub token is required.".into()));
    }
    let token = token.to_owned();
    tokio::task::spawn_blocking(move || {
        Entry::new(SERVICE, ACCOUNT)?.set_password(&token)?;
        Ok(())
    })
    .await?
}

pub async fn clear_token() -> Result<()> {
    tokio::task::spawn_blocking(|| {
        let _ = Entry::new(SERVICE, ACCOUNT).and_then(|entry| entry.delete_credential());
        Ok(())
    })
    .await?
}

async fn authenticated_user(client: &Client, token: &str) -> Result<String> {
    let response = request(client, reqwest::Method::GET, api_url(&["user"])?, token)
        .send()
        .await
        .map_err(|_| GithubError::Message("Could not reach GitHub.".into()))?;
    let response = checked(response, false).await?.expect("checked response");
    response
        .json::<GithubUser>()
        .await
        .map(|user| user.login)
        .map_err(|_| GithubError::Message("GitHub returned an invalid account response.".into()))
}

pub async fn connect(settings: &GithubSettings, token: &str) -> Result<GithubConnection> {
    validate_settings(settings)?;
    let client = client()?;
    let login = authenticated_user(&client, token).await?;
    let repo_response = request(
        &client,
        reqwest::Method::GET,
        repo_url(settings, &[])?,
        token,
    )
    .send()
    .await
    .map_err(|_| GithubError::Message("Could not reach GitHub.".into()))?;
    checked(repo_response, false).await?;
    crate::store::save_github_settings(settings)?;
    save_token(token).await?;
    Ok(connection_view(settings.clone(), true, Some(login)))
}

pub async fn status() -> Result<GithubConnection> {
    let settings = crate::store::get_github_settings();
    if settings.owner.is_empty() || settings.repository.is_empty() {
        return Ok(connection_view(settings, false, None));
    }
    let token = match load_token().await {
        Ok(token) => token,
        Err(_) => return Ok(connection_view(settings, false, None)),
    };
    let login = authenticated_user(&client()?, &token).await?;
    Ok(connection_view(settings, true, Some(login)))
}

fn connection_view(
    settings: GithubSettings,
    connected: bool,
    account_login: Option<String>,
) -> GithubConnection {
    GithubConnection {
        owner: settings.owner,
        repository: settings.repository,
        root_path: settings.root_path,
        default_branch: settings.default_branch,
        connected,
        account_login,
    }
}

pub async fn branches() -> Result<Vec<GithubBranch>> {
    let settings = crate::store::get_github_settings();
    let token = load_token().await?;
    let client = client()?;
    let mut url = repo_url(&settings, &["branches"])?;
    url.query_pairs_mut().append_pair("per_page", "100");
    let response = request(&client, reqwest::Method::GET, url, &token)
        .send()
        .await
        .map_err(|_| GithubError::Message("Could not reach GitHub.".into()))?;
    let response = checked(response, false).await?.expect("checked response");
    let mut branches: Vec<GithubBranch> = response
        .json::<Vec<BranchResponse>>()
        .await
        .map_err(|_| GithubError::Message("GitHub returned an invalid branch list.".into()))?
        .into_iter()
        .map(|branch| GithubBranch {
            name: branch.name,
            protected: branch.protected,
            sha: branch.commit.sha,
        })
        .collect();
    branches.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(branches)
}

fn normalized_path(settings: &GithubSettings, path: &str) -> Result<String> {
    let parts: Vec<&str> = settings
        .root_path
        .split('/')
        .chain(path.split('/'))
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() || parts.contains(&"..") {
        return Err(GithubError::Message("Invalid repository file path.".into()));
    }
    Ok(parts.join("/"))
}

async fn content_response(
    settings: &GithubSettings,
    branch: &str,
    path: &str,
    raw: bool,
) -> Result<Option<reqwest::Response>> {
    let token = load_token().await?;
    let client = client()?;
    let full_path = normalized_path(settings, path)?;
    let path_parts: Vec<&str> = full_path.split('/').collect();
    let mut suffix = vec!["contents"];
    suffix.extend(path_parts);
    let mut url = repo_url(settings, &suffix)?;
    url.query_pairs_mut().append_pair("ref", branch);
    let mut request = request(&client, reqwest::Method::GET, url, &token);
    if raw {
        request = request.header("Accept", "application/vnd.github.raw+json");
    }
    let response = request
        .send()
        .await
        .map_err(|_| GithubError::Message("Could not reach GitHub.".into()))?;
    checked(response, true).await
}

pub async fn file(branch: &str, path: &str) -> Result<Option<String>> {
    let settings = crate::store::get_github_settings();
    let response = content_response(&settings, branch, path, true).await?;
    match response {
        Some(response) => response
            .text()
            .await
            .map(Some)
            .map_err(|_| GithubError::Message("Could not read the GitHub file.".into())),
        None => Ok(None),
    }
}

pub async fn commit_file(branch: &str, path: &str, content: &str, message: &str) -> Result<String> {
    if branch.trim().is_empty() || message.trim().is_empty() {
        return Err(GithubError::Message(
            "Branch and commit message are required.".into(),
        ));
    }
    let settings = crate::store::get_github_settings();
    let token = load_token().await?;
    let existing_sha = match content_response(&settings, branch, path, false).await? {
        Some(response) => Some(
            response
                .json::<ContentResponse>()
                .await
                .map_err(|_| {
                    GithubError::Message("Could not read the existing GitHub file.".into())
                })?
                .sha,
        ),
        None => None,
    };
    let full_path = normalized_path(&settings, path)?;
    let path_parts: Vec<&str> = full_path.split('/').collect();
    let mut suffix = vec!["contents"];
    suffix.extend(path_parts);
    let mut body = json!({
        "message": message.trim(),
        "content": base64::engine::general_purpose::STANDARD.encode(content),
        "branch": branch,
    });
    if let Some(sha) = existing_sha {
        body["sha"] = json!(sha);
    }
    let client = client()?;
    let response = request(
        &client,
        reqwest::Method::PUT,
        repo_url(&settings, &suffix)?,
        &token,
    )
    .json(&body)
    .send()
    .await
    .map_err(|_| GithubError::Message("Could not reach GitHub.".into()))?;
    let response = checked(response, false).await?.expect("checked response");
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|_| GithubError::Message("GitHub returned an invalid commit response.".into()))?;
    payload["commit"]["html_url"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| GithubError::Message("GitHub did not return the commit URL.".into()))
}

pub async fn compare(base: &str, head: &str) -> Result<String> {
    if base.trim().is_empty() || head.trim().is_empty() {
        return Err(GithubError::Message(
            "Choose both branches to compare.".into(),
        ));
    }
    let settings = crate::store::get_github_settings();
    let token = load_token().await?;
    let comparison = format!("{base}...{head}");
    let client = client()?;
    let response = request(
        &client,
        reqwest::Method::GET,
        repo_url(&settings, &["compare", &comparison])?,
        &token,
    )
    .header("Accept", "application/vnd.github.diff")
    .send()
    .await
    .map_err(|_| GithubError::Message("Could not reach GitHub.".into()))?;
    checked(response, false)
        .await?
        .expect("checked response")
        .text()
        .await
        .map_err(|_| GithubError::Message("Could not read the GitHub comparison.".into()))
}

pub async fn create_pull_request(
    title: &str,
    body: &str,
    head: &str,
    base: &str,
) -> Result<GithubPullRequest> {
    if title.trim().is_empty() || head.trim().is_empty() || base.trim().is_empty() {
        return Err(GithubError::Message(
            "Pull request title, head and base branches are required.".into(),
        ));
    }
    let settings = crate::store::get_github_settings();
    let token = load_token().await?;
    let client = client()?;
    let response = request(
        &client,
        reqwest::Method::POST,
        repo_url(&settings, &["pulls"])?,
        &token,
    )
    .json(&json!({"title": title.trim(), "body": body.trim(), "head": head, "base": base}))
    .send()
    .await
    .map_err(|_| GithubError::Message("Could not reach GitHub.".into()))?;
    checked(response, false)
        .await?
        .expect("checked response")
        .json::<GithubPullRequest>()
        .await
        .map_err(|_| {
            GithubError::Message("GitHub returned an invalid pull request response.".into())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_names_and_paths_are_validated() {
        let settings = GithubSettings {
            owner: "acme".into(),
            repository: "database-code".into(),
            root_path: "database/definitions".into(),
            default_branch: "main".into(),
        };
        assert!(validate_settings(&settings).is_ok());
        assert_eq!(
            normalized_path(&settings, "public/functions/run.sql").unwrap(),
            "database/definitions/public/functions/run.sql"
        );

        let mut invalid = settings;
        invalid.root_path = "../private".into();
        assert!(validate_settings(&invalid).is_err());
    }
}
