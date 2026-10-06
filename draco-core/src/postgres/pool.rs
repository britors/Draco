use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::watch;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{NoTls, Row, SimpleQueryMessage};

use crate::connection::DbConnection;
use crate::error::Result;
use crate::postgres::tls;
use crate::postgres::tunnel::SshTunnel;

#[derive(Clone)]
pub struct PostgresDriver {
    pool: Pool,
    app_name: String,
    tunnel: Option<Arc<SshTunnel>>,
    external_host: String,
    external_port: u16,
    read_only: bool,
}

/// Result of `query_text`: the statement's columns with their types and every row as text
/// (`None` for SQL `NULL`).
#[derive(Debug, Clone, Default)]
pub struct TextQueryResult {
    pub columns: Vec<(String, Type)>,
    pub rows: Vec<Vec<Option<String>>>,
}

async fn text_query(
    client: &tokio_postgres::Client,
    sql: &str,
) -> std::result::Result<TextQueryResult, tokio_postgres::Error> {
    let statement = client.prepare(sql).await?;
    let columns: Vec<(String, Type)> = statement
        .columns()
        .iter()
        .map(|column| (column.name().to_string(), column.type_().clone()))
        .collect();
    let rows = client
        .simple_query(sql)
        .await?
        .into_iter()
        .filter_map(|message| match message {
            SimpleQueryMessage::Row(row) => Some(
                (0..row.len())
                    .map(|index| row.get(index).map(str::to_string))
                    .collect(),
            ),
            _ => None,
        })
        .collect();
    Ok(TextQueryResult { columns, rows })
}

