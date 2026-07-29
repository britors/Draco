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
use draco_core::postgres::queries::{self, FunctionInfo, SchemaInfo, SequenceInfo, TableInfo, TableKind, TriggerInfo};
use draco_core::{secrets, store};
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

use crate::confirm::confirm_destructive;
use crate::table_editor;

type SharedManager = Arc<Mutex<ConnectionManager>>;
/// `(connection id, schema, table)` — fired by a table row's "View details" button.
type OnOpenTable = Rc<dyn Fn(String, String, String)>;
/// Fired after a table row's Edit/Delete action succeeds — the caller reloads the whole tree
/// (see `refresh_connections` in `window_main.rs`), same shape as `on_new_table` and friends.
type OnTableChanged = Rc<dyn Fn()>;
/// `(connection id, SQL, tab title)` — fired by a table row's "Open SELECT * in New Query" action.
type OnOpenSelect = Rc<dyn Fn(String, String, String)>;
/// `(connection id)` — fired by a connection row's "Dashboard" button.
type OnOpenDashboard = Rc<dyn Fn(String)>;
/// `(connection id, schema, function name — `None` means "create new")`.
type OnOpenFunction = Rc<dyn Fn(String, String, Option<String>)>;
/// `(connection id, schema)` — fired by a schema row's "New Table" button.
type OnNewTable = Rc<dyn Fn(String, String)>;
/// `(connection id, schema)` — fired by a sequence row's "New Sequence" button.
type OnNewSequence = Rc<dyn Fn(String, String)>;
/// `(connection id, schema)` — fired by a trigger row's "New Trigger" button.
type OnNewTrigger = Rc<dyn Fn(String, String)>;
/// `(connection id)` — fired by a connection row's "Admin" button.
type OnOpenAdmin = Rc<dyn Fn(String)>;
/// `(connection id)` — fired by a connection row's "AI Assistant" button.
type OnOpenAssistant = Rc<dyn Fn(String)>;
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
    on_new_sequence: OnNewSequence,
    on_new_trigger: OnNewTrigger,
    on_open_admin: OnOpenAdmin,
    on_open_assistant: OnOpenAssistant,
    on_open_erd: OnOpenErd,
    on_edit_connection: OnEditConnection,
    on_backup_restore: OnBackupRestore,
    on_table_changed: OnTableChanged,
    on_open_select: OnOpenSelect,
}

pub struct Explorer {
    list_box: gtk::ListBox,
    callbacks: Callbacks,
    toasts: adw::ToastOverlay,
}

