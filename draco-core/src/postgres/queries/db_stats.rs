use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct DbStatsOverview {
    pub xact_commit: i64,
    pub xact_rollback: i64,
    pub deadlocks: i64,
    pub temp_files: i64,
    pub cache_hit_pct: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableSizeRow {
    pub schema: String,
    pub table: String,
    pub total_size: String,
    pub table_size: String,
    pub index_size: String,
    pub n_live_tup: Option<i64>,
    pub n_dead_tup: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BloatRow {
    pub schema: String,
    pub table: String,
    pub n_live_tup: Option<i64>,
    pub n_dead_tup: Option<i64>,
    pub bloat_pct: String,
    pub last_autovacuum: Option<String>,
    pub last_vacuum: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnusedIndexRow {
    pub schema: String,
    pub table: String,
    pub index: String,
    pub size: String,
    pub idx_scan: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeqScanRow {
    pub schema: String,
    pub table: String,
    pub seq_scan: i64,
    pub n_live_tup: Option<i64>,
    pub size: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DbStats {
    pub db: Option<DbStatsOverview>,
    pub tables: Vec<TableSizeRow>,
    pub bloat: Vec<BloatRow>,
    pub unused_idx: Vec<UnusedIndexRow>,
    pub seq_scans: Vec<SeqScanRow>,
}

pub async fn get_db_stats(driver: &PostgresDriver) -> Result<DbStats> {
    let db_rows = driver
        .query(
            "SELECT xact_commit, xact_rollback, deadlocks, temp_files, \
                    (CASE WHEN blks_hit + blks_read = 0 THEN '100' \
                          ELSE round(blks_hit::numeric / (blks_hit + blks_read) * 100, 2)::text END) AS cache_hit_pct, \
                    pg_size_pretty(pg_database_size(current_database())) AS size \
             FROM pg_stat_database WHERE datname = current_database()",
            &[],
        )
        .await?;
    let table_rows = driver
        .query(
            "SELECT n.nspname AS schema, c.relname AS table, \
                    pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size, \
                    pg_size_pretty(pg_relation_size(c.oid)) AS table_size, \
                    pg_size_pretty(pg_indexes_size(c.oid)) AS index_size, \
                    s.n_live_tup, s.n_dead_tup \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid \
             WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema') \
             ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 30",
            &[],
        )
        .await?;
    let bloat_rows = driver
        .query(
            "SELECT schemaname AS schema, relname AS table, n_live_tup, n_dead_tup, \
                    (CASE WHEN n_live_tup + n_dead_tup = 0 THEN '0' \
                          ELSE round(n_dead_tup::numeric / (n_live_tup + n_dead_tup) * 100, 1)::text END) AS bloat_pct, \
                    to_char(last_autovacuum, 'YYYY-MM-DD HH24:MI') AS last_autovacuum, \
                    to_char(last_vacuum, 'YYYY-MM-DD HH24:MI') AS last_vacuum \
             FROM pg_stat_user_tables WHERE n_dead_tup > 0 \
             ORDER BY n_dead_tup DESC LIMIT 30",
            &[],
        )
        .await?;
    let unused_idx_rows = driver
        .query(
            "SELECT schemaname AS schema, relname AS table, indexrelname AS index, \
                    pg_size_pretty(pg_relation_size(indexrelid)) AS size, idx_scan \
             FROM pg_stat_user_indexes WHERE idx_scan = 0 \
             ORDER BY pg_relation_size(indexrelid) DESC LIMIT 30",
            &[],
        )
        .await?;
    let seq_scan_rows = driver
        .query(
            "SELECT schemaname AS schema, relname AS table, seq_scan, n_live_tup, \
                    pg_size_pretty(pg_relation_size(relid)) AS size \
             FROM pg_stat_user_tables WHERE seq_scan > 50 AND n_live_tup > 1000 \
             ORDER BY seq_scan DESC LIMIT 20",
            &[],
        )
        .await?;

    Ok(DbStats {
        db: db_rows.first().map(|r| DbStatsOverview {
            xact_commit: get_i64(r, "xact_commit"),
            xact_rollback: get_i64(r, "xact_rollback"),
            deadlocks: get_i64(r, "deadlocks"),
            temp_files: get_i64(r, "temp_files"),
            cache_hit_pct: get_str(r, "cache_hit_pct"),
            size: get_str(r, "size"),
        }),
        tables: table_rows
            .iter()
            .map(|r| TableSizeRow {
                schema: get_str(r, "schema"),
                table: get_str(r, "table"),
                total_size: get_str(r, "total_size"),
                table_size: get_str(r, "table_size"),
                index_size: get_str(r, "index_size"),
                n_live_tup: r.try_get("n_live_tup").ok(),
                n_dead_tup: r.try_get("n_dead_tup").ok(),
            })
            .collect(),
        bloat: bloat_rows
            .iter()
            .map(|r| BloatRow {
                schema: get_str(r, "schema"),
                table: get_str(r, "table"),
                n_live_tup: r.try_get("n_live_tup").ok(),
                n_dead_tup: r.try_get("n_dead_tup").ok(),
                bloat_pct: get_str(r, "bloat_pct"),
                last_autovacuum: get_opt_str(r, "last_autovacuum"),
                last_vacuum: get_opt_str(r, "last_vacuum"),
            })
            .collect(),
        unused_idx: unused_idx_rows
            .iter()
            .map(|r| UnusedIndexRow { schema: get_str(r, "schema"), table: get_str(r, "table"), index: get_str(r, "index"), size: get_str(r, "size"), idx_scan: get_i64(r, "idx_scan") })
            .collect(),
        seq_scans: seq_scan_rows
            .iter()
            .map(|r| SeqScanRow { schema: get_str(r, "schema"), table: get_str(r, "table"), seq_scan: get_i64(r, "seq_scan"), n_live_tup: r.try_get("n_live_tup").ok(), size: get_str(r, "size") })
            .collect(),
    })
}
