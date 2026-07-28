//! Table detail tab: DDL, columns, indexes, constraints and FK map for one table — opened from
//! the Explorer's "View details" button on a table row.

use std::sync::Arc;
use std::rc::Rc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, ConstraintInfo, FkDirection, FkMapEntry, IndexInfo, TableDetail, TableDetailColumn};
use gtk::glib;
use gtk::glib::clone;
use sourceview5::prelude::*;
use tokio::sync::Mutex;

use crate::confirm::confirm_destructive;
use crate::table_editor;

const VACUUM_OPS: &[&str] = &["VACUUM", "ANALYZE", "VACUUM ANALYZE", "VACUUM FULL"];

type SharedManager = Arc<Mutex<ConnectionManager>>;

pub struct TableDetailView {
    root: gtk::Box,
}

impl TableDetailView {
    pub fn new_with_navigation(
        conn_id: String,
        schema: String,
        table: String,
        runtime: tokio::runtime::Handle,
        manager: SharedManager,
        on_open_table: Rc<dyn Fn(String, String, String)>,
    ) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
        load(root.clone(), conn_id, schema, table, runtime, manager, on_open_table);
        Self { root }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

fn load(
    root: gtk::Box,
    conn_id: String,
    schema: String,
    table: String,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_open_table: Rc<dyn Fn(String, String, String)>,
) {
    while let Some(child) = root.first_child() {
        root.remove(&child);
    }

    let spinner = gtk::Spinner::builder().spinning(true).margin_top(24).margin_bottom(24).halign(gtk::Align::Center).build();
    root.append(&spinner);

    let task_conn_id = conn_id.clone();
    let task_schema = schema.clone();
    let task_table = table.clone();
    let task_manager = manager.clone();
    let handle = runtime.spawn(async move {
        let mgr = task_manager.lock().await;
        let driver = mgr.get_driver(&task_conn_id).ok_or(CoreError::NotConnected)?;
        let detail = queries::get_table_detail(driver, &task_schema, &task_table).await?;
        let ddl = queries::get_table_ddl(driver, &task_schema, &task_table).await?;
        Ok::<_, CoreError>((detail, ddl))
    });

    let root_for_task = root.clone();
    glib::MainContext::default().spawn_local(async move {
        root_for_task.remove(&spinner);
        match handle.await {
            Ok(Ok((detail, ddl))) => populate(&root_for_task, conn_id, schema, table, detail, ddl, runtime, manager, on_open_table),
            Ok(Err(err)) => {
                root_for_task.append(&adw::StatusPage::builder().icon_name("dialog-error-symbolic").title("Failed to load table").description(err.to_string()).build());
            }
            Err(_) => {}
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn populate(
    root: &gtk::Box,
    conn_id: String,
    schema: String,
    table: String,
    detail: TableDetail,
    ddl: String,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_open_table: Rc<dyn Fn(String, String, String)>,
) {
    let scroller = gtk::ScrolledWindow::builder().vexpand(true).build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let root_owned = root.clone();
    let header_box = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(12).build();
    let header = adw::PreferencesGroup::builder()
        .title(format!("{schema}.{table}"))
        .description(format!("~{} rows (estimate)", detail.row_estimate))
        .hexpand(true)
        .build();
    let edit_btn = gtk::Button::builder().label("Edit").valign(gtk::Align::Center).build();
    edit_btn.connect_clicked(clone!(
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        table,
        #[strong]
        detail,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        on_open_table,
        #[strong]
        root_owned,
        move |btn| {
            let Some(parent) = btn.root() else { return };
            table_editor::open(
                &parent,
                conn_id.clone(),
                schema.clone(),
                table.clone(),
                detail.clone(),
                runtime.clone(),
                manager.clone(),
                clone!(
                    #[strong]
                    conn_id,
                    #[strong]
                    schema,
                    #[strong]
                    table,
                    #[strong]
                    runtime,
                    #[strong]
                    manager,
                    #[strong]
                    on_open_table,
                    #[strong]
                    root_owned,
                    move || load(root_owned.clone(), conn_id.clone(), schema.clone(), table.clone(), runtime.clone(), manager.clone(), on_open_table.clone())
                ),
            );
        }
    ));
    let maintenance_status = gtk::Label::builder().wrap(true).xalign(0.0).visible(false).build();

    let maintenance_btn = gtk::MenuButton::builder()
        .icon_name("org.gnome.Settings-symbolic")
        .tooltip_text("Maintenance (VACUUM / ANALYZE)")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let maintenance_popover = gtk::Popover::new();
    let maintenance_popover_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).build();
    for op in VACUUM_OPS {
        let op_btn = gtk::Button::builder().label(*op).css_classes(["flat"]).build();
        maintenance_popover_box.append(&op_btn);
        op_btn.connect_clicked(clone!(
            #[weak]
            maintenance_popover,
            #[strong]
            conn_id,
            #[strong]
            schema,
            #[strong]
            table,
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            maintenance_status,
            move |op_btn| {
                maintenance_popover.popdown();

                let run = {
                    let conn_id = conn_id.clone();
                    let schema = schema.clone();
                    let table = table.clone();
                    let runtime = runtime.clone();
                    let manager = manager.clone();
                    let maintenance_status = maintenance_status.clone();
                    move || {
                        maintenance_status.remove_css_class("error");
                        maintenance_status.set_label(&format!("Running {op}…"));
                        maintenance_status.set_visible(true);

                        let task_schema = schema.clone();
                        let task_table = table.clone();
                        let task_conn_id = conn_id.clone();
                        let task_manager = manager.clone();
                        let handle = runtime.spawn(async move {
                            let mut mgr = task_manager.lock().await;
                            crate::connection_runtime::ensure_connected(&mut mgr, &task_conn_id).await?;
                            let driver = mgr.get_driver(&task_conn_id).ok_or(CoreError::NotConnected)?;
                            queries::run_vacuum(driver, &task_schema, &task_table, op).await
                        });

                        let maintenance_status = maintenance_status.clone();
                        glib::MainContext::default().spawn_local(async move {
                            match handle.await {
                                Ok(Ok(())) => maintenance_status.set_label(&format!("{op} completed on \"{schema}\".\"{table}\".")),
                                Ok(Err(err)) => {
                                    maintenance_status.add_css_class("error");
                                    maintenance_status.set_label(&format!("{op} failed: {err}"));
                                }
                                Err(_) => {}
                            }
                        });
                    }
                };

                if *op == "VACUUM FULL" {
                    let Some(parent) = op_btn.root() else { return };
                    confirm_destructive(
                        &parent,
                        "Run VACUUM FULL?",
                        &format!(
                            "VACUUM FULL rewrites \"{schema}\".\"{table}\" on disk and holds an ACCESS EXCLUSIVE lock for the \
                             duration — every other read or write against this table blocks until it finishes."
                        ),
                        "Run VACUUM FULL",
                        run,
                    );
                } else {
                    run();
                }
            }
        ));
    }
    maintenance_popover.set_child(Some(&maintenance_popover_box));
    maintenance_btn.set_popover(Some(&maintenance_popover));

    header_box.append(&header);
    header_box.append(&maintenance_btn);
    header_box.append(&edit_btn);
    content.append(&header_box);
    content.append(&maintenance_status);

    content.append(&section_label("Columns"));
    content.append(&columns_group(&detail.columns));

    if !detail.indexes.is_empty() {
        content.append(&section_label("Indexes"));
        content.append(&indexes_group(&detail.indexes));
    }

    if !detail.constraints.is_empty() {
        content.append(&section_label("Constraints"));
        content.append(&constraints_group(&detail.constraints));
    }

    if !detail.fk_map.is_empty() {
        content.append(&section_label("Foreign Keys"));
        content.append(&fk_map_group(&detail.fk_map, &conn_id, &on_open_table));
    }

    scroller.set_child(Some(&content));
    root.append(&scroller);

    // Pinned below the scrollable columns/indexes/constraints/FKs (not inside `scroller`) so the
    // DDL stays visible at the bottom instead of pushing those objects further down the page.
    let ddl_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).margin_top(8).margin_bottom(12).margin_start(12).margin_end(12).build();
    ddl_box.append(&section_label("DDL"));
    ddl_box.append(&ddl_view(&ddl));
    root.append(&ddl_box);
}

fn section_label(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).css_classes(["heading"]).build()
}

fn ddl_view(ddl: &str) -> gtk::Widget {
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_text(ddl);
    if let Some(lang) = sourceview5::LanguageManager::default().language("sql") {
        buffer.set_language(Some(&lang));
    }
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_monospace(true);
    view.set_editable(false);
    view.set_top_margin(6);
    view.set_left_margin(6);
    view.set_bottom_margin(6);
    let scroller = gtk::ScrolledWindow::builder().child(&view).min_content_height(160).build();
    scroller.upcast()
}

fn columns_group(columns: &[TableDetailColumn]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    for c in columns {
        let mut badges = Vec::new();
        if c.is_primary_key {
            badges.push("PK");
        }
        if c.is_foreign_key {
            badges.push("FK");
        }
        if !c.is_nullable {
            badges.push("NOT NULL");
        }
        let mut subtitle = c.full_type.clone();
        if let Some(default) = &c.column_default {
            subtitle.push_str(&format!(" · default {default}"));
        }
        if !badges.is_empty() {
            subtitle.push_str(&format!(" · {}", badges.join(", ")));
        }
        let row = adw::ActionRow::builder().title(glib::markup_escape_text(&c.name)).subtitle(subtitle).build();
        group.add(&row);
    }
    group
}

fn indexes_group(indexes: &[IndexInfo]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    for idx in indexes {
        let mut badges = Vec::new();
        if idx.is_primary {
            badges.push("PRIMARY");
        } else if idx.is_unique {
            badges.push("UNIQUE");
        }
        let subtitle = if badges.is_empty() { idx.definition.clone() } else { format!("{} · {}", badges.join(", "), idx.definition) };
        let row = adw::ActionRow::builder().title(glib::markup_escape_text(&idx.name)).subtitle(subtitle).build();
        row.add_suffix(&gtk::Label::builder().label(&idx.size).css_classes(["dim-label"]).build());
        group.add(&row);
    }
    group
}

fn constraints_group(constraints: &[ConstraintInfo]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    for c in constraints {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&c.name))
            .subtitle(format!("{} · {}", c.kind, c.definition))
            .build();
        group.add(&row);
    }
    group
}

fn fk_map_group(fk_map: &[FkMapEntry], conn_id: &str, on_open_table: &Rc<dyn Fn(String, String, String)>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    for fk in fk_map {
        let (title, subtitle) = match fk.direction {
            FkDirection::Outgoing => (
                format!("{} → {}.{}", fk.column, fk.foreign_table, fk.foreign_column),
                format!("outgoing · {}", fk.constraint_name),
            ),
            FkDirection::Incoming => (
                format!("{}.{} → {}", fk.foreign_table, fk.foreign_column, fk.column),
                format!("incoming · {}", fk.constraint_name),
            ),
        };
        let target_schema = fk.foreign_schema.clone();
        let target_table = fk.foreign_table.clone();
        let conn_id = conn_id.to_string();
        let on_open_table = on_open_table.clone();
        let row = adw::ActionRow::builder().title(title).subtitle(subtitle).activatable(true).build();
        row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        row.connect_activated(move |_| (on_open_table)(conn_id.clone(), target_schema.clone(), target_table.clone()));
        group.add(&row);
    }
    group
}
