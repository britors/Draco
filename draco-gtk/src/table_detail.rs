//! Table detail tab: DDL, columns, indexes, constraints and FK map for one table — opened from
//! the Explorer's "View details" button on a table row.

use std::sync::Arc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, ConstraintInfo, FkDirection, FkMapEntry, IndexInfo, TableDetail, TableDetailColumn};
use gtk::glib;
use sourceview5::prelude::*;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

pub struct TableDetailView {
    root: gtk::Box,
}

impl TableDetailView {
    pub fn new(conn_id: String, schema: String, table: String, runtime: tokio::runtime::Handle, manager: SharedManager) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();

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
                Ok(Ok((detail, ddl))) => populate(&root_for_task, &schema, &table, detail, ddl),
                Ok(Err(err)) => {
                    root_for_task.append(&adw::StatusPage::builder().icon_name("dialog-error-symbolic").title("Failed to load table").description(err.to_string()).build());
                }
                Err(_) => {}
            }
        });

        Self { root }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

fn populate(root: &gtk::Box, schema: &str, table: &str, detail: TableDetail, ddl: String) {
    let scroller = gtk::ScrolledWindow::builder().vexpand(true).build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let header = adw::PreferencesGroup::builder()
        .title(format!("{schema}.{table}"))
        .description(format!("~{} rows (estimate)", detail.row_estimate))
        .build();
    content.append(&header);

    content.append(&section_label("DDL"));
    content.append(&ddl_view(&ddl));

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
        content.append(&fk_map_group(&detail.fk_map));
    }

    scroller.set_child(Some(&content));
    root.append(&scroller);
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

fn fk_map_group(fk_map: &[FkMapEntry]) -> adw::PreferencesGroup {
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
        let row = adw::ActionRow::builder().title(title).subtitle(subtitle).build();
        group.add(&row);
    }
    group
}
