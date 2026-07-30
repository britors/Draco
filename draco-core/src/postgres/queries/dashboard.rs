use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize, Default)]
pub struct DashboardData {
    pub pg_version: String,
    pub uptime: String,
    pub host: String,
    pub port: String,
    pub db_name: String,
    pub db_size: String,
    pub encoding: String,
    pub collation: String,
    pub total_conn: i64,
    pub active_conn: i64,
    pub idle_conn: i64,
    pub idle_in_tx_conn: i64,
    pub max_conn: i64,
    pub commits: i64,
    pub rollbacks: i64,
    pub deadlocks: i64,
    pub temp_files: i64,
    pub cache_hit: String,
    pub top_tables: Vec<TopTable>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopTable {
    pub schema: String,
    pub table: String,
    pub total_size: String,
    pub raw_bytes: i64,
    pub n_live_tup: Option<i64>,
}

pub async fn get_dashboard(driver: &PostgresDriver) -> Result<DashboardData> {
    let server = driver
        .query(
            "SELECT split_part(version(), ' ', 2) AS pg_version, \
                    EXTRACT(EPOCH FROM (now() - pg_postmaster_start_time()))::bigint AS uptime_sec, \
                    inet_server_addr()::text AS host, inet_server_port()::text AS port",
            &[],
        )
        .await?;
    let db = driver
        .query(
            "SELECT current_database() AS db_name, pg_size_pretty(pg_database_size(current_database())) AS db_size, \
                    pg_encoding_to_char(encoding) AS encoding, datcollate AS collation \
             FROM pg_database WHERE datname = current_database()",
            &[],
        )
        .await?;
    let conns = driver
        .query(
            "SELECT count(*) AS total, \
                    count(*) FILTER (WHERE state = 'active') AS active, \
                    count(*) FILTER (WHERE state = 'idle') AS idle, \
                    count(*) FILTER (WHERE state = 'idle in transaction') AS idle_in_tx, \
                    current_setting('max_connections')::bigint AS max_conn \
             FROM pg_stat_activity WHERE datname = current_database()",
            &[],
        )
        .await?;
    let perf = driver
        .query(
            "SELECT xact_commit AS commits, xact_rollback AS rollbacks, deadlocks, temp_files, \
                    (CASE WHEN blks_hit + blks_read = 0 THEN '100' \
                          ELSE round(blks_hit::numeric / (blks_hit + blks_read) * 100, 2)::text END) AS cache_hit \
             FROM pg_stat_database WHERE datname = current_database()",
            &[],
        )
        .await?;
    let top_tables = driver
        .query(
            "SELECT n.nspname AS schema, c.relname AS table, \
                    pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size, \
                    pg_total_relation_size(c.oid) AS raw_bytes, s.n_live_tup \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid \
             WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema') \
             ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 10",
            &[],
        )
        .await?;

    let sv = server.first();
    let dbr = db.first();
    let cn = conns.first();
    let pf = perf.first();

    let uptime_sec = sv.map(|r| get_i64(r, "uptime_sec")).unwrap_or(0);
    let days = uptime_sec / 86400;
    let hours = (uptime_sec % 86400) / 3600;
    let mins = (uptime_sec % 3600) / 60;
    let uptime = if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    };

    Ok(DashboardData {
        pg_version: sv.map(|r| get_str(r, "pg_version")).unwrap_or_default(),
        uptime,
        host: sv.map(|r| get_str(r, "host")).unwrap_or_default(),
        port: sv.map(|r| get_str(r, "port")).unwrap_or_default(),
        db_name: dbr.map(|r| get_str(r, "db_name")).unwrap_or_default(),
        db_size: dbr.map(|r| get_str(r, "db_size")).unwrap_or_default(),
        encoding: dbr.map(|r| get_str(r, "encoding")).unwrap_or_default(),
        collation: dbr.map(|r| get_str(r, "collation")).unwrap_or_default(),
        total_conn: cn.map(|r| get_i64(r, "total")).unwrap_or(0),
        active_conn: cn.map(|r| get_i64(r, "active")).unwrap_or(0),
        idle_conn: cn.map(|r| get_i64(r, "idle")).unwrap_or(0),
        idle_in_tx_conn: cn.map(|r| get_i64(r, "idle_in_tx")).unwrap_or(0),
        max_conn: cn.map(|r| get_i64(r, "max_conn")).unwrap_or(0),
        commits: pf.map(|r| get_i64(r, "commits")).unwrap_or(0),
        rollbacks: pf.map(|r| get_i64(r, "rollbacks")).unwrap_or(0),
        deadlocks: pf.map(|r| get_i64(r, "deadlocks")).unwrap_or(0),
        temp_files: pf.map(|r| get_i64(r, "temp_files")).unwrap_or(0),
        cache_hit: pf.map(|r| get_str(r, "cache_hit")).unwrap_or_default(),
        top_tables: top_tables
            .iter()
            .map(|r| TopTable {
                schema: get_str(r, "schema"),
                table: get_str(r, "table"),
                total_size: get_str(r, "total_size"),
                raw_bytes: get_i64(r, "raw_bytes"),
                n_live_tup: r.try_get("n_live_tup").ok(),
            })
            .collect(),
    })
}
