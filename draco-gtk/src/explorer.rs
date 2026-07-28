//! Sidebar: connections expand into schemas, schemas expand into tables/functions. Tables are
//! leaf rows — columns live in the table detail tab, not inline here (see `table_detail.rs`).
//! Rebuilt on every refresh (connection count is small); each level's children are fetched
//! lazily, on first expand — same "plain nested boxes, rebuilt on refresh" philosophy as
//! `sulafat-gtk::host_list`, one level deeper.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::connection::DbConnection;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, FunctionInfo, SchemaInfo, TableInfo, TableKind};
use draco_core::{secrets, store};
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

use crate::confirm::confirm_destructive;

type SharedManager = Arc<Mutex<ConnectionManager>>;
/// `(connection id, schema, table)` — fired by a table row's "View details" button.
type OnOpenTable = Rc<dyn Fn(String, String, String)>;
/// `(connection id)` — fired by a connection row's "Dashboard" button.
type OnOpenDashboard = Rc<dyn Fn(String)>;
/// `(connection id, schema, function name — `None` means "create new")`.
type OnOpenFunction = Rc<dyn Fn(String, String, Option<String>)>;
/// `(connection id, schema)` — fired by a schema row's "New Table" button.
type OnNewTable = Rc<dyn Fn(String, String)>;
/// `(connection id)` — fired by a connection row's "Admin" button.
type OnOpenAdmin = Rc<dyn Fn(String)>;
/// `(connection id, schema)` — fired by a schema row's "ERD" button.
type OnOpenErd = Rc<dyn Fn(String, String)>;
/// The connection being edited — fired by a connection row's "Edit Connection" action.
type OnEditConnection = Rc<dyn Fn(DbConnection)>;
/// The connection's backup/restore dialog — fired by a connection row's options menu.
type OnBackupRestore = Rc<dyn Fn(DbConnection)>;

/// Bundles every "open something in a tab/dialog" callback the Explorer's rows can fire, so
/// adding a new one doesn't mean threading another parameter through every builder function.
#[derive(Clone)]
struct Callbacks {
    on_open_table: OnOpenTable,
    on_open_dashboard: OnOpenDashboard,
    on_open_function: OnOpenFunction,
    on_new_table: OnNewTable,
    on_open_admin: OnOpenAdmin,
    on_open_erd: OnOpenErd,
    on_edit_connection: OnEditConnection,
    on_backup_restore: OnBackupRestore,
}

pub struct Explorer {
    list_box: gtk::ListBox,
    callbacks: Callbacks,
}

impl Explorer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        on_open_table: impl Fn(String, String, String) + 'static,
        on_open_dashboard: impl Fn(String) + 'static,
        on_open_function: impl Fn(String, String, Option<String>) + 'static,
        on_new_table: impl Fn(String, String) + 'static,
        on_open_admin: impl Fn(String) + 'static,
        on_open_erd: impl Fn(String, String) + 'static,
        on_edit_connection: impl Fn(DbConnection) + 'static,
        on_backup_restore: impl Fn(DbConnection) + 'static,
    ) -> Self {
        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .valign(gtk::Align::Start)
            .build();
        let callbacks = Callbacks {
            on_open_table: Rc::new(on_open_table),
            on_open_dashboard: Rc::new(on_open_dashboard),
            on_open_function: Rc::new(on_open_function),
            on_new_table: Rc::new(on_new_table),
            on_open_admin: Rc::new(on_open_admin),
            on_open_erd: Rc::new(on_open_erd),
            on_edit_connection: Rc::new(on_edit_connection),
            on_backup_restore: Rc::new(on_backup_restore),
        };
        Self {
            list_box,
            callbacks,
        }
    }

    pub fn widget(&self) -> &gtk::ListBox {
        &self.list_box
    }

    pub fn set_connections(
        &self,
        connections: Vec<DbConnection>,
        runtime: tokio::runtime::Handle,
        manager: SharedManager,
    ) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
        for conn in connections {
            self.list_box.append(&build_connection_row(
                conn,
                runtime.clone(),
                manager.clone(),
                self.callbacks.clone(),
                self.list_box.clone(),
            ));
        }
    }
}

