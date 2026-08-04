//! Live validation of the application boundary used by the Tauri commands.
//!
//! This test is ignored by default and never stores a password. It reads the password for the
//! source connection from Secret Service, creates a temporary metadata connection, exercises the
//! same application methods exposed by Tauri, and removes only its temporary metadata afterward.

use draco_app::{Application, ConnectionInput};
use draco_core::secrets;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this test"))
}

#[tokio::test]
#[ignore]
async fn application_boundary_reaches_postgres_for_tauri_views() {
    let source_id = env("DRACO_TEST_CONN_ID");
    let password = secrets::get_password(&source_id)
        .await
        .expect("source password available in Secret Service");
    let id = format!("draco-tauri-live-{}", std::process::id());
    let input = ConnectionInput {
        id: Some(id.clone()),
        label: "Tauri live test".to_string(),
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
    };

    let app = Application::new();
    let scenario = AssertUnwindSafe(async {
        app.save_connection(input)
            .await
            .expect("save temporary connection metadata");
        app.connect(&id, &password, 30_000, None, None)
            .await
            .expect("connect through application boundary");

        let schemas = app.list_schemas(&id).await.expect("list schemas");
        assert!(schemas.iter().any(|schema| schema.name == "public"));
        let dashboard = app.dashboard(&id).await.expect("load dashboard");
        assert!(!dashboard.dashboard.is_null());
        let result = app
            .execute_query(&id, "SELECT 1 AS one")
            .await
            .expect("execute query through application boundary");
        assert_eq!(result.rows.len(), 1);
        let _ = app.admin(&id).await.expect("load administration");
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
            if let Err(error) = cleanup {
                eprintln!("application live-test cleanup failed: {error}");
            }
            std::panic::resume_unwind(payload);
        }
    }
}
