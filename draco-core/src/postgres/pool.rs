use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use std::sync::Arc;
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Row};
use tokio::sync::watch;

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
            Some(Arc::new(SshTunnel::open(conn, ssh_password, jump_password).await?))
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
            .options(format!("-c statement_timeout={statement_timeout_ms}"));

        let manager_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
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
        })
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

    /// Same simple-query protocol as `batch_execute`, but keeps each statement's row/command
    /// data instead of discarding it — used by the query editor's "Run as script" so a
    /// multi-statement buffer still shows a result, not just a side effect.
    pub async fn simple_query(&self, sql: &str) -> Result<Vec<tokio_postgres::SimpleQueryMessage>> {
        let client = self.pool.get().await?;
        Ok(client.simple_query(sql).await?)
    }

    /// Runs a query on one pooled backend and cancels that backend if the operation receiver is
    /// signalled. This keeps concurrent queries on the same application connection isolated.
    pub async fn query_cancelable(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        mut cancel_rx: watch::Receiver<bool>,
    ) -> Result<Vec<Row>> {
        let client = self.pool.get().await?;
        let backend_pid = client
            .query_one("SELECT pg_backend_pid()", &[])
            .await?
            .get::<_, i32>(0);
        let query = client.query(sql, params);
        tokio::pin!(query);
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
            result = &mut query => Ok(result?),
            cancelled = &mut cancellation => {
                if cancelled {
                    self.cancel_backend(backend_pid).await?;
                }
                Ok(query.await?)
            }
        }
    }

    /// Simple-query counterpart to `query_cancelable`, used by multi-statement scripts.
    pub async fn simple_query_cancelable(
        &self,
        sql: &str,
        mut cancel_rx: watch::Receiver<bool>,
    ) -> Result<Vec<tokio_postgres::SimpleQueryMessage>> {
        let client = self.pool.get().await?;
        let backend_pid = client
            .query_one("SELECT pg_backend_pid()", &[])
            .await?
            .get::<_, i32>(0);
        let query = client.simple_query(sql);
        tokio::pin!(query);
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
            result = &mut query => Ok(result?),
            cancelled = &mut cancellation => {
                if cancelled {
                    self.cancel_backend(backend_pid).await?;
                }
                Ok(query.await?)
            }
        }
    }

    /// Cancels exactly one backend PID, unlike the legacy application-name based fallback.
    pub async fn cancel_backend(&self, backend_pid: i32) -> Result<()> {
        self.query(
            "SELECT pg_cancel_backend($1)",
            &[&backend_pid],
        )
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
    let driver = PostgresDriver::connect(conn, password, 30_000, "draco-test", ssh_password, jump_password).await?;
    driver.disconnect().await;
    Ok(())
}