impl Explorer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        on_open_table: impl Fn(String, String, String) + 'static,
        on_open_dashboard: impl Fn(String) + 'static,
        on_open_function: impl Fn(String, String, Option<String>) + 'static,
        on_new_table: impl Fn(String, String) + 'static,
        on_new_sequence: impl Fn(String, String) + 'static,
        on_new_trigger: impl Fn(String, String) + 'static,
        on_open_admin: impl Fn(String) + 'static,
        on_open_assistant: impl Fn(String) + 'static,
        on_open_erd: impl Fn(String, String) + 'static,
        on_edit_connection: impl Fn(DbConnection) + 'static,
        on_backup_restore: impl Fn(DbConnection) + 'static,
        on_table_changed: impl Fn() + 'static,
        on_open_select: impl Fn(String, String, String) + 'static,
        toasts: adw::ToastOverlay,
    ) -> Self {
        // `.navigation-sidebar` (not `.boxed-list`) is the HIG style class for a left-hand
        // navigation panel like this one — `.boxed-list` is meant for preferences-style card
        // lists, which is why this used to read as a stack of settings cards rather than a
        // sidebar (same treatment Nautilus/Settings/Builder give their left panel).
        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["navigation-sidebar"])
            .valign(gtk::Align::Start)
            .build();
        let callbacks = Callbacks {
            on_open_table: Rc::new(on_open_table),
            on_open_dashboard: Rc::new(on_open_dashboard),
            on_open_function: Rc::new(on_open_function),
            on_new_table: Rc::new(on_new_table),
            on_new_sequence: Rc::new(on_new_sequence),
            on_new_trigger: Rc::new(on_new_trigger),
            on_open_admin: Rc::new(on_open_admin),
            on_open_assistant: Rc::new(on_open_assistant),
            on_open_erd: Rc::new(on_open_erd),
            on_edit_connection: Rc::new(on_edit_connection),
            on_backup_restore: Rc::new(on_backup_restore),
            on_table_changed: Rc::new(on_table_changed),
            on_open_select: Rc::new(on_open_select),
        };
        Self {
            list_box,
            callbacks,
            toasts,
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
                self.toasts.clone(),
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
    toasts: adw::ToastOverlay,
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
    // The same button then doubles as "Disconnect" once connected, rather than disappearing —
    // closing the driver is otherwise unreachable from the UI.
    let connect_btn = gtk::Button::builder()
        .icon_name("network-wired-symbolic")
        .tooltip_text("Connect")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&connect_btn);
    let connected = Rc::new(Cell::new(false));

    // Everything past Connect/Disconnect (Dashboard/Admin/AI Assistant/Edit/Backup/Delete) lives
    // in this one options menu now, so the row only ever shows two buttons — the per-row action
    // buttons multiplied every time a new connection-scoped view was added.
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

    // Disabled until connected — there's nothing to show yet (same gating the standalone buttons
    // used to do via `.visible(false)`, now `.sensitive(false)` since these are menu rows).
    let dashboard_btn = gtk::Button::builder()
        .label("Dashboard")
        .halign(gtk::Align::Start)
        .css_classes(["flat"])
        .sensitive(false)
        .build();
    more_box.append(&dashboard_btn);
    let id_for_dashboard = conn.id.clone();
    dashboard_btn.connect_clicked(clone!(
        #[weak]
        more_popover,
        #[strong]
        id_for_dashboard,
        #[strong]
        callbacks,
        move |_| {
            more_popover.popdown();
            (callbacks.on_open_dashboard)(id_for_dashboard.clone());
        }
    ));

    let admin_btn = gtk::Button::builder()
        .label("Administration")
        .halign(gtk::Align::Start)
        .css_classes(["flat"])
        .sensitive(false)
        .build();
    more_box.append(&admin_btn);
    let id_for_admin = conn.id.clone();
    admin_btn.connect_clicked(clone!(
        #[weak]
        more_popover,
        #[strong]
        id_for_admin,
        #[strong]
        callbacks,
        move |_| {
            more_popover.popdown();
            (callbacks.on_open_admin)(id_for_admin.clone());
        }
    ));

    let assistant_btn = gtk::Button::builder()
        .label("AI Assistant")
        .halign(gtk::Align::Start)
        .css_classes(["flat"])
        .sensitive(false)
        .build();
    more_box.append(&assistant_btn);
    let id_for_assistant = conn.id.clone();
    assistant_btn.connect_clicked(clone!(
        #[weak]
        more_popover,
        #[strong]
        id_for_assistant,
        #[strong]
        callbacks,
        move |_| {
            more_popover.popdown();
            (callbacks.on_open_assistant)(id_for_assistant.clone());
        }
    ));

    more_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

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
        assistant_btn,
        #[strong]
        status_icon,
        #[strong]
        connection_details,
        #[strong]
        connected,
        move |btn| {
            btn.set_sensitive(false);
            let task_id = id.clone();
            let task_manager = manager.clone();

            if connected.get() {
                let handle = runtime.spawn(async move {
                    let mut mgr = task_manager.lock().await;
                    mgr.disconnect(&task_id).await;
                });

                let btn = btn.clone();
                let row = row.clone();
                let dashboard_btn = dashboard_btn.clone();
                let admin_btn = admin_btn.clone();
                let assistant_btn = assistant_btn.clone();
                let status_icon = status_icon.clone();
                let connection_details = connection_details.clone();
                let connected = connected.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = handle.await;
                    connected.set(false);
                    // Rows already fetched stay in the (now hidden) tree — `loaded` isn't reset —
                    // so reconnecting shows them instantly instead of refetching.
                    row.set_expanded(false);
                    row.set_enable_expansion(false);
                    dashboard_btn.set_sensitive(false);
                    admin_btn.set_sensitive(false);
                    assistant_btn.set_sensitive(false);
                    status_icon.remove_css_class("success");
                    status_icon.add_css_class("error");
                    status_icon.set_tooltip_text(Some("Disconnected"));
                    row.set_subtitle(&connection_details);
                    btn.set_icon_name("network-wired-symbolic");
                    btn.set_tooltip_text(Some("Connect"));
                    btn.set_sensitive(true);
                });
                return;
            }

            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
                Ok::<_, draco_core::error::CoreError>(())
            });

            let btn = btn.clone();
            let row = row.clone();
            let dashboard_btn = dashboard_btn.clone();
            let admin_btn = admin_btn.clone();
            let assistant_btn = assistant_btn.clone();
            let status_icon = status_icon.clone();
            let connection_details = connection_details.clone();
            let connected = connected.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok(())) => {
                        connected.set(true);
                        row.set_enable_expansion(true);
                        row.set_expanded(true);
                        dashboard_btn.set_sensitive(true);
                        admin_btn.set_sensitive(true);
                        assistant_btn.set_sensitive(true);
                        status_icon.remove_css_class("error");
                        status_icon.add_css_class("success");
                        status_icon.set_tooltip_text(Some("Connected"));
                        row.set_subtitle(&connection_details);
                        btn.set_icon_name("network-offline-symbolic");
                        btn.set_tooltip_text(Some("Disconnect"));
                        btn.set_sensitive(true);
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
        #[strong]
        toasts,
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
            let toasts_for_task = toasts.clone();
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
                                toasts_for_task.clone(),
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
    toasts: adw::ToastOverlay,
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
        #[strong]
        toasts,
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
                let sequences = queries::get_sequences(driver, &task_schema).await?;
                let triggers = queries::get_triggers(driver, &task_schema).await?;
                Ok::<_, draco_core::error::CoreError>((tables, functions, sequences, triggers))
            });

            let row_for_task = row.clone();
            let runtime_for_task = runtime.clone();
            let manager_for_task = manager.clone();
            let conn_id_for_task = conn_id.clone();
            let schema_for_task = schema_name.clone();
            let callbacks_for_task = callbacks.clone();
            let toasts_for_task = toasts.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok((tables, functions, sequences, triggers))) => {
                        row_for_task.add_row(&build_tables_row(
                            conn_id_for_task.clone(),
                            schema_for_task.clone(),
                            tables,
                            runtime_for_task.clone(),
                            manager_for_task.clone(),
                            callbacks_for_task.on_open_table.clone(),
                            callbacks_for_task.on_table_changed.clone(),
                            callbacks_for_task.on_open_select.clone(),
                            toasts_for_task.clone(),
                        ));
                        row_for_task.add_row(&build_objects_row(
                            conn_id_for_task.clone(),
                            schema_for_task.clone(),
                            functions,
                            sequences,
                            triggers,
                            callbacks_for_task.on_open_function.clone(),
                            callbacks_for_task.on_new_sequence.clone(),
                            callbacks_for_task.on_new_trigger.clone(),
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

#[allow(clippy::too_many_arguments)]
fn build_tables_row(
    conn_id: String,
    schema: String,
    tables: Vec<TableInfo>,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_open_table: OnOpenTable,
    on_table_changed: OnTableChanged,
    on_open_select: OnOpenSelect,
    toasts: adw::ToastOverlay,
) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder().title("Tables").build();
    row.add_prefix(&gtk::Image::from_icon_name("view-grid-symbolic"));
    if tables.is_empty() {
        row.add_row(&adw::ActionRow::builder().title("No tables").build());
    } else {
        for table in tables {
            row.add_row(&build_table_row(
                conn_id.clone(),
                schema.clone(),
                table,
                runtime.clone(),
                manager.clone(),
                on_open_table.clone(),
                on_table_changed.clone(),
                on_open_select.clone(),
                toasts.clone(),
            ));
        }
    }
    row
}

#[allow(clippy::too_many_arguments)]
fn build_objects_row(
    conn_id: String,
    schema: String,
    functions: Vec<FunctionInfo>,
    sequences: Vec<SequenceInfo>,
    triggers: Vec<TriggerInfo>,
    on_open_function: OnOpenFunction,
    on_new_sequence: OnNewSequence,
    on_new_trigger: OnNewTrigger,
) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder().title("Database objects").build();
    row.add_prefix(&gtk::Image::from_icon_name("applications-system-symbolic"));
    row.add_row(&build_functions_row(conn_id.clone(), schema.clone(), functions, on_open_function));

    let sequences_row = adw::ExpanderRow::builder().title("Sequences").build();
    sequences_row.add_prefix(&gtk::Image::from_icon_name("view-sort-ascending-symbolic"));
    let new_sequence_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("New sequence")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    sequences_row.add_suffix(&new_sequence_btn);
    new_sequence_btn.connect_clicked(clone!(
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        on_new_sequence,
        move |_| on_new_sequence(conn_id.clone(), schema.clone())
    ));
    if sequences.is_empty() {
        sequences_row.add_row(&adw::ActionRow::builder().title("No sequences").build());
    } else {
        for sequence in sequences {
            sequences_row.add_row(&adw::ActionRow::builder().title(glib::markup_escape_text(&sequence.name)).build());
        }
    }
    row.add_row(&sequences_row);

    let triggers_row = adw::ExpanderRow::builder().title("Triggers").build();
    triggers_row.add_prefix(&gtk::Image::from_icon_name("system-run-symbolic"));
    let new_trigger_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("New trigger")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    triggers_row.add_suffix(&new_trigger_btn);
    new_trigger_btn.connect_clicked(clone!(
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        on_new_trigger,
        move |_| on_new_trigger(conn_id.clone(), schema.clone())
    ));
    if triggers.is_empty() {
        triggers_row.add_row(&adw::ActionRow::builder().title("No triggers").build());
    } else {
        for trigger in triggers {
            triggers_row.add_row(
                &adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&trigger.name))
                    .subtitle(format!("{} · {}", trigger.table, trigger.definition))
                    .build(),
            );
        }
    }
    row.add_row(&triggers_row);
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
/// Details" menu entry below), reached in one click without cluttering the tree. The
/// "view-more-symbolic" button is a real options menu (not just an alias for the row click) —
/// view/edit/maintenance/destructive actions that don't need a whole tab.
#[allow(clippy::too_many_arguments)]
fn build_table_row(
    conn_id: String,
    schema: String,
    table: TableInfo,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_open_table: OnOpenTable,
    on_table_changed: OnTableChanged,
    on_open_select: OnOpenSelect,
    toasts: adw::ToastOverlay,
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
    let table_name = table.name;

    let open = clone!(
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        on_open_table,
        move || {
            on_open_table(conn_id.clone(), schema.clone(), table_name.clone());
        }
    );
    row.connect_activated(clone!(
        #[strong]
        open,
        move |_| open()
    ));

    let menu_btn = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Table options")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&menu_btn);

    let popover = gtk::Popover::new();
    let popover_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let menu_item = |label: &str| gtk::Button::builder().label(label).halign(gtk::Align::Start).css_classes(["flat"]).build();

    let view_btn = menu_item("View Details");
    popover_box.append(&view_btn);
    view_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        open,
        move |_| {
            popover.popdown();
            open();
        }
    ));

    let edit_btn = menu_item("Edit Table…");
    popover_box.append(&edit_btn);
    edit_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        on_table_changed,
        #[strong]
        toasts,
        move |btn| {
            popover.popdown();
            let Some(parent) = btn.root() else { return };
            let task_conn_id = conn_id.clone();
            let task_schema = schema.clone();
            let task_table = table_name.clone();
            let task_manager = manager.clone();
            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                crate::connection_runtime::ensure_connected(&mut mgr, &task_conn_id).await?;
                let driver = mgr.get_driver(&task_conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
                queries::get_table_detail(driver, &task_schema, &task_table).await
            });
            let runtime = runtime.clone();
            let manager = manager.clone();
            let conn_id = conn_id.clone();
            let schema = schema.clone();
            let table_name = table_name.clone();
            let on_table_changed = on_table_changed.clone();
            let toasts = toasts.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok(detail)) => {
                        table_editor::open(&parent, conn_id, schema, table_name, detail, runtime, manager, move || on_table_changed());
                    }
                    Ok(Err(err)) => toasts.add_toast(adw::Toast::new(&format!("Failed to load table: {err}"))),
                    Err(_) => toasts.add_toast(adw::Toast::new("Failed to load table")),
                }
            });
        }
    ));

    let copy_btn = menu_item("Copy Qualified Name");
    popover_box.append(&copy_btn);
    copy_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        toasts,
        move |btn| {
            popover.popdown();
            btn.clipboard().set_text(&format!("{schema}.{table_name}"));
            toasts.add_toast(adw::Toast::new("Qualified name copied"));
        }
    ));

    let select_btn = menu_item("Open SELECT * in New Query");
    popover_box.append(&select_btn);
    select_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        on_open_select,
        move |_| {
            popover.popdown();
            let preview_row_limit = store::get_settings().preview_row_limit.max(1);
            let sql = format!("SELECT * FROM \"{schema}\".\"{table_name}\" LIMIT {preview_row_limit};");
            on_open_select(conn_id.clone(), sql, format!("{schema}.{table_name}"));
        }
    ));

    let vacuum_btn = menu_item("Vacuum");
    popover_box.append(&vacuum_btn);
    vacuum_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        toasts,
        move |_| {
            popover.popdown();
            run_maintenance(&runtime, &manager, &toasts, conn_id.clone(), schema.clone(), table_name.clone(), "VACUUM", "Vacuum completed", "Vacuum failed");
        }
    ));

    let reindex_btn = menu_item("Reindex Table");
    popover_box.append(&reindex_btn);
    reindex_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        toasts,
        move |_| {
            popover.popdown();
            run_maintenance(&runtime, &manager, &toasts, conn_id.clone(), schema.clone(), table_name.clone(), "REINDEX", "Table reindexed", "Reindex failed");
        }
    ));

    let truncate_btn = menu_item("Truncate Table…");
    popover_box.append(&truncate_btn);
    truncate_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        toasts,
        move |btn| {
            popover.popdown();
            let Some(parent) = btn.root() else { return };
            let runtime = runtime.clone();
            let manager = manager.clone();
            let conn_id = conn_id.clone();
            let schema = schema.clone();
            let table_name = table_name.clone();
            let toasts = toasts.clone();
            confirm_destructive(
                &parent,
                "Truncate table?",
                &format!("This permanently removes every row from \u{201c}{schema}.{table_name}\u{201d}. The table definition stays intact. This cannot be undone."),
                "Truncate Table",
                move || {
                    run_maintenance(&runtime, &manager, &toasts, conn_id.clone(), schema.clone(), table_name.clone(), "TRUNCATE", "Table truncated", "Truncate failed");
                },
            );
        }
    ));

    let delete_btn = menu_item("Delete Table…");
    popover_box.append(&delete_btn);
    delete_btn.connect_clicked(clone!(
        #[weak]
        popover,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table_name,
        #[strong]
        toasts,
        #[strong]
        on_table_changed,
        move |btn| {
            popover.popdown();
            let Some(parent) = btn.root() else { return };
            let task_conn_id = conn_id.clone();
            let task_schema = schema.clone();
            let task_table = table_name.clone();
            let task_manager = manager.clone();
            let runtime = runtime.clone();
            let toasts = toasts.clone();
            let on_table_changed = on_table_changed.clone();
            confirm_destructive(
                &parent,
                "Delete table?",
                &format!("This permanently removes the table \u{201c}{schema}.{table_name}\u{201d} and all of its data. This cannot be undone."),
                "Delete Table",
                move || {
                    let handle = runtime.spawn(async move {
                        let mut mgr = task_manager.lock().await;
                        crate::connection_runtime::ensure_connected(&mut mgr, &task_conn_id).await?;
                        let driver = mgr.get_driver(&task_conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
                        queries::drop_table(driver, &task_schema, &task_table).await
                    });
                    let toasts = toasts.clone();
                    let on_table_changed = on_table_changed.clone();
                    glib::MainContext::default().spawn_local(async move {
                        match handle.await {
                            Ok(Ok(())) => {
                                toasts.add_toast(adw::Toast::new("Table deleted"));
                                on_table_changed();
                            }
                            Ok(Err(err)) => toasts.add_toast(adw::Toast::new(&format!("Delete failed: {err}"))),
                            Err(_) => toasts.add_toast(adw::Toast::new("Delete task failed")),
                        }
                    });
                },
            );
        }
    ));

    popover.set_child(Some(&popover_box));
    menu_btn.set_popover(Some(&popover));

    row
}

