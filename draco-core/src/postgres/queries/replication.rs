//! Read-only replication monitor. A primary reports its WAL senders (`pg_stat_replication`) and
//! replication slots; a standby reports how far it has received and replayed WAL. Without
//! `pg_monitor` (or superuser) PostgreSQL hides the LSN and lag columns of other roles' senders,
//! so `has_monitor_privilege` lets the interface say why values are missing.

use super::helpers::*;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use serde::Serialize;
use tokio_postgres::Row;

#[derive(Debug, Clone, Serialize)]
pub struct ReplicaRow {
    pub pid: i32,
    pub usename: Option<String>,
    pub application_name: Option<String>,
    pub client_addr: Option<String>,
    pub state: Option<String>,
    pub sync_state: Option<String>,
    pub sent_lsn: Option<String>,
    pub replay_lsn: Option<String>,
    /// Bytes between the primary's current WAL position and what the standby wrote, flushed
    /// and replayed.
    pub write_lag_bytes: Option<i64>,
    pub flush_lag_bytes: Option<i64>,
    pub replay_lag_bytes: Option<i64>,
    /// Seconds, as PostgreSQL measured them for the last acknowledged WAL.
    pub write_lag_seconds: Option<String>,
    pub flush_lag_seconds: Option<String>,
    pub replay_lag_seconds: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplicationSlotRow {
    pub slot_name: String,
    pub slot_type: Option<String>,
    pub plugin: Option<String>,
    pub database: Option<String>,
    pub active: bool,
    pub active_pid: Option<i32>,
    /// WAL kept on disk for this slot, from its `restart_lsn` to the current position.
    pub retained_wal_bytes: Option<i64>,
    /// `reserved`, `extended`, `unreserved` or `lost` (PostgreSQL 13+).
    pub wal_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StandbyStatus {
    pub receive_lsn: Option<String>,
    pub replay_lsn: Option<String>,
    /// Received but not yet replayed WAL.
    pub replay_backlog_bytes: Option<i64>,
    pub last_replay_at: Option<String>,
    /// Seconds since the last replayed transaction committed on the primary. It grows while the
    /// primary is idle, so it is an upper bound of the delay, not the delay itself.
    pub seconds_since_last_replay: Option<String>,
    pub receiver_status: Option<String>,
    pub sender_host: Option<String>,
    pub sender_port: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplicationStatus {
    pub in_recovery: bool,
    pub has_monitor_privilege: bool,
    pub server_version_num: i32,
    pub replicas: Vec<ReplicaRow>,
    pub slots: Vec<ReplicationSlotRow>,
    pub standby: Option<StandbyStatus>,
}

pub async fn get_replication_status(driver: &PostgresDriver) -> Result<ReplicationStatus> {
    let rows = driver
        .query(
            "SELECT pg_is_in_recovery() AS in_recovery, \
                    current_setting('server_version_num')::int AS version, \
                    (pg_has_role(current_user, 'pg_monitor', 'USAGE') \
                     OR (SELECT rolsuper FROM pg_roles WHERE rolname = current_user)) AS monitor",
            &[],
        )
        .await?;
    let row = rows.first();
    let in_recovery = row.is_some_and(|row| get_bool(row, "in_recovery"));
    let server_version_num = row.map_or(0, |row| get_i32(row, "version"));
    let has_monitor_privilege = row.is_some_and(|row| get_bool(row, "monitor"));

    // `pg_current_wal_lsn()` raises an error during recovery, so a standby measures from what it
    // has received instead.
    let current_lsn = if in_recovery {
        "pg_last_wal_receive_lsn()"
    } else {
        "pg_current_wal_lsn()"
    };

    let replicas = if in_recovery {
        Vec::new()
    } else {
        driver
            .query(
                &format!(
                    "SELECT pid, usename::text AS usename, application_name, \
                            host(client_addr) AS client_addr, state, sync_state, \
                            sent_lsn::text AS sent_lsn, replay_lsn::text AS replay_lsn, \
                            pg_wal_lsn_diff({current_lsn}, write_lsn)::bigint AS write_lag_bytes, \
                            pg_wal_lsn_diff({current_lsn}, flush_lsn)::bigint AS flush_lag_bytes, \
                            pg_wal_lsn_diff({current_lsn}, replay_lsn)::bigint AS replay_lag_bytes, \
                            {write} AS write_lag_seconds, {flush} AS flush_lag_seconds, \
                            {replay} AS replay_lag_seconds \
                     FROM pg_stat_replication ORDER BY application_name, pid",
                    write = interval_seconds("write_lag"),
                    flush = interval_seconds("flush_lag"),
                    replay = interval_seconds("replay_lag"),
                ),
                &[],
            )
            .await?
            .iter()
            .map(replica_row)
            .collect()
    };

    let wal_status = if server_version_num >= 130_000 {
        "wal_status"
    } else {
        "NULL::text"
    };
    let slots = driver
        .query(
            &format!(
                "SELECT slot_name::text AS slot_name, slot_type, plugin::text AS plugin, \
                        database::text AS database, active, active_pid, \
                        pg_wal_lsn_diff({current_lsn}, restart_lsn)::bigint AS retained_wal_bytes, \
                        {wal_status} AS wal_status \
                 FROM pg_replication_slots ORDER BY slot_name"
            ),
            &[],
        )
        .await?
        .iter()
        .map(slot_row)
        .collect();

    let standby = if in_recovery {
        Some(standby_status(driver, server_version_num).await?)
    } else {
        None
    };

    Ok(ReplicationStatus {
        in_recovery,
        has_monitor_privilege,
        server_version_num,
        replicas,
        slots,
        standby,
    })
}

async fn standby_status(driver: &PostgresDriver, server_version_num: i32) -> Result<StandbyStatus> {
    let rows = driver
        .query(
            "SELECT pg_last_wal_receive_lsn()::text AS receive_lsn, \
                    pg_last_wal_replay_lsn()::text AS replay_lsn, \
                    pg_wal_lsn_diff(pg_last_wal_receive_lsn(), pg_last_wal_replay_lsn())::bigint \
                      AS replay_backlog_bytes, \
                    pg_last_xact_replay_timestamp()::text AS last_replay_at, \
                    EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp()))::numeric(14,1)::text \
                      AS seconds_since_last_replay",
            &[],
        )
        .await?;
    let row = rows.first();
    // `sender_host`/`sender_port` exist from PostgreSQL 11; the view has one row while a WAL
    // receiver runs and none otherwise (for example when restoring from an archive).
    let sender = if server_version_num >= 110_000 {
        "sender_host, sender_port"
    } else {
        "NULL::text AS sender_host, NULL::int AS sender_port"
    };
    let receiver = driver
        .query(
            &format!("SELECT status, {sender} FROM pg_stat_wal_receiver LIMIT 1"),
            &[],
        )
        .await?;
    let receiver = receiver.first();
    Ok(StandbyStatus {
        receive_lsn: row.and_then(|row| get_opt_str(row, "receive_lsn")),
        replay_lsn: row.and_then(|row| get_opt_str(row, "replay_lsn")),
        replay_backlog_bytes: row.and_then(|row| get_opt_i64(row, "replay_backlog_bytes")),
        last_replay_at: row.and_then(|row| get_opt_str(row, "last_replay_at")),
        seconds_since_last_replay: row
            .and_then(|row| get_opt_str(row, "seconds_since_last_replay")),
        receiver_status: receiver.and_then(|row| get_opt_str(row, "status")),
        sender_host: receiver.and_then(|row| get_opt_str(row, "sender_host")),
        sender_port: receiver
            .and_then(|row| row.try_get::<_, Option<i32>>("sender_port").ok().flatten()),
    })
}

fn interval_seconds(column: &str) -> String {
    format!("EXTRACT(EPOCH FROM {column})::numeric(14,3)::text")
}

fn get_opt_i64(row: &Row, column: &str) -> Option<i64> {
    row.try_get::<_, Option<i64>>(column).ok().flatten()
}

fn replica_row(row: &Row) -> ReplicaRow {
    ReplicaRow {
        pid: get_i32(row, "pid"),
        usename: get_opt_str(row, "usename"),
        application_name: get_opt_str(row, "application_name"),
        client_addr: get_opt_str(row, "client_addr"),
        state: get_opt_str(row, "state"),
        sync_state: get_opt_str(row, "sync_state"),
        sent_lsn: get_opt_str(row, "sent_lsn"),
        replay_lsn: get_opt_str(row, "replay_lsn"),
        write_lag_bytes: get_opt_i64(row, "write_lag_bytes"),
        flush_lag_bytes: get_opt_i64(row, "flush_lag_bytes"),
        replay_lag_bytes: get_opt_i64(row, "replay_lag_bytes"),
        write_lag_seconds: get_opt_str(row, "write_lag_seconds"),
        flush_lag_seconds: get_opt_str(row, "flush_lag_seconds"),
        replay_lag_seconds: get_opt_str(row, "replay_lag_seconds"),
    }
}

fn slot_row(row: &Row) -> ReplicationSlotRow {
    ReplicationSlotRow {
        slot_name: get_str(row, "slot_name"),
        slot_type: get_opt_str(row, "slot_type"),
        plugin: get_opt_str(row, "plugin"),
        database: get_opt_str(row, "database"),
        active: get_bool(row, "active"),
        active_pid: row.try_get::<_, Option<i32>>("active_pid").ok().flatten(),
        retained_wal_bytes: get_opt_i64(row, "retained_wal_bytes"),
        wal_status: get_opt_str(row, "wal_status"),
    }
}