/// Startup options sent to every session of a driver (and, through `PGOPTIONS`, to the external
/// backup/restore tools).
pub(crate) fn session_options(statement_timeout_ms: u32, read_only: bool) -> String {
    let mut options = format!("-c statement_timeout={statement_timeout_ms}");
    if read_only {
        options.push_str(" -c default_transaction_read_only=on");
    }
    options
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalTarget {
    pub host: String,
    pub port: u16,
}

impl PostgresDriver {
    #[allow(clippy::too_many_arguments)]
    pub async fn connect(
        conn: &DbConnection,
        password: &str,
        statement_timeout_ms: u32,
        app_name: &str,
        ssh_password: Option<&str>,
        jump_password: Option<&str>,
    ) -> Result<Self> {
        let tunnel = if conn.ssh_enabled {
            Some(Arc::new(
                SshTunnel::open(conn, ssh_password, jump_password).await?,
            ))
        } else {
            None
        };

        let (host, port) = match &tunnel {
            Some(t) => ("127.0.0.1".to_string(), t.local_port),
            None => (conn.host.clone(), conn.port),
        };

        let mut pg_config = tokio_postgres::Config::new();
        pg_config
            .host(&host)
            .port(port)
            .dbname(&conn.database)
            .user(&conn.user)
            .password(password)
            .application_name(app_name)
            .connect_timeout(std::time::Duration::from_secs(10))
            .options(session_options(statement_timeout_ms, conn.read_only));

        // A read-only session can still be switched back with `SET` from the editor; resetting the
        // setting before a pooled connection is reused brings back the startup value (`on`), so
        // such a change never outlives the statement that made it.
        let recycling_method = if conn.read_only {
            RecyclingMethod::Custom("RESET default_transaction_read_only".to_string())
        } else {
            RecyclingMethod::Fast
        };
        let manager_config = ManagerConfig { recycling_method };
        let manager = if conn.ssl {
            Manager::from_config(pg_config, tls::make_connector()?, manager_config)
        } else {
            Manager::from_config(pg_config, NoTls, manager_config)
        };

        let pool = Pool::builder(manager)
            .max_size(5)
            .runtime(Runtime::Tokio1)
            .build()?;

        // Fail fast on bad credentials/unreachable host instead of surfacing the error lazily
        // on the first query.
        let client = pool.get().await?;
        drop(client);

        Ok(Self {
            pool,
            app_name: app_name.to_string(),
            tunnel,
            external_host: host,
            external_port: port,
            read_only: conn.read_only,
        })
    }

    /// Whether sessions were opened with `default_transaction_read_only = on`.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub async fn disconnect(&self) {
        self.pool.close();
        if let Some(tunnel) = &self.tunnel {
            tunnel.close();
        }
    }

    pub async fn query(&self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Vec<Row>> {
        let client = self.pool.get().await?;
        Ok(client.query(sql, params).await?)
    }

    /// Runs semicolon-separated statements as a single simple-query batch (used for multi-step
    /// DDL like `ALTER TABLE`, wrapped in `BEGIN`/`COMMIT` by the caller so it's atomic).
    pub async fn batch_execute(&self, sql: &str) -> Result<()> {
        let client = self.pool.get().await?;
        client.batch_execute(sql).await?;
        Ok(())
    }

    /// Executes a set of single PostgreSQL statements atomically. The extended query protocol
    /// rejects extra statements hidden in any item, which is important for object editors that
    /// combine a Draco-generated statement with user-reviewed DDL.
    pub async fn execute_transaction(&self, statements: &[&str]) -> Result<()> {
        let mut client = self.pool.get().await?;
        let transaction = client.transaction().await?;
        for statement in statements {
            transaction.execute(*statement, &[]).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Runs read-only catalog queries on one backend inside a `READ ONLY` transaction with an
    /// empty `search_path`, so every `pg_get_*def` output is fully schema-qualified and does not
    /// depend on the session settings. Each statement receives `params`.
    pub async fn catalog_snapshot(
        &self,
        statements: &[&str],
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Vec<Row>>> {
        let mut client = self.pool.get().await?;
        let transaction = client.build_transaction().read_only(true).start().await?;
        transaction
            .batch_execute("SET LOCAL search_path = ''")
            .await?;
        let mut results = Vec::with_capacity(statements.len());
        for statement in statements {
            results.push(transaction.query(*statement, params).await?);
        }
        transaction.rollback().await?;
        Ok(results)
    }

    /// Same simple-query protocol as `batch_execute`, but keeps each statement's row/command
    /// data instead of discarding it — used by the query editor's "Run as script" so a
    /// multi-statement buffer still shows a result, not just a side effect.
    pub async fn simple_query(&self, sql: &str) -> Result<Vec<tokio_postgres::SimpleQueryMessage>> {
        let client = self.pool.get().await?;
        Ok(client.simple_query(sql).await?)
    }

    /// Runs a single statement and returns its rows as PostgreSQL's own text output, the same
    /// representation `psql` shows. The binary protocol used by `query` only decodes the Rust
    /// types tokio-postgres knows, so `numeric`, timestamps, `uuid`, `json`, arrays and the like
    /// would come back empty. The statement is prepared first: that yields the column types even
    /// for an empty result and keeps the extended protocol's guarantee that hidden extra
    /// statements are rejected before anything runs.
    pub async fn query_text(&self, sql: &str) -> Result<TextQueryResult> {
        let client = self.pool.get().await?;
        Ok(text_query(&client, sql).await?)
    }

    /// Cancelable counterpart to `query_text`, used by the query editor.
    pub async fn query_text_cancelable(
        &self,
        sql: &str,
        cancel_rx: watch::Receiver<bool>,
    ) -> Result<TextQueryResult> {
        let client = self.pool.get().await?;
        let backend_pid = client
            .query_one("SELECT pg_backend_pid()", &[])
            .await?
            .get::<_, i32>(0);
        self.cancelable(backend_pid, text_query(&client, sql), cancel_rx)
            .await
    }

    /// Runs a query on one pooled backend and cancels that backend if the operation receiver is
    /// signalled. This keeps concurrent queries on the same application connection isolated.
    pub async fn query_cancelable(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        cancel_rx: watch::Receiver<bool>,
    ) -> Result<Vec<Row>> {
        let client = self.pool.get().await?;
        let backend_pid = client
            .query_one("SELECT pg_backend_pid()", &[])
            .await?
            .get::<_, i32>(0);
        self.cancelable(backend_pid, client.query(sql, params), cancel_rx)
            .await
    }

    /// Simple-query counterpart to `query_cancelable`, used by multi-statement scripts.
    pub async fn simple_query_cancelable(
        &self,
        sql: &str,
        cancel_rx: watch::Receiver<bool>,
    ) -> Result<Vec<tokio_postgres::SimpleQueryMessage>> {
        let client = self.pool.get().await?;
        let backend_pid = client
            .query_one("SELECT pg_backend_pid()", &[])
            .await?
            .get::<_, i32>(0);
        self.cancelable(backend_pid, client.simple_query(sql), cancel_rx)
            .await
    }

    /// Drives `operation` to completion, cancelling `backend_pid` if the receiver is signalled
    /// first. The operation still runs to its end so the server's cancellation error surfaces.
    async fn cancelable<T>(
        &self,
        backend_pid: i32,
        operation: impl Future<Output = std::result::Result<T, tokio_postgres::Error>>,
        mut cancel_rx: watch::Receiver<bool>,
    ) -> Result<T> {
        tokio::pin!(operation);
        let cancellation = async {
            loop {
                if *cancel_rx.borrow() {
                    break true;
                }
                if cancel_rx.changed().await.is_err() {
                    break false;
                }
            }
        };
        tokio::pin!(cancellation);
        tokio::select! {
            result = &mut operation => Ok(result?),
            cancelled = &mut cancellation => {
                if cancelled {
                    self.cancel_backend(backend_pid).await?;
                }
                Ok(operation.await?)
            }
        }
    }

    /// Cancels exactly one backend PID, unlike the legacy application-name based fallback.
    pub async fn cancel_backend(&self, backend_pid: i32) -> Result<()> {
        self.query("SELECT pg_cancel_backend($1)", &[&backend_pid])
            .await?;
        Ok(())
    }

    /// `pg_cancel_backend()` for whatever this driver's own connections are currently running —
    /// used to implement the query editor's Cancel button.
    pub async fn cancel_active(&self) -> Result<()> {
        self.query(
            "SELECT pg_cancel_backend(pid) FROM pg_stat_activity \
             WHERE application_name = $1 AND state = 'active' AND pid <> pg_backend_pid()",
            &[&self.app_name],
        )
        .await?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        !self.pool.is_closed()
    }

    /// Returns the endpoint that command-line PostgreSQL tools must use. For an SSH
    /// connection this is the local listener kept alive by this driver.
    pub(crate) fn external_target(&self) -> ExternalTarget {
        match &self.tunnel {
            Some(tunnel) => ExternalTarget {
                host: "127.0.0.1".to_string(),
                port: tunnel.local_port,
            },
            None => ExternalTarget {
                host: self.external_host.clone(),
                port: self.external_port,
            },
        }
    }
}

pub async fn test_connection(conn: &DbConnection, password: &str) -> Result<()> {
    test_connection_with_ssh(conn, password, None, None).await
}

pub async fn test_connection_with_ssh(
    conn: &DbConnection,
    password: &str,
    ssh_password: Option<&str>,
    jump_password: Option<&str>,
) -> Result<()> {
    let driver = PostgresDriver::connect(
        conn,
        password,
        30_000,
        "draco-test",
        ssh_password,
        jump_password,
    )
    .await?;
    driver.disconnect().await;
    Ok(())
}