fn build_connection_row(
    conn: DbConnection,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    callbacks: Callbacks,
    list_box: gtk::ListBox,
) -> adw::ExpanderRow {
    let connection_details = format!("{}@{}:{}/{}", conn.user, conn.host, conn.port, conn.database);
    let row = adw::ExpanderRow::builder()
        .title(glib::markup_escape_text(&conn.label))
        .subtitle(&connection_details)
        .enable_expansion(false)
        .build();
    let status_icon = gtk::Image::builder()
        .icon_name("drive-harddisk-symbolic")
        .css_classes(["error"])
        .build();
    row.add_prefix(&status_icon);

    // Before connecting: just a "Connect" action, no expand arrow and no other actions — there's
    // nothing to browse/administer yet. All of that appears only once the connection succeeds.
    let connect_btn = gtk::Button::builder()
        .icon_name("network-wired-symbolic")
        .tooltip_text("Connect")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&connect_btn);

    let dashboard_btn = gtk::Button::builder()
        .icon_name("org.gnome.SystemMonitor-symbolic")
        .tooltip_text("Connection dashboard")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .visible(false)
        .build();
    row.add_suffix(&dashboard_btn);
    let id_for_dashboard = conn.id.clone();
    dashboard_btn.connect_clicked(clone!(
        #[strong]
        id_for_dashboard,
        #[strong]
        callbacks,
        move |_| {
            (callbacks.on_open_dashboard)(id_for_dashboard.clone());
        }
    ));

    let admin_btn = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("Administration (roles, jobs, activity, extensions)")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .visible(false)
        .build();
    row.add_suffix(&admin_btn);
    let id_for_admin = conn.id.clone();
    admin_btn.connect_clicked(clone!(
        #[strong]
        id_for_admin,
        #[strong]
        callbacks,
        move |_| {
            (callbacks.on_open_admin)(id_for_admin.clone());
        }
    ));

    let more_btn = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Connection options")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let more_popover = gtk::Popover::new();
    let more_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let edit_btn = gtk::Button::builder()
        .label("Edit Connection")
        .halign(gtk::Align::Start)
        .css_classes(["flat"])
        .build();
    more_box.append(&edit_btn);
    edit_btn.connect_clicked(clone!(
        #[weak]
        more_popover,
        #[strong]
        callbacks,
        #[strong]
        conn,
        move |_| {
            more_popover.popdown();
            (callbacks.on_edit_connection)(conn.clone());
        }
    ));

    let backup_restore_btn = gtk::Button::builder()
        .label("Backup / Restore…")
        .halign(gtk::Align::Start)
        .css_classes(["flat"])
        .build();
    more_box.append(&backup_restore_btn);
    backup_restore_btn.connect_clicked(clone!(
        #[weak]
        more_popover,
        #[strong]
        callbacks,
        #[strong]
        conn,
        move |_| {
            more_popover.popdown();
            (callbacks.on_backup_restore)(conn.clone());
        }
    ));

    let delete_btn = gtk::Button::builder()
        .label("Delete Connection")
        .halign(gtk::Align::Start)
        .css_classes(["flat"])
        .build();
    more_box.append(&delete_btn);
    let id_for_delete = conn.id.clone();
    let label_for_delete = conn.label.clone();
    delete_btn.connect_clicked(clone!(
        #[weak]
        more_popover,
        #[weak]
        row,
        #[strong]
        list_box,
        #[strong]
        id_for_delete,
        #[strong]
        label_for_delete,
        #[strong]
        runtime,
        move |btn| {
            more_popover.popdown();
            let Some(parent) = btn.root() else { return };
            let id = id_for_delete.clone();
            let label = label_for_delete.clone();
            let list_box = list_box.clone();
            let row = row.clone();
            let runtime = runtime.clone();
            confirm_destructive(
                &parent,
                "Delete connection?",
                &format!("This permanently removes the saved connection \u{201c}{label}\u{201d} (its stored password is also removed). This cannot be undone."),
                "Delete Connection",
                move || {
                    let _ = store::delete_connection(&id);
                    list_box.remove(&row);
                    // Best-effort: the connection is already gone from the list either way, so
                    // this doesn't block the UI or surface its own errors.
                    runtime.spawn(async move {
                        let _ = secrets::remove_password(&id).await;
                    });
                },
            );
        }
    ));

    more_popover.set_child(Some(&more_box));
    more_btn.set_popover(Some(&more_popover));
    row.add_suffix(&more_btn);

    let loaded = Rc::new(Cell::new(false));
    let id = conn.id;

    connect_btn.connect_clicked(clone!(
        #[weak]
        row,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        id,
        #[strong]
        dashboard_btn,
        #[strong]
        admin_btn,
        #[strong]
        status_icon,
        #[strong]
        connection_details,
        move |btn| {
            btn.set_sensitive(false);
            let task_id = id.clone();
            let task_manager = manager.clone();
            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
                Ok::<_, draco_core::error::CoreError>(())
            });

            let btn = btn.clone();
            let row = row.clone();
            let dashboard_btn = dashboard_btn.clone();
            let admin_btn = admin_btn.clone();
            let status_icon = status_icon.clone();
            let connection_details = connection_details.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok(())) => {
                        row.set_enable_expansion(true);
                        row.set_expanded(true);
                        btn.set_visible(false);
                        dashboard_btn.set_visible(true);
                        admin_btn.set_visible(true);
                        status_icon.remove_css_class("error");
                        status_icon.add_css_class("success");
                        status_icon.set_tooltip_text(Some("Connected"));
                        row.set_subtitle(&connection_details);
                    }
                    Ok(Err(err)) => {
                        btn.set_tooltip_text(Some(&format!("Failed to connect: {err}")));
                        status_icon.set_tooltip_text(Some(&format!("Connection failed: {err}")));
                        row.set_subtitle(&format!("Connection failed: {err}"));
                        btn.set_sensitive(true);
                    }
                    Err(_) => {
                        status_icon.set_tooltip_text(Some("Connection task failed"));
                        row.set_subtitle("Connection task failed");
                        btn.set_sensitive(true);
                    }
                }
            });
        }
    ));

    row.connect_expanded_notify(clone!(
        #[weak]
        row,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        loaded,
        #[strong]
        id,
        #[strong]
        callbacks,
        move |exp_row| {
            if !exp_row.is_expanded() || loaded.get() {
                return;
            }
            loaded.set(true);

            let spinner_row = adw::ActionRow::builder().title("Loading…").build();
            row.add_row(&spinner_row);

            let task_id = id.clone();
            let task_manager = manager.clone();
            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
                let driver = mgr
                    .get_driver(&task_id)
                    .ok_or(draco_core::error::CoreError::NotConnected)?;
                queries::get_schemas(driver).await
            });

            let row_for_task = row.clone();
            let runtime_for_task = runtime.clone();
            let manager_for_task = manager.clone();
            let conn_id_for_task = id.clone();
            let callbacks_for_task = callbacks.clone();
            glib::MainContext::default().spawn_local(async move {
                row_for_task.remove(&spinner_row);
                match handle.await {
                    Ok(Ok(schemas)) => {
                        if schemas.is_empty() {
                            row_for_task
                                .add_row(&adw::ActionRow::builder().title("No schemas").build());
                        }
                        for schema in schemas {
                            row_for_task.add_row(&build_schema_row(
                                conn_id_for_task.clone(),
                                schema,
                                runtime_for_task.clone(),
                                manager_for_task.clone(),
                                callbacks_for_task.clone(),
                            ));
                        }
                    }
                    Ok(Err(err)) => {
                        row_for_task.add_row(
                            &adw::ActionRow::builder()
                                .title(format!("Failed to connect: {err}"))
                                .build(),
                        );
                    }
                    Err(_) => {}
                }
            });
        }
    ));

    row
}

