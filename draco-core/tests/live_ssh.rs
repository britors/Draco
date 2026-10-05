//! Exercises the in-process SSH tunnel (`russh`) against real `sshd` servers: a direct tunnel
//! with password and with a passphrase-protected key, a tunnel through a jump host, and the
//! refusals for a wrong password and a changed host key. Ignored by default and, like
//! `live_postgres`, it has no credentials of its own: the PostgreSQL password, the SSH
//! password/key passphrase and the jump host password are read from the Secret Service under
//! `DRACO_TEST_CONN_ID` (kinds `password`, `ssh` and `jump`). Host keys are checked against
//! `~/.ssh/known_hosts`, trust-on-first-use, exactly like the app.
//!
//! `scripts/ci-ssh-tunnel.sh` builds the topology on a disposable runner. Run explicitly with:
//!
//! ```sh
//! DRACO_TEST_CONN_ID=my-conn DRACO_TEST_HOST=127.0.0.1 DRACO_TEST_DB=mydb DRACO_TEST_USER=me \
//! DRACO_TEST_SSH_PORT=2223 DRACO_TEST_SSH_USER=target DRACO_TEST_SSH_KEY=/path/to/key \
//! DRACO_TEST_SSH_JUMP_PORT=2222 DRACO_TEST_SSH_JUMP_USER=bastion \
//! DRACO_TEST_SSH_MISMATCH_PORT=2224 \
//!   cargo test -p draco-core --test live_ssh -- --ignored --nocapture --test-threads=1
//! ```

use draco_core::connection::DbConnection;
use draco_core::postgres::{queries, PostgresDriver};
use draco_core::secrets;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this test (see module docs)"))
}

fn port(name: &str) -> u16 {
    env(name)
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be a port number"))
}

fn ssh_host() -> String {
    std::env::var("DRACO_TEST_SSH_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
}

/// PostgreSQL as seen from the SSH target, reached only through the tunnel.
fn tunnelled_connection() -> DbConnection {
    DbConnection {
        id: env("DRACO_TEST_CONN_ID"),
        label: "live SSH test".to_string(),
        host: env("DRACO_TEST_HOST"),
        port: 5432,
        database: env("DRACO_TEST_DB"),
        user: env("DRACO_TEST_USER"),
        ssh_enabled: true,
        ssh_host: Some(ssh_host()),
        ssh_port: Some(port("DRACO_TEST_SSH_PORT")),
        ssh_user: Some(env("DRACO_TEST_SSH_USER")),
        ..Default::default()
    }
}

async fn postgres_password(conn: &DbConnection) -> String {
    secrets::get_password(&conn.id)
        .await
        .expect("PostgreSQL password readable from Secret Service")
}

async fn ssh_password(conn: &DbConnection) -> String {
    secrets::get_ssh_password(&conn.id)
        .await
        .expect("SSH password readable from Secret Service")
}

async fn assert_query_through_tunnel(
    conn: &DbConnection,
    ssh_password: Option<&str>,
    jump_password: Option<&str>,
) {
    let password = postgres_password(conn).await;
    let driver = PostgresDriver::connect(
        conn,
        &password,
        30_000,
        "draco-live-ssh",
        ssh_password,
        jump_password,
    )
    .await
    .expect("connect to PostgreSQL through the SSH tunnel");
    let result = queries::execute_query(&driver, "SELECT 42 AS answer, current_user AS name")
        .await
        .expect("query through the SSH tunnel");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].get("answer").and_then(|v| v.as_i64()),
        Some(42)
    );
    assert_eq!(
        result.rows[0].get("name").and_then(|v| v.as_str()),
        Some(conn.user.as_str())
    );
}

#[tokio::test]
#[ignore]
async fn direct_tunnel_with_password() {
    let conn = tunnelled_connection();
    let ssh = ssh_password(&conn).await;
    assert_query_through_tunnel(&conn, Some(&ssh), None).await;
}

#[tokio::test]
#[ignore]
async fn direct_tunnel_with_passphrase_protected_key() {
    let mut conn = tunnelled_connection();
    conn.ssh_key_path = Some(env("DRACO_TEST_SSH_KEY"));
    let passphrase = ssh_password(&conn).await;
    assert_query_through_tunnel(&conn, Some(&passphrase), None).await;
}

#[tokio::test]
#[ignore]
async fn tunnel_through_jump_host() {
    let mut conn = tunnelled_connection();
    conn.ssh_key_path = Some(env("DRACO_TEST_SSH_KEY"));
    conn.ssh_jump_host = Some(ssh_host());
    conn.ssh_jump_port = Some(port("DRACO_TEST_SSH_JUMP_PORT"));
    conn.ssh_jump_user = Some(env("DRACO_TEST_SSH_JUMP_USER"));
    let passphrase = ssh_password(&conn).await;
    let jump = secrets::get_jump_ssh_password(&conn.id)
        .await
        .expect("jump host password readable from Secret Service");
    assert_query_through_tunnel(&conn, Some(&passphrase), Some(&jump)).await;
}

#[tokio::test]
#[ignore]
async fn rejects_wrong_ssh_password() {
    let conn = tunnelled_connection();
    let password = postgres_password(&conn).await;
    let result = PostgresDriver::connect(
        &conn,
        &password,
        3_000,
        "draco-live-ssh",
        Some("draco-invalid-ssh-password"),
        None,
    )
    .await;
    assert!(
        result.is_err(),
        "a wrong SSH password unexpectedly opened a tunnel"
    );
}

#[tokio::test]
#[ignore]
async fn rejects_changed_host_key() {
    // The harness records a different key for this server in known_hosts beforehand.
    let mut conn = tunnelled_connection();
    conn.ssh_port = Some(port("DRACO_TEST_SSH_MISMATCH_PORT"));
    let ssh = ssh_password(&conn).await;
    let password = postgres_password(&conn).await;
    let result =
        PostgresDriver::connect(&conn, &password, 3_000, "draco-live-ssh", Some(&ssh), None).await;
    assert!(result.is_err(), "a changed SSH host key was accepted");
}
