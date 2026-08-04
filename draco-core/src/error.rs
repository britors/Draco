#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("not connected")]
    NotConnected,
    #[error("connection not registered")]
    NotRegistered,
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[error(transparent)]
    Pool(#[from] deadpool_postgres::PoolError),
    #[error(transparent)]
    BuildPool(#[from] deadpool_postgres::BuildError),
    #[error(transparent)]
    Tls(#[from] rustls::Error),
    #[error(transparent)]
    Ssh(#[from] russh::Error),
    #[error(transparent)]
    SshKeys(#[from] russh::keys::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    Secret(#[from] keyring::Error),
    #[error("secret store task failed: {0}")]
    SecretTask(#[from] tokio::task::JoinError),
    #[error("{0}")]
    Other(String),
}

impl From<String> for CoreError {
    fn from(s: String) -> Self {
        CoreError::Other(s)
    }
}

impl CoreError {
    /// Full user-facing message, walking the whole `source()` chain — plain `{err}`/`to_string()`
    /// on a `CoreError::Postgres` only prints `tokio_postgres::Error`'s generic per-`Kind` label
    /// (literally the string `"db error"` for `Kind::Db`, `"error communicating with the
    /// server"` for `Kind::Io`, etc.); the actual Postgres-reported reason (severity, message,
    /// `DETAIL`, `HINT`) only lives on the wrapped `DbError`/`io::Error` in `source()`. UI code
    /// that shows an error to the user should call this instead of `{err}`.
    pub fn detailed_message(&self) -> String {
        let mut message = self.to_string();
        let mut source = std::error::Error::source(self);
        while let Some(err) = source {
            message.push_str(": ");
            message.push_str(&err.to_string());
            source = err.source();
        }
        message
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