fn build_schema_row(
    conn_id: String,
    schema: SchemaInfo,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    callbacks: Callbacks,
) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder()
        .title(glib::markup_escape_text(&schema.name))
        .build();
    row.add_prefix(&gtk::Image::from_icon_name("folder-symbolic"));

    let erd_btn = gtk::Button::builder()
        .icon_name("network-workgroup-symbolic")
        .tooltip_text("Entity-Relationship Diagram")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&erd_btn);

    let new_table_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("New table")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&new_table_btn);

    let loaded = Rc::new(Cell::new(false));
    let schema_name = schema.name;

    new_table_btn.connect_clicked(clone!(
        #[strong]
        conn_id,
        #[strong]
        schema_name,
        #[strong]
        callbacks,
        move |_| {
            (callbacks.on_new_table)(conn_id.clone(), schema_name.clone());
        }
    ));

    erd_btn.connect_clicked(clone!(
        #[strong]
        conn_id,
        #[strong]
        schema_name,
        #[strong]
        callbacks,
        move |_| {
            (callbacks.on_open_erd)(conn_id.clone(), schema_name.clone());
        }
    ));

    row.connect_expanded_notify(clone!(
        #[weak]
        row,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        loaded,
        #[strong]
        conn_id,
        #[strong]
        schema_name,
        #[strong]
        callbacks,
        move |exp_row| {
            if !exp_row.is_expanded() || loaded.get() {
                return;
            }
            loaded.set(true);

            let task_id = conn_id.clone();
            let task_schema = schema_name.clone();
            let task_manager = manager.clone();
            let handle = runtime.spawn(async move {
                let mgr = task_manager.lock().await;
                let driver = mgr
                    .get_driver(&task_id)
                    .ok_or(draco_core::error::CoreError::NotConnected)?;
                let tables = queries::get_tables(driver, &task_schema).await?;
                let functions = queries::get_functions(driver, &task_schema).await?;
                Ok::<_, draco_core::error::CoreError>((tables, functions))
            });

            let row_for_task = row.clone();
            let conn_id_for_task = conn_id.clone();
            let schema_for_task = schema_name.clone();
            let callbacks_for_task = callbacks.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok((tables, functions))) => {
                        if tables.is_empty() {
                            row_for_task
                                .add_row(&adw::ActionRow::builder().title("No tables").build());
                        }
                        for table in tables {
                            row_for_task.add_row(&build_table_row(
                                conn_id_for_task.clone(),
                                schema_for_task.clone(),
                                table,
                                callbacks_for_task.on_open_table.clone(),
                            ));
                        }
                        row_for_task.add_row(&build_functions_row(
                            conn_id_for_task.clone(),
                            schema_for_task.clone(),
                            functions,
                            callbacks_for_task.on_open_function.clone(),
                        ));
                    }
                    Ok(Err(err)) => {
                        row_for_task.add_row(
                            &adw::ActionRow::builder()
                                .title(format!("Failed to load tables: {err}"))
                                .build(),
                        );
                    }
                    Err(_) => {}
                }
            });
        }
    ));

    row
}

