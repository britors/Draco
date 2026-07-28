use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Row};

use crate::connection::DbConnection;
use crate::error::Result;
use crate::postgres::tls;
use crate::postgres::tunnel::SshTunnel;

pub struct PostgresDriver {
    pool: Pool,
    app_name: String,
    tunnel: Option<SshTunnel>,
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
            Some(SshTunnel::open(conn, ssh_password, jump_password).await?)
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

    pub async fn disconnect(mut self) {
        self.pool.close();
        if let Some(tunnel) = self.tunnel.take() {
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
