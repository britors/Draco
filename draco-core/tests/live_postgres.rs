//! Exercises the exact stack the GTK explorer/query editor drive — secrets, connect,
//! introspection, arbitrary query — against a real Postgres. Not run by default (`#[ignore]`),
//! and deliberately has no credentials of its own: it reads everything from env vars, so nothing
//! connection-specific ever needs to be committed. Needs a live server, a connection whose
//! password is already in the Secret Service under `id = DRACO_TEST_CONN_ID`, and no sandboxing
//! around D-Bus. Run explicitly with:
//!
//! ```sh
//! DRACO_TEST_CONN_ID=my-conn DRACO_TEST_HOST=localhost DRACO_TEST_DB=mydb DRACO_TEST_USER=me \
//!   cargo test -p draco-core --test live_postgres -- --ignored --nocapture
//! ```

use draco_core::connection::DbConnection;
use draco_core::postgres::{queries, PostgresDriver};
use draco_core::secrets;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

fn env_or_skip(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this test (see module docs)"))
}

fn test_connection() -> DbConnection {
    DbConnection {
        id: env_or_skip("DRACO_TEST_CONN_ID"),
        label: "live test".to_string(),
        host: env_or_skip("DRACO_TEST_HOST"),
        port: 5432,
        database: env_or_skip("DRACO_TEST_DB"),
        user: env_or_skip("DRACO_TEST_USER"),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore]
async fn connects_and_introspects_the_real_database() {
    let conn = test_connection();
    let password = secrets::get_password(&conn.id)
        .await
        .expect("password readable from Secret Service");
    assert!(
        !password.is_empty(),
        "expected a password stored under id {} in the Secret Service",
        conn.id
    );

    let invalid = PostgresDriver::connect(
        &conn,
        "draco-invalid-password",
        3_000,
        "draco-live-invalid",
        None,
        None,
    )
    .await;
    assert!(
        invalid.is_err(),
        "invalid credentials unexpectedly connected"
    );

    let driver = PostgresDriver::connect(&conn, &password, 30_000, "draco-live-test", None, None)
        .await
        .expect("connect to the real PostgreSQL database");

    let test_schema = format!(
        "draco_live_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_millis()
    );

    // Keep cleanup outside the scenario future. `catch_unwind` lets us drop the isolated schema
    // even when an assertion or a database operation panics halfway through the checklist.
    let scenario = AssertUnwindSafe(async {

    // Remove only schemas created by this test family, including leftovers from an interrupted
    // run. The prefix is deliberately unique to this test and never targets application data.
    let stale_schemas = driver
        .query(
            "SELECT schema_name FROM information_schema.schemata WHERE schema_name LIKE 'draco_live_%'",
            &[],
        )
        .await
        .expect("find stale live-test schemas");
    for schema in stale_schemas {
        let name: String = schema.try_get("schema_name").expect("stale schema name");
        driver
            .batch_execute(&format!("DROP SCHEMA \"{}\" CASCADE", name.replace('"', "\"\"")))
            .await
            .expect("drop stale live-test schema");
    }

    queries::create_schema(&driver, &test_schema).await.expect("create isolated test schema");

    let schemas = queries::get_schemas(&driver).await.expect("get_schemas");
    assert!(schemas.iter().any(|s| s.name == "public"), "expected a public schema, got {schemas:?}");
    assert!(schemas.iter().any(|s| s.name == test_schema), "created test schema is missing from introspection");

    let tables = queries::get_tables(&driver, "public").await.expect("get_tables");

    let _ = queries::get_functions(&driver, "public").await.expect("get_functions");

    let result = queries::execute_query(&driver, "SELECT 1 AS one, 'draco' AS label").await.expect("execute_query");
    assert_eq!(result.columns, vec!["one".to_string(), "label".to_string()]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].get("label").and_then(|v| v.as_str()), Some("draco"));

    if let Some(table) = tables.first() {
        let columns = queries::get_columns(&driver, "public", &table.name).await.expect("get_columns");
        assert!(!columns.is_empty(), "a real table should have at least one column");

        // Same functions the M4 table-detail tab drives.
        let ddl = queries::get_table_ddl(&driver, "public", &table.name).await.expect("get_table_ddl");
        assert!(ddl.starts_with("CREATE TABLE"), "unexpected DDL: {ddl}");

        let _ = queries::get_indexes(&driver, "public", &table.name).await.expect("get_indexes");

        let _ = queries::get_constraints(&driver, "public", &table.name).await.expect("get_constraints");

        let _ = queries::get_fk_map(&driver, "public", &table.name).await.expect("get_fk_map");

        let detail = queries::get_table_detail(&driver, "public", &table.name).await.expect("get_table_detail");
        assert_eq!(detail.columns.len(), columns.len(), "get_table_detail should see the same columns as get_columns");
    }

    // Same functions the M5 connection dashboard drives.
    let dashboard = queries::get_dashboard(&driver).await.expect("get_dashboard");
    assert!(!dashboard.pg_version.is_empty());
    assert!(dashboard.max_conn > 0);

    let stats = queries::get_db_stats(&driver).await.expect("get_db_stats");
    assert!(stats.db.is_some());

    // Same functions the M6 table creator, table editor and data grid drive. Everything lives
    // under a unique schema so a failed run cannot touch an application table.
    let table = "draco_live_test_table";
    let parent_table = "draco_live_test_parent";
    let create_sql = format!(
        "CREATE TABLE \"{test_schema}\".\"{parent_table}\" (\n  \"id\" integer PRIMARY KEY\n);\n\
         CREATE TABLE \"{test_schema}\".\"{table}\" (\n  \"id\" integer GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,\n  \"label\" text,\n  \"parent_id\" integer REFERENCES \"{test_schema}\".\"{parent_table}\" (\"id\")\n);"
    );
    driver
        .batch_execute(&create_sql)
        .await
        .expect("create test table");
    let created = queries::get_tables(&driver, &test_schema).await.expect("get_tables after create");
    assert!(created.iter().any(|t| t.name == table));
    let detail_before = queries::get_table_detail(&driver, &test_schema, table).await.expect("get_table_detail before alter");
    assert_eq!(detail_before.columns.len(), 3);
    assert!(detail_before.fk_map.iter().any(|fk| fk.foreign_table == parent_table));
    let id_type = detail_before.columns.iter().find(|column| column.name == "id").expect("id column").full_type.clone();
    let label_type = detail_before.columns.iter().find(|column| column.name == "label").expect("label column").full_type.clone();
    let parent_id_type = detail_before.columns.iter().find(|column| column.name == "parent_id").expect("parent_id column").full_type.clone();

    let edits = vec![
        queries::ColumnEdit {
            original_name: Some("id".to_string()),
            name: "id".to_string(),
            data_type: id_type,
            nullable: false,
            default: None,
            primary_key: true,
            removed: false,
        },
        queries::ColumnEdit {
            original_name: Some("label".to_string()),
            name: "description".to_string(),
            data_type: label_type,
            nullable: true,
            default: None,
            primary_key: false,
            removed: false,
        },
        queries::ColumnEdit {
            original_name: Some("parent_id".to_string()),
            name: "parent_id".to_string(),
            data_type: parent_id_type,
            nullable: true,
            default: None,
            primary_key: false,
            removed: false,
        },
        queries::ColumnEdit {
            original_name: None,
            name: "status".to_string(),
            data_type: "text".to_string(),
            nullable: false,
            default: Some("'new'".to_string()),
            primary_key: false,
            removed: false,
        },
    ];
    let primary_key_constraint = detail_before
        .constraints
        .iter()
        .find(|constraint| constraint.kind == "PRIMARY KEY")
        .map(|constraint| constraint.name.as_str());
    let alter_statements = queries::build_alter_table_statements(
        &test_schema,
        table,
        table,
        &detail_before.columns,
        &edits,
        primary_key_constraint,
    );
    assert!(!queries::alter_table_is_destructive(&alter_statements));
    queries::alter_table(&driver, &alter_statements).await.expect("alter test table");
    let detail_after = queries::get_table_detail(&driver, &test_schema, table).await.expect("get_table_detail after alter");
    assert!(detail_after.columns.iter().any(|column| column.name == "description"));
    assert!(detail_after.columns.iter().any(|column| column.name == "status"));

    let imported = queries::import_table_rows(
        &driver,
        &test_schema,
        table,
        &["description".to_string()],
        &[vec![Some("first".to_string())], vec![Some("second".to_string())]],
    )
    .await
    .expect("import table rows");
    assert_eq!(imported, 2);
    let browse = queries::browse_table_data(&driver, &test_schema, table, 0, 10).await.expect("browse table data");
    assert_eq!(browse.total, 2);
    let pk_value: i32 = 1;
    queries::update_table_row(&driver, &test_schema, table, &["id".to_string()], &[&pk_value], "description", Some("updated"))
        .await
        .expect("update table row");
    queries::delete_table_row(&driver, &test_schema, table, &["id".to_string()], &[&2_i32])
        .await
        .expect("delete table row");
    queries::run_vacuum(&driver, &test_schema, table, "ANALYZE").await.expect("analyze test table");
    let stats = queries::get_column_stats(&driver, &test_schema, table).await.expect("get_column_stats");
    assert!(stats.iter().any(|stat| stat.column == "description"));

    // Same functions the M6 function editor drives — save, introspect, validate and call.
    let function_name = "draco_live_test_fn";
    let trigger_function = "draco_live_test_trigger_fn";
    queries::save_function(
        &driver,
        &format!("CREATE OR REPLACE FUNCTION \"{test_schema}\".\"{function_name}\"(a integer, b integer)\nRETURNS integer\nLANGUAGE sql\nAS $$ SELECT a + b; $$;"),
    )
    .await
    .expect("save_function");

    let overloads = queries::get_function_ddl(&driver, &test_schema, function_name).await.expect("get_function_ddl");
    assert_eq!(overloads.len(), 1);
    let functions = queries::get_functions(&driver, &test_schema).await.expect("get_functions after create");
    assert!(functions.iter().any(|function| function.name == function_name));

    let invalid = queries::validate_function(&driver, &format!("CREATE OR REPLACE FUNCTION \"{test_schema}\".draco_live_test_fn_bad() RETURNS integer LANGUAGE sql AS $$ THIS IS NOT SQL $$;"))
        .await
        .expect("validate_function (invalid)");
    assert!(invalid.is_some(), "garbage SQL should fail validation");

    let valid = queries::validate_function(&driver, &format!("CREATE OR REPLACE FUNCTION \"{test_schema}\".\"{function_name}\"(a integer, b integer) RETURNS integer LANGUAGE sql AS $$ SELECT a + b; $$;"))
        .await
        .expect("validate_function (valid)");
    assert!(valid.is_none(), "valid SQL should pass validation, got {valid:?}");

    let call_result = queries::call_function(&driver, &format!("SELECT \"{test_schema}\".\"{function_name}\"(2, 3) AS sum")).await.expect("call_function");
    assert_eq!(call_result.rows[0].get("sum").and_then(|v| v.as_i64()), Some(5));

    queries::save_function(
        &driver,
        &format!("CREATE OR REPLACE FUNCTION \"{test_schema}\".\"{trigger_function}\"() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.description := upper(NEW.description); RETURN NEW; END; $$;"),
    )
    .await
    .expect("save trigger function");
    queries::create_sequence(&driver, &test_schema, "draco_live_test_seq").await.expect("create sequence");
    let sequence_value = queries::seq_next_val(&driver, &test_schema, "draco_live_test_seq").await.expect("next sequence value");
    assert_eq!(sequence_value, "1");
    queries::seq_set_val(&driver, &test_schema, "draco_live_test_seq", 41).await.expect("set sequence value");
    assert_eq!(queries::seq_next_val(&driver, &test_schema, "draco_live_test_seq").await.expect("next sequence value after set"), "42");
    queries::create_trigger(&driver, &test_schema, "draco_live_test_trigger", table, "BEFORE", "INSERT", &format!("{test_schema}.{trigger_function}")).await.expect("create trigger");
    let triggers = queries::get_triggers(&driver, &test_schema).await.expect("get_triggers after create");
    assert!(triggers.iter().any(|trigger| trigger.name == "draco_live_test_trigger"));
    let trigger_row = queries::insert_table_row(&driver, &test_schema, table, &["description".to_string()], &[Some("mixed".to_string())]).await;
    assert!(trigger_row.is_ok(), "triggered insert failed: {trigger_row:?}");
    let transformed = queries::execute_query(&driver, &format!("SELECT description FROM \"{test_schema}\".\"{table}\" WHERE description = 'MIXED'"))
        .await
        .expect("read triggered row");
    assert_eq!(transformed.rows.len(), 1, "trigger should have uppercased the inserted value");

    // Same functions the M7 admin tab drives. Job mutation is exercised only when pg_cron is
    // installed; the live database used in CI may legitimately not have that extension.
    let roles = queries::get_roles(&driver).await.expect("get_roles");
    assert!(!roles.is_empty(), "pg_roles should never be empty");

    let _ = queries::get_activity(&driver).await.expect("get_activity");

    let _ = queries::get_locks(&driver).await.expect("get_locks");

    let extensions = queries::get_extensions(&driver).await.expect("get_extensions");
    assert!(!extensions.available.is_empty());

    // pg_stat_statements may legitimately be absent (needs shared_preload_libraries + a server
    // restart, outside what CREATE EXTENSION alone can do), same tolerance as pg_cron below.
    let query_stats = queries::get_query_stats(&driver).await.expect("get_query_stats");
    if !query_stats.installed {
        assert!(query_stats.queries.is_empty());
    }

    let sequences = queries::get_sequences(&driver, &test_schema).await.expect("get_sequences");
    assert!(sequences.iter().any(|sequence| sequence.name == "draco_live_test_seq"));

    let triggers = queries::get_triggers(&driver, &test_schema).await.expect("get_triggers");
    assert!(triggers.iter().any(|trigger| trigger.name == "draco_live_test_trigger"));

    let completion = queries::get_completion_data(&driver).await.expect("get_completion_data");
    assert!(!completion.schemas.is_empty());
    assert!(!completion.tables.is_empty());
    assert!(completion.schemas.iter().any(|schema| schema == &test_schema));
    assert!(completion.tables.iter().any(|entry| entry.schema == test_schema && entry.name == table));
    assert!(completion.functions.iter().any(|entry| entry.schema == test_schema && entry.name == function_name));

    let jobs = queries::get_jobs(&driver).await.expect("get_jobs");
    if jobs.installed {
        let job_name = format!("draco_live_{}_job", std::process::id());
        queries::create_job(&driver, Some(&job_name), "@once", "SELECT 1").await.expect("create temporary cron job");
        let created_jobs = queries::get_jobs(&driver).await.expect("get_jobs after create");
        let job = created_jobs.jobs.iter().find(|job| job.jobname.as_deref() == Some(job_name.as_str())).expect("created cron job");
        queries::update_job(&driver, job.jobid, "@once", "SELECT 1").await.expect("update temporary cron job");
        queries::toggle_job(&driver, job.jobid, false).await.expect("disable temporary cron job");
        queries::toggle_job(&driver, job.jobid, true).await.expect("enable temporary cron job");
        let _ = queries::get_job_runs(&driver, job.jobid).await.expect("get temporary cron job history");
        queries::delete_job(&driver, job.jobid).await.expect("delete temporary cron job");
    }

    let explain = queries::execute_explain(&driver, "SELECT 1").await.expect("execute_explain");
    assert!(!explain.is_null(), "EXPLAIN should return JSON plan data");

    // A statement error must not poison the pool: this mirrors the query editor's error and
    // recovery path without deliberately dropping the real server connection.
    assert!(queries::execute_query(&driver, "SELECT draco_missing_column").await.is_err());
    assert_eq!(queries::execute_query(&driver, "SELECT 2 AS recovered").await.expect("query after error").rows.len(), 1);

    // Same functions the M8 ERD and global search drive.
    let erd = queries::get_erd_data(&driver, "public").await.expect("get_erd_data");
    assert_eq!(erd.tables.len(), tables.len());
    assert!(!erd.relations.is_empty(), "api_appointments alone has 3 outgoing FKs, expected at least one relation");

    let search_results = queries::global_search(&driver, "appointment").await.expect("global_search");
    assert!(search_results.iter().any(|r| r.name.contains("appointment")));

    })
    .catch_unwind()
    .await;

    let cleanup = queries::execute_query(
        &driver,
        &format!("DROP SCHEMA IF EXISTS \"{test_schema}\" CASCADE"),
    )
    .await;
    driver.disconnect().await;

    match scenario {
        Ok(()) => {
            cleanup.expect("cleanup: drop isolated test schema");
        }
        Err(payload) => {
            if cleanup.is_err() {
                eprintln!("live PostgreSQL cleanup failed");
            }
            std::panic::resume_unwind(payload);
        }
    }
}
