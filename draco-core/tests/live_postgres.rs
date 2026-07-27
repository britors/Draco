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
    let password = secrets::get_password(&conn.id).await.expect("password readable from Secret Service");
    assert!(!password.is_empty(), "expected a password stored under id {} in the Secret Service", conn.id);

    let driver = PostgresDriver::connect(&conn, &password, 30_000, "draco-live-test", None, None)
        .await
        .expect("connect to the real torven database");

    let schemas = queries::get_schemas(&driver).await.expect("get_schemas");
    assert!(schemas.iter().any(|s| s.name == "public"), "expected a public schema, got {schemas:?}");

    let tables = queries::get_tables(&driver, "public").await.expect("get_tables");
    println!("public tables: {tables:?}");

    let result = queries::execute_query(&driver, "SELECT 1 AS one, 'draco' AS label").await.expect("execute_query");
    assert_eq!(result.columns, vec!["one".to_string(), "label".to_string()]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].get("label").and_then(|v| v.as_str()), Some("draco"));

    if let Some(table) = tables.first() {
        let columns = queries::get_columns(&driver, "public", &table.name).await.expect("get_columns");
        println!("columns of {}: {columns:?}", table.name);
        assert!(!columns.is_empty(), "a real table should have at least one column");

        // Same functions the M4 table-detail tab drives.
        let ddl = queries::get_table_ddl(&driver, "public", &table.name).await.expect("get_table_ddl");
        assert!(ddl.starts_with("CREATE TABLE"), "unexpected DDL: {ddl}");
        println!("DDL of {}:\n{ddl}", table.name);

        let indexes = queries::get_indexes(&driver, "public", &table.name).await.expect("get_indexes");
        println!("indexes of {}: {indexes:?}", table.name);

        let constraints = queries::get_constraints(&driver, "public", &table.name).await.expect("get_constraints");
        println!("constraints of {}: {constraints:?}", table.name);

        let fk_map = queries::get_fk_map(&driver, "public", &table.name).await.expect("get_fk_map");
        println!("fk_map of {}: {fk_map:?}", table.name);

        let detail = queries::get_table_detail(&driver, "public", &table.name).await.expect("get_table_detail");
        assert_eq!(detail.columns.len(), columns.len(), "get_table_detail should see the same columns as get_columns");
    }

    // Same functions the M5 connection dashboard drives.
    let dashboard = queries::get_dashboard(&driver).await.expect("get_dashboard");
    println!("dashboard: {dashboard:?}");
    assert!(!dashboard.pg_version.is_empty());
    assert!(dashboard.max_conn > 0);

    let stats = queries::get_db_stats(&driver).await.expect("get_db_stats");
    println!("db_stats: {stats:?}");
    assert!(stats.db.is_some());

    // Same functions the M6 table creator and function editor drive — create real objects,
    // exercise the queries, then drop everything.
    queries::execute_query(
        &driver,
        "CREATE TABLE \"public\".\"draco_live_test_table\" (\n  \"id\" integer NOT NULL,\n  \"label\" text,\n  PRIMARY KEY (\"id\")\n);",
    )
    .await
    .expect("create test table");
    let created = queries::get_tables(&driver, "public").await.expect("get_tables after create");
    assert!(created.iter().any(|t| t.name == "draco_live_test_table"));
    queries::drop_table(&driver, "public", "draco_live_test_table").await.expect("drop test table");

    queries::save_function(
        &driver,
        "CREATE OR REPLACE FUNCTION public.draco_live_test_fn(a integer, b integer)\nRETURNS integer\nLANGUAGE sql\nAS $$ SELECT a + b; $$;",
    )
    .await
    .expect("save_function");

    let overloads = queries::get_function_ddl(&driver, "public", "draco_live_test_fn").await.expect("get_function_ddl");
    assert_eq!(overloads.len(), 1);
    println!("function ddl: {}", overloads[0].ddl);

    let invalid = queries::validate_function(&driver, "CREATE OR REPLACE FUNCTION public.draco_live_test_fn_bad() RETURNS integer LANGUAGE sql AS $$ THIS IS NOT SQL $$;")
        .await
        .expect("validate_function (invalid)");
    assert!(invalid.is_some(), "garbage SQL should fail validation");

    let valid = queries::validate_function(&driver, "CREATE OR REPLACE FUNCTION public.draco_live_test_fn(a integer, b integer) RETURNS integer LANGUAGE sql AS $$ SELECT a + b; $$;")
        .await
        .expect("validate_function (valid)");
    assert!(valid.is_none(), "valid SQL should pass validation, got {valid:?}");

    let call_result = queries::call_function(&driver, "SELECT public.draco_live_test_fn(2, 3) AS sum").await.expect("call_function");
    assert_eq!(call_result.rows[0].get("sum").and_then(|v| v.as_i64()), Some(5));

    driver.query("DROP FUNCTION public.draco_live_test_fn(integer, integer)", &[]).await.expect("cleanup: drop test function");

    // Same functions the M7 admin tab drives — all read-only here (no cancel_activity, no
    // ext_install/drop, no role/job mutation — those are one-line wrappers around DDL/DML
    // already proven safe by the table/function tests above).
    let roles = queries::get_roles(&driver).await.expect("get_roles");
    println!("roles: {} found, e.g. {:?}", roles.len(), roles.first());
    assert!(!roles.is_empty(), "pg_roles should never be empty");

    let activity = queries::get_activity(&driver).await.expect("get_activity");
    println!("activity: {activity:?}");

    let locks = queries::get_locks(&driver).await.expect("get_locks");
    println!("locks: {locks:?}");

    let extensions = queries::get_extensions(&driver).await.expect("get_extensions");
    println!("extensions: {} installed, {} available", extensions.installed.len(), extensions.available.len());
    assert!(!extensions.available.is_empty());

    let sequences = queries::get_sequences(&driver, "public").await.expect("get_sequences");
    println!("sequences: {sequences:?}");

    let jobs = queries::get_jobs(&driver).await.expect("get_jobs");
    println!("jobs: installed={} count={}", jobs.installed, jobs.jobs.len());

    // Same functions the M8 ERD and global search drive.
    let erd = queries::get_erd_data(&driver, "public").await.expect("get_erd_data");
    println!("erd: {} tables, {} relations", erd.tables.len(), erd.relations.len());
    assert_eq!(erd.tables.len(), tables.len());
    assert!(!erd.relations.is_empty(), "api_appointments alone has 3 outgoing FKs, expected at least one relation");

    let search_results = queries::global_search(&driver, "appointment").await.expect("global_search");
    println!("search 'appointment': {search_results:?}");
    assert!(search_results.iter().any(|r| r.name.contains("appointment")));

    driver.disconnect().await;
}
