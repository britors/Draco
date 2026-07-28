mod admin;
mod confirm;
mod connection_dialog;
mod dashboard;
mod erd;
mod explorer;
mod function_editor;
mod query_editor;
mod results_grid;
mod search;
mod table_creator;
mod table_detail;
mod table_editor;
mod window_main;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

const APP_ID: &str = "org.lyraos.Draco";

fn register_icon_resources() {
    gio::resources_register_include!("icons.gresource").expect("failed to register embedded Draco icons");
    gtk::Window::set_default_icon_name(APP_ID);

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_resource_path("/org/lyraos/Draco/icons");
    }
}

fn main() -> glib::ExitCode {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_env_var("DRACO_LOG")
        .with_default_directive(tracing::level_filters::LevelFilter::WARN.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // The tokio runtime drives every DB connection (Postgres pool, SSH tunnels) on its own
    // thread; the GTK main loop stays on this (the main) thread. Frontend code hands work
    // between the two by spawning on `runtime_handle` and awaiting the `JoinHandle` inside a
    // `glib::MainContext::spawn_local` block — never by blocking either loop on the other.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start the tokio runtime");
    let runtime_handle = runtime.handle().clone();
    std::thread::Builder::new()
        .name("draco-tokio".to_owned())
        .spawn(move || {
            runtime.block_on(std::future::pending::<()>());
        })
        .expect("failed to spawn the tokio runtime thread");

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| register_icon_resources());
    app.connect_activate(move |app| {
        window_main::build(app, runtime_handle.clone());
    });
    app.run()
}