/// Shared by Vacuum/Reindex/Truncate — the three "quick" maintenance ops that just need a
/// connected driver, an SQL keyword and a toast on completion (Truncate's confirmation dialog
/// happens before this is called; VACUUM/REINDEX run immediately, matching the non-FULL vacuum
/// ops in `table_detail.rs`'s own maintenance menu).
#[allow(clippy::too_many_arguments)]
fn run_maintenance(
    runtime: &tokio::runtime::Handle,
    manager: &SharedManager,
    toasts: &adw::ToastOverlay,
    conn_id: String,
    schema: String,
    table: String,
    op: &'static str,
    success: &'static str,
    failure: &'static str,
) {
    let task_manager = manager.clone();
    let handle = runtime.spawn(async move {
        let mut mgr = task_manager.lock().await;
        crate::connection_runtime::ensure_connected(&mut mgr, &conn_id).await?;
        let driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
        match op {
            "TRUNCATE" => queries::truncate_table(driver, &schema, &table).await,
            "REINDEX" => queries::reindex_table(driver, &schema, &table).await,
            _ => queries::run_vacuum(driver, &schema, &table, op).await,
        }
    });
    let toasts = toasts.clone();
    glib::MainContext::default().spawn_local(async move {
        match handle.await {
            Ok(Ok(())) => toasts.add_toast(adw::Toast::new(success)),
            Ok(Err(err)) => toasts.add_toast(adw::Toast::new(&format!("{failure}: {err}"))),
            Err(_) => toasts.add_toast(adw::Toast::new(&format!("{failure} (task error)"))),
        }
    });
}
