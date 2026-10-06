//! Live validation of the application boundary used by the Tauri commands.
//!
//! This test is ignored by default and never stores a password. It reads the password for the
//! source connection from Secret Service, creates a temporary metadata connection, exercises the
//! same application methods exposed by Tauri, and removes only its temporary metadata afterward.

use draco_app::{Application, ApplicationError, ConnectionInput, CreateRoleInput};
use draco_core::error::CoreError;
use draco_core::secrets;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this test"))
}

fn live_input(id: &str, label: &str, read_only: bool) -> ConnectionInput {
    ConnectionInput {
        id: Some(id.to_string()),
        label: label.to_string(),
        host: env("DRACO_TEST_HOST"),
        port: 5432,
        database: env("DRACO_TEST_DB"),
        user: env("DRACO_TEST_USER"),
        ssl: false,
        ssh_enabled: false,
        ssh_host: None,
        ssh_port: None,
        ssh_user: None,
        ssh_key_path: None,
        ssh_jump_host: None,
        ssh_jump_port: None,
        ssh_jump_user: None,
        ssh_jump_key_path: None,
        favorite: false,
        environment: None,
        read_only,
    }
}

/// SQLSTATE 25006, raised by PostgreSQL itself (the message text depends on the server locale).
fn is_read_only_transaction_error(error: &ApplicationError) -> bool {
    matches!(
        error,
        ApplicationError::Core(CoreError::Postgres(error))
            if error.code().map(|state| state.code()) == Some("25006")
    )
}

