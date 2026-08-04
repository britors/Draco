//! Programmatic SSH port-forward (not a terminal session) used to reach a Postgres server that
//! sits behind a bastion. Connects in-process via `russh` — no shelling out to the `ssh` binary,
//! since we need a raw local TCP listener forwarding to the remote host, optionally through a
//! jump host (`ProxyJump`).

use std::sync::Arc;

use async_trait::async_trait;
use russh::client::{self, Handle};
use russh::keys::key::PublicKey;
use russh::Disconnect;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::connection::DbConnection;
use crate::error::{CoreError, Result};

struct ClientHandler {
    host: String,
    port: u16,
}

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    /// Trust-on-first-use against the user's real `~/.ssh/known_hosts`, exactly like a plain
    /// `ssh` client on first connect. A *changed* key (possible MITM) is rejected; a read/parse
    /// error on the known_hosts file fails closed.
    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        match russh::keys::check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(true) => Ok(true),
            Ok(false) => {
                if let Err(err) =
                    russh::keys::learn_known_hosts(&self.host, self.port, server_public_key)
                {
                    tracing::warn!("failed to record {}:{} in known_hosts: {err}", self.host, self.port);
                }
                Ok(true)
            }
            Err(err) => {
                tracing::warn!("SSH host key check failed for {}:{}: {err}", self.host, self.port);
                Ok(false)
            }
        }
    }
}

fn default_ssh_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
}

async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    password: Option<&str>,
    key_path: Option<&str>,
) -> Result<()> {
    let ok = if let Some(path) = key_path {
        let key = russh::keys::load_secret_key(path, password)?;
        handle.authenticate_publickey(user, Arc::new(key)).await?
    } else {
        handle
            .authenticate_password(user, password.unwrap_or_default())
            .await?
    };
    if !ok {
        return Err(CoreError::Other(format!("SSH authentication failed for {user}")));
    }
    Ok(())
}

async fn connect_and_authenticate(
    host: &str,
    port: u16,
    user: &str,
    password: Option<&str>,
    key_path: Option<&str>,
) -> Result<Handle<ClientHandler>> {
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(
        config,
        (host, port),
        ClientHandler { host: host.to_string(), port },
    )
    .await?;
    authenticate(&mut handle, user, password, key_path).await?;
    Ok(handle)
}

/// A live SSH tunnel: a local TCP listener whose connections are forwarded, one `direct-tcpip`
/// channel each, to the target host through the (possibly jump-hosted) SSH session.
///
/// The SSH `Handle`s are owned entirely by the accept-loop task (never shared behind an `Arc`),
/// so opening a new forward channel for each accepted connection is sequential inside that one
/// task — cheap enough for a connection pool of a handful of Postgres connections. Only the
/// resulting per-channel byte stream is handed off to its own task for the actual copy.
pub struct SshTunnel {
    pub local_port: u16,
    accept_task: JoinHandle<()>,
}

impl SshTunnel {
    pub async fn open(
        conn: &DbConnection,
        ssh_password: Option<&str>,
        jump_password: Option<&str>,
    ) -> Result<Self> {
        let ssh_host = conn.ssh_host.clone().unwrap_or_else(|| conn.host.clone());
        let ssh_port = conn.ssh_port.unwrap_or(22);
        let ssh_user = conn.ssh_user.clone().unwrap_or_else(default_ssh_user);

        let (jump_handle, main_handle) = match &conn.ssh_jump_host {
            Some(jump_host) => {
                let jump_port = conn.ssh_jump_port.unwrap_or(22);
                let jump_user = conn.ssh_jump_user.clone().unwrap_or_else(default_ssh_user);

                let jump_handle = connect_and_authenticate(
                    jump_host,
                    jump_port,
                    &jump_user,
                    jump_password,
                    conn.ssh_jump_key_path.as_deref(),
                )
                .await?;

                // Tunnel the main SSH session's TCP bytes through a channel opened on the jump
                // session, then run a full second SSH handshake over that channel.
                let channel = jump_handle
                    .channel_open_direct_tcpip(ssh_host.as_str(), ssh_port as u32, "127.0.0.1", 0)
                    .await?;
                let stream = channel.into_stream();

                let config = Arc::new(client::Config::default());
                let mut main_handle = client::connect_stream(
                    config,
                    stream,
                    ClientHandler { host: ssh_host.clone(), port: ssh_port },
                )
                .await?;
                authenticate(&mut main_handle, &ssh_user, ssh_password, conn.ssh_key_path.as_deref())
                    .await?;

                (Some(jump_handle), main_handle)
            }
            None => {
                let handle = connect_and_authenticate(
                    &ssh_host,
                    ssh_port,
                    &ssh_user,
                    ssh_password,
                    conn.ssh_key_path.as_deref(),
                )
                .await?;
                (None, handle)
            }
        };

        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let local_port = listener.local_addr()?.port();

        let target_host = conn.host.clone();
        let target_port = conn.port;

        let accept_task = tokio::spawn(async move {
            // `_jump_handle` is only kept alive here so the jump session isn't torn down while
            // the main session (tunnelled through it) is still in use.
            let _jump_handle = jump_handle;
            loop {
                let (mut socket, _peer) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let channel = match main_handle
                    .channel_open_direct_tcpip(target_host.as_str(), target_port as u32, "127.0.0.1", 0)
                    .await
                {
                    Ok(c) => c,
                    Err(err) => {
                        tracing::warn!("failed to open SSH forward channel: {err}");
                        continue;
                    }
                };
                let mut stream = channel.into_stream();
                tokio::spawn(async move {
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                });
            }
            let _ = main_handle.disconnect(Disconnect::ByApplication, "", "en").await;
        });

        Ok(SshTunnel { local_port, accept_task })
    }

    pub fn close(&self) {
        self.accept_task.abort();
    }
}