fn build_functions_row(
    conn_id: String,
    schema: String,
    functions: Vec<FunctionInfo>,
    on_open_function: OnOpenFunction,
) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder().title("Functions").build();
    row.add_prefix(&gtk::Image::from_icon_name("system-run-symbolic"));

    let new_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("New function")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&new_btn);
    new_btn.connect_clicked(clone!(
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        on_open_function,
        move |_| {
            on_open_function(conn_id.clone(), schema.clone(), None);
        }
    ));

    if functions.is_empty() {
        row.add_row(&adw::ActionRow::builder().title("No functions").build());
    }
    for func in functions {
        let func_row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&func.name))
            .subtitle(format!("→ {}", func.return_type))
            .build();
        let edit_btn = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Edit function")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        func_row.add_suffix(&edit_btn);
        edit_btn.connect_clicked(clone!(
            #[strong]
            conn_id,
            #[strong]
            schema,
            #[strong]
            on_open_function,
            #[strong(rename_to = func_name)]
            func.name,
            move |_| {
                on_open_function(conn_id.clone(), schema.clone(), Some(func_name.clone()));
            }
        ));
        row.add_row(&func_row);
    }

    row
}

/// A leaf row — no inline column expansion. Columns live in the table detail tab (the "View
/// details" button below), reached in one click without cluttering the tree.
fn build_table_row(
    conn_id: String,
    schema: String,
    table: TableInfo,
    on_open_table: OnOpenTable,
) -> adw::ActionRow {
    let icon_name = if table.kind == TableKind::View {
        "view-list-symbolic"
    } else {
        "view-grid-symbolic"
    };
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&table.name))
        .activatable(true)
        .build();
    row.add_prefix(&gtk::Image::from_icon_name(icon_name));

    let details_btn = gtk::Button::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("View details (DDL, indexes, constraints, FKs)")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&details_btn);

    let open = clone!(
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong(rename_to = table_name)]
        table.name,
        #[strong]
        on_open_table,
        move || {
            on_open_table(conn_id.clone(), schema.clone(), table_name.clone());
        }
    );
    details_btn.connect_clicked(clone!(
        #[strong]
        open,
        move |_| open()
    ));
    row.connect_activated(move |_| open());

    row
}
