use serde::Serialize;
use crate::error::{CoreError, Result};
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct ActivityRow {
    pub pid: i32,
    pub usename: Option<String>,
    pub application_name: Option<String>,
    pub state: Option<String>,
    pub wait_event_type: Option<String>,
    pub wait_event: Option<String>,
    pub duration: Option<String>,
    pub query: Option<String>,
}

pub async fn get_activity(driver: &PostgresDriver) -> Result<Vec<ActivityRow>> {
    let rows = driver
        .query(
            "SELECT pid, usename, application_name, state, wait_event_type, wait_event, \
                    EXTRACT(EPOCH FROM (now() - query_start))::numeric(10,1)::text AS duration, \
                    LEFT(query, 200) AS query \
             FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid() \
             ORDER BY duration DESC NULLS LAST",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| ActivityRow {
            pid: r.try_get("pid").unwrap_or(0),
            usename: get_opt_str(r, "usename"),
            application_name: get_opt_str(r, "application_name"),
            state: get_opt_str(r, "state"),
            wait_event_type: get_opt_str(r, "wait_event_type"),
            wait_event: get_opt_str(r, "wait_event"),
            duration: get_opt_str(r, "duration"),
            query: get_opt_str(r, "query"),
        })
        .collect())
}

pub async fn cancel_activity(driver: &PostgresDriver, pid: i32) -> Result<()> {
    let rows = driver
        .query("SELECT pg_cancel_backend($1) AS cancelled", &[&pid])
        .await?;
    if rows
        .first()
        .and_then(|row| row.try_get::<_, bool>("cancelled").ok())
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(CoreError::Other(
            "PostgreSQL did not cancel the requested activity".to_string(),
        ))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LockRow {
    pub blocked_pid: i32,
    pub blocked_user: Option<String>,
    pub blocked_query: Option<String>,
    pub blocking_pid: i32,
    pub blocking_user: Option<String>,
    pub blocking_query: Option<String>,
    pub locktype: Option<String>,
    pub wait_sec: Option<String>,
}

pub async fn get_locks(driver: &PostgresDriver) -> Result<Vec<LockRow>> {
    let rows = driver
        .query(
            "SELECT
               blocked.pid AS blocked_pid, blocked_act.usename AS blocked_user, LEFT(blocked_act.query, 120) AS blocked_query,
               blocking.pid AS blocking_pid, blocking_act.usename AS blocking_user, LEFT(blocking_act.query, 120) AS blocking_query,
               blocked.locktype, EXTRACT(EPOCH FROM (now() - blocked_act.query_start))::numeric(10,1)::text AS wait_sec
             FROM pg_locks blocked
             JOIN pg_stat_activity blocked_act ON blocked_act.pid = blocked.pid
             JOIN pg_locks blocking ON blocking.transactionid = blocked.transactionid
                                    AND blocking.pid != blocked.pid AND blocking.granted
             JOIN pg_stat_activity blocking_act ON blocking_act.pid = blocking.pid
             WHERE NOT blocked.granted
             ORDER BY wait_sec DESC NULLS LAST",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| LockRow {
            blocked_pid: r.try_get("blocked_pid").unwrap_or(0),
            blocked_user: get_opt_str(r, "blocked_user"),
            blocked_query: get_opt_str(r, "blocked_query"),
            blocking_pid: r.try_get("blocking_pid").unwrap_or(0),
            blocking_user: get_opt_str(r, "blocking_user"),
            blocking_query: get_opt_str(r, "blocking_query"),
            locktype: get_opt_str(r, "locktype"),
            wait_sec: get_opt_str(r, "wait_sec"),
        })
        .collect())
}
