use serde::Serialize;
use crate::error::{CoreError, Result};
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct CronJob {
    pub jobid: i64,
    pub jobname: Option<String>,
    pub schedule: String,
    pub command: String,
    pub database: Option<String>,
    pub username: Option<String>,
    pub active: bool,
    pub last_status: Option<String>,
    pub last_run: Option<String>,
    pub last_msg: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CronJobs {
    pub installed: bool,
    pub jobs: Vec<CronJob>,
}

pub async fn get_jobs(driver: &PostgresDriver) -> Result<CronJobs> {
    let ext_rows = driver.query("SELECT extname FROM pg_extension WHERE extname = 'pg_cron'", &[]).await?;
    if ext_rows.is_empty() {
        return Ok(CronJobs { installed: false, jobs: vec![] });
    }
    let rows = driver
        .query(
            "SELECT j.jobid, j.jobname, j.schedule, j.command, j.database, j.username, j.active, \
                    r.status AS last_status, \
                    to_char(r.start_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS last_run, \
                    r.return_message AS last_msg \
             FROM cron.job j \
             LEFT JOIN LATERAL ( \
               SELECT status, start_time, return_message FROM cron.job_run_details \
               WHERE jobid = j.jobid ORDER BY start_time DESC LIMIT 1 \
             ) r ON true \
             ORDER BY j.jobid",
            &[],
        )
        .await?;
    Ok(CronJobs {
        installed: true,
        jobs: rows
            .iter()
            .map(|r| CronJob {
                jobid: r.try_get("jobid").unwrap_or(0),
                jobname: get_opt_str(r, "jobname"),
                schedule: get_str(r, "schedule"),
                command: get_str(r, "command"),
                database: get_opt_str(r, "database"),
                username: get_opt_str(r, "username"),
                active: get_bool(r, "active"),
                last_status: get_opt_str(r, "last_status"),
                last_run: get_opt_str(r, "last_run"),
                last_msg: get_opt_str(r, "last_msg"),
            })
            .collect(),
    })
}

pub async fn create_job(driver: &PostgresDriver, name: Option<&str>, schedule: &str, command: &str) -> Result<()> {
    match name.filter(|n| !n.trim().is_empty()) {
        Some(name) => driver.query("SELECT cron.schedule($1, $2, $3)", &[&name, &schedule, &command]).await?,
        None => driver.query("SELECT cron.schedule($1, $2)", &[&schedule, &command]).await?,
    };
    Ok(())
}

pub async fn update_job(driver: &PostgresDriver, job_id: i64, schedule: &str, command: &str) -> Result<()> {
    driver
        .query("UPDATE cron.job SET schedule = $1, command = $2 WHERE jobid = $3", &[&schedule, &command, &job_id])
        .await?;
    Ok(())
}

pub async fn toggle_job(driver: &PostgresDriver, job_id: i64, active: bool) -> Result<()> {
    let rows = driver
        .query(
            "UPDATE cron.job SET active = $1 WHERE jobid = $2 RETURNING jobid",
            &[&active, &job_id],
        )
        .await?;
    if rows.is_empty() {
        Err(CoreError::Other("Scheduled job was not found".to_string()))
    } else {
        Ok(())
    }
}

pub async fn delete_job(driver: &PostgresDriver, job_id: i64) -> Result<()> {
    let rows = driver
        .query("SELECT cron.unschedule($1) AS deleted", &[&job_id])
        .await?;
    if rows
        .first()
        .and_then(|row| row.try_get::<_, bool>("deleted").ok())
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(CoreError::Other("Scheduled job was not found".to_string()))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JobRun {
    pub runid: i64,
    pub status: Option<String>,
    pub return_message: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub duration_sec: Option<String>,
}

pub async fn get_job_runs(driver: &PostgresDriver, job_id: i64) -> Result<Vec<JobRun>> {
    let rows = driver
        .query(
            "SELECT runid, status, return_message, \
                    to_char(start_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS start_time, \
                    to_char(end_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS end_time, \
                    (CASE WHEN end_time IS NOT NULL AND start_time IS NOT NULL \
                          THEN round(EXTRACT(EPOCH FROM (end_time - start_time))::numeric, 2)::text \
                          ELSE NULL END) AS duration_sec \
             FROM cron.job_run_details WHERE jobid = $1 ORDER BY start_time DESC LIMIT 50",
            &[&job_id],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| JobRun {
            runid: r.try_get("runid").unwrap_or(0),
            status: get_opt_str(r, "status"),
            return_message: get_opt_str(r, "return_message"),
            start_time: get_opt_str(r, "start_time"),
            end_time: get_opt_str(r, "end_time"),
            duration_sec: get_opt_str(r, "duration_sec"),
        })
        .collect())
}
