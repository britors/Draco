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
    Secret(#[from] oo7::Error),
    #[error("{0}")]
    Other(String),
}

impl From<String> for CoreError {
    fn from(s: String) -> Self {
        CoreError::Other(s)
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