#[tokio::test]
#[ignore]
async fn read_only_connection_is_refused_writes_by_the_server() {
    let source_id = env("DRACO_TEST_CONN_ID");
    let password = secrets::get_password(&source_id)
        .await
        .expect("source password available in Secret Service");
    let writer = format!("draco-tauri-live-writer-{}", std::process::id());
    let reader = format!("draco-tauri-live-reader-{}", std::process::id());
    let schema = format!("draco_live_ro_{}", std::process::id());

    let app = Application::new();
    let scenario = AssertUnwindSafe(async {
        app.save_connection(live_input(&writer, "Read-only live writer", false))
            .await
            .expect("save writer metadata");
        app.save_connection(live_input(&reader, "Read-only live reader", true))
            .await
            .expect("save reader metadata");
        app.connect(&writer, &password, 30_000, None, None)
            .await
            .expect("connect writer");
        app.connect(&reader, &password, 30_000, None, None)
            .await
            .expect("connect read-only reader");
        app.execute_script(
            &writer,
            &format!(
                "CREATE SCHEMA {schema}; CREATE TABLE {schema}.items (id integer PRIMARY KEY); \
                 INSERT INTO {schema}.items VALUES (1)"
            ),
        )
        .await
        .expect("writer creates the fixture");

        let rows = app
            .execute_query(
                &reader,
                &format!("SELECT count(*) AS total FROM {schema}.items"),
            )
            .await
            .expect("read-only connection still reads");
        assert_eq!(rows.rows.len(), 1);

        for sql in [
            format!("INSERT INTO {schema}.items VALUES (2)"),
            format!("UPDATE {schema}.items SET id = 3"),
            format!("DELETE FROM {schema}.items"),
            format!("CREATE TABLE {schema}.other (id integer)"),
            format!("DROP TABLE {schema}.items"),
        ] {
            let error = app
                .execute_query(&reader, &sql)
                .await
                .expect_err("PostgreSQL must refuse a write on a read-only connection");
            assert!(is_read_only_transaction_error(&error), "{sql}: {error}");
        }
        let error = app
            .execute_script(&reader, &format!("TRUNCATE {schema}.items"))
            .await
            .expect_err("scripts are refused too");
        assert!(is_read_only_transaction_error(&error), "{error}");

        // Draco refuses its own write operations and attempts to undo the session setting.
        assert!(matches!(
            app.execute_query(&reader, "SET default_transaction_read_only = off")
                .await,
            Err(ApplicationError::ReadOnly(_))
        ));
        assert!(matches!(
            app.create_schema(&reader, "draco_live_ro_never").await,
            Err(ApplicationError::ReadOnly(_))
        ));

        let survivors = app
            .execute_query(&writer, &format!("SELECT id FROM {schema}.items"))
            .await
            .expect("writer reads the fixture back");
        assert_eq!(
            survivors.rows.len(),
            1,
            "no write went through the read-only connection"
        );
    })
    .catch_unwind()
    .await;

    let cleanup = app
        .execute_query(&writer, &format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .await;
    let _ = app.disconnect(&reader).await;
    let _ = app.disconnect(&writer).await;
    let removed_reader = app.delete_connection(&reader).await;
    let removed_writer = app.delete_connection(&writer).await;
    match scenario {
        Ok(()) => {
            cleanup.expect("drop the read-only fixture");
            removed_reader.expect("remove reader metadata");
            removed_writer.expect("remove writer metadata");
        }
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[tokio::test]
#[ignore]
async fn application_boundary_reaches_postgres_for_tauri_views() {
    let source_id = env("DRACO_TEST_CONN_ID");
    let password = secrets::get_password(&source_id)
        .await
        .expect("source password available in Secret Service");
    let id = format!("draco-tauri-live-{}", std::process::id());
    let input = live_input(&id, "Tauri live test", false);

    let app = Application::new();
    let scenario = AssertUnwindSafe(async {
        app.save_connection(input)
            .await
            .expect("save temporary connection metadata");
        let rejected = app
            .connect(&id, "draco-invalid-password", 3_000, None, None)
            .await;
        assert!(
            rejected.is_err(),
            "invalid credentials unexpectedly connected through application boundary"
        );
        app.connect(&id, &password, 30_000, None, None)
            .await
            .expect("recover from invalid credentials through application boundary");

        let schemas = app.list_schemas(&id).await.expect("list schemas");
        assert!(schemas.iter().any(|schema| schema.name == "public"));
        let tables = app.list_tables(&id, "public").await.expect("list tables");
        if let Some(table) = tables.first() {
            let detail = app
                .table_detail(&id, "public", &table.name)
                .await
                .expect("load full table detail");
            assert!(detail.ddl.starts_with("CREATE TABLE"));
            assert!(detail.column_stats.is_array());
        }
        let _ = app
            .list_schema_objects(&id, "public")
            .await
            .expect("list functions, sequences and triggers");
        let dashboard = app.dashboard(&id).await.expect("load dashboard");
        assert!(!dashboard.dashboard.is_null());
        let result = app
            .execute_query(&id, "SELECT 1 AS one")
            .await
            .expect("execute query through application boundary");
        assert_eq!(result.rows.len(), 1);
        let explain = app
            .execute_explain(&id, "SELECT 1")
            .await
            .expect("execute planning-only EXPLAIN through application boundary");
        assert_eq!(explain.columns, vec!["QUERY PLAN"]);
        assert!(!explain.rows[0]["QUERY PLAN"].is_null());

        let long_query = app.execute_query_with_operation(
            &id,
            "draco-live-cancel-activity",
            "SELECT pg_sleep(10) /* draco_live_cancel_activity */",
        );
        let cancel_from_admin = async {
            let mut target_pid = None;
            for _ in 0..40 {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let admin = app.admin(&id).await.expect("poll activity for long query");
                target_pid = admin.activity.as_array().and_then(|rows| {
                    rows.iter().find_map(|row| {
                        let query = row["query"].as_str()?;
                        if !query.contains("draco_live_cancel_activity") {
                            return None;
                        }
                        row["pid"].as_i64().and_then(|pid| i32::try_from(pid).ok())
                    })
                });
                if target_pid.is_some() {
                    break;
                }
            }
            let pid = target_pid.expect("long query should appear in pg_stat_activity");
            app.cancel_activity(&id, pid)
                .await
                .expect("cancel active query through application boundary");
        };
        let (long_query_result, ()) = tokio::join!(long_query, cancel_from_admin);
        assert!(
            long_query_result.is_err(),
            "cancelled query must return an error"
        );
        let recovery = app
            .execute_query(&id, "SELECT 2 AS recovered")
            .await
            .expect("connection remains usable after activity cancellation");
        assert_eq!(recovery.rows[0]["recovered"].as_i64(), Some(2));

        app.disconnect(&id)
            .await
            .expect("disconnect through application boundary");
        assert!(
            app.execute_query(&id, "SELECT 3 AS disconnected")
                .await
                .is_err(),
            "database operation unexpectedly succeeded while disconnected"
        );
        app.connect(&id, &password, 30_000, None, None)
            .await
            .expect("reconnect after connection loss through application boundary");
        let reconnected = app
            .execute_query(&id, "SELECT 4 AS reconnected")
            .await
            .expect("query succeeds after reconnecting");
        assert_eq!(reconnected.rows[0]["reconnected"].as_i64(), Some(4));

        let _ = app.admin(&id).await.expect("load administration");
        let cron = app
            .list_cron_jobs(&id)
            .await
            .expect("detect pg_cron through application boundary");
        if !cron.installed {
            assert!(cron.jobs.is_empty());
        }
        let extensions = app
            .list_extensions(&id)
            .await
            .expect("list extensions through application boundary");
        assert!(!extensions.available.is_empty());
        let query_stats = app
            .query_stats(&id)
            .await
            .expect("load query stats through application boundary");
        if !query_stats.installed {
            assert!(query_stats.queries.is_empty());
        }
        let roles = app.list_roles(&id).await.expect("list PostgreSQL roles");
        assert!(!roles.is_empty());

        let privilege = app
            .execute_query(
                &id,
                "SELECT rolsuper FROM pg_roles WHERE rolname = current_user",
            )
            .await
            .expect("inspect current role privilege");
        if privilege.rows[0]["rolsuper"].as_bool() == Some(true) {
            let role_name = format!(
                "draco_live_role_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock after epoch")
                    .as_millis()
            );
            app.create_role(
                &id,
                CreateRoleInput {
                    name: role_name.clone(),
                    login: false,
                    create_database: false,
                    create_role: false,
                    superuser: false,
                    connection_limit: 2,
                    valid_until: None,
                },
            )
            .await
            .expect("create temporary role through application boundary");
            let listed = app.list_roles(&id).await;
            let cleanup = app.delete_role(&id, &role_name).await;
            let listed = listed.expect("reload roles after creation");
            cleanup.expect("delete temporary role through application boundary");
            assert!(listed.iter().any(|role| {
                role.name == role_name && !role.login && role.connection_limit == 2
            }));
        }
    })
    .catch_unwind()
    .await;

    // The temporary metadata and connection must be removed on both success and assertion
    // failure. `disconnect` is best-effort because connect may have failed.
    let _ = app.disconnect(&id).await;
    let cleanup = app.delete_connection(&id).await;
    match scenario {
        Ok(()) => cleanup.expect("remove temporary metadata"),
        Err(payload) => {
            if cleanup.is_err() {
                eprintln!("application live-test cleanup failed");
            }
            std::panic::resume_unwind(payload);
        }
    }
}
