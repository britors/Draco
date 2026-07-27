//! Function editor tab: GtkSourceView5 editor for `CREATE OR REPLACE FUNCTION` DDL, with
//! Save/Validate/Test — same three actions as the old Monaco-based editor, backed by
//! `draco-core::postgres::queries::{save_function, validate_function, call_function}`.

use std::sync::Arc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries;
use draco_core::secrets;
use gtk::glib;
use sourceview5::prelude::*;
use tokio::sync::Mutex;

use crate::results_grid::ResultsGrid;

type SharedManager = Arc<Mutex<ConnectionManager>>;

const NEW_FUNCTION_TEMPLATE: &str = "CREATE OR REPLACE FUNCTION public.my_function()\nRETURNS void\nLANGUAGE plpgsql\nAS $$\nBEGIN\n  -- body\nEND;\n$$;\n";

pub struct FunctionEditor {
    root: gtk::Box,
}

impl FunctionEditor {
    /// `existing_name` is `None` when creating a brand new function.
    pub fn new(
        conn_id: String,
        schema: String,
        existing_name: Option<String>,
        runtime: tokio::runtime::Handle,
        manager: SharedManager,
    ) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();

        let buffer = sourceview5::Buffer::new(None);
        if let Some(lang) = sourceview5::LanguageManager::default().language("sql") {
            buffer.set_language(Some(&lang));
        }
        buffer.set_text(NEW_FUNCTION_TEMPLATE);

        let view = sourceview5::View::with_buffer(&buffer);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        view.set_highlight_current_line(true);
        view.set_top_margin(6);
        view.set_left_margin(6);
        view.set_bottom_margin(6);
        let editor_scroller = gtk::ScrolledWindow::builder().child(&view).vexpand(true).build();

        let save_btn = gtk::Button::builder().label("Save").tooltip_text("Ctrl+S").css_classes(["suggested-action"]).build();
        let validate_btn = gtk::Button::builder().label("Validate").build();
        let test_btn = gtk::Button::builder().label("Test").build();
        let test_sql_entry = gtk::Entry::builder().placeholder_text("SELECT schema.func(args)").hexpand(true).build();
        let status_label = gtk::Label::builder().xalign(0.0).css_classes(["dim-label"]).build();

        let toolbar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        toolbar.append(&save_btn);
        toolbar.append(&validate_btn);
        toolbar.append(&test_btn);
        toolbar.append(&test_sql_entry);

        let status_row = gtk::Box::builder().margin_start(6).margin_bottom(6).build();
        status_row.append(&status_label);

        let results = std::rc::Rc::new(ResultsGrid::new());
        let results_scroller = gtk::ScrolledWindow::builder().child(results.widget()).min_content_height(160).build();

        root.append(&toolbar);
        root.append(&status_row);
        root.append(&editor_scroller);
        root.append(&results_scroller);

        // Load existing DDL, if editing rather than creating.
        if let Some(func_name) = existing_name.clone() {
            let task_id = conn_id.clone();
            let task_schema = schema.clone();
            let task_manager = manager.clone();
            let handle = runtime.spawn(async move {
                let mgr = task_manager.lock().await;
                let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
                queries::get_function_ddl(driver, &task_schema, &func_name).await
            });
            let buffer_for_task = buffer.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(Ok(overloads)) = handle.await {
                    if let Some(first) = overloads.first() {
                        buffer_for_task.set_text(&first.ddl);
                    }
                }
            });
        }

        save_btn.connect_clicked(clone_run(
            buffer.clone(),
            conn_id.clone(),
            runtime.clone(),
            manager.clone(),
            status_label.clone(),
            RunKind::Save,
        ));
        validate_btn.connect_clicked(clone_run(
            buffer.clone(),
            conn_id.clone(),
            runtime.clone(),
            manager.clone(),
            status_label.clone(),
            RunKind::Validate,
        ));

        test_btn.connect_clicked(move |btn| {
            let sql = test_sql_entry.text().to_string();
            if sql.trim().is_empty() {
                status_label.set_label("Enter a SQL expression to test");
                return;
            }
            btn.set_sensitive(false);
            let task_id = conn_id.clone();
            let task_manager = manager.clone();
            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                if mgr.get_driver(&task_id).is_none() {
                    let password = secrets::get_password(&task_id).await.unwrap_or_default();
                    mgr.connect(&task_id, &password, 30_000, None, None).await?;
                }
                let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
                queries::call_function(driver, &sql).await
            });
            let results_for_task = results.clone();
            let status_label_for_task = status_label.clone();
            let btn_for_task = btn.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok(result)) => {
                        status_label_for_task.set_label(&format!("{} row(s)", result.rows.len()));
                        results_for_task.set_data(&result.columns, result.rows);
                    }
                    Ok(Err(err)) => status_label_for_task.set_label(&format!("Error: {err}")),
                    Err(_) => {}
                }
                btn_for_task.set_sensitive(true);
            });
        });

        Self { root }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

enum RunKind {
    Save,
    Validate,
}

fn clone_run(
    buffer: sourceview5::Buffer,
    conn_id: String,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    status_label: gtk::Label,
    kind: RunKind,
) -> impl Fn(&gtk::Button) {
    move |btn: &gtk::Button| {
        let (start, end) = buffer.bounds();
        let ddl = buffer.text(&start, &end, false).to_string();
        if ddl.trim().is_empty() {
            return;
        }

        btn.set_sensitive(false);
        status_label.set_label(match kind {
            RunKind::Save => "Saving…",
            RunKind::Validate => "Validating…",
        });

        let task_id = conn_id.clone();
        let task_manager = manager.clone();
        let task_kind_is_save = matches!(kind, RunKind::Save);
        let handle = runtime.spawn(async move {
            let mut mgr = task_manager.lock().await;
            if mgr.get_driver(&task_id).is_none() {
                let password = secrets::get_password(&task_id).await.unwrap_or_default();
                mgr.connect(&task_id, &password, 30_000, None, None).await?;
            }
            let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
            if task_kind_is_save {
                queries::save_function(driver, &ddl).await?;
                Ok::<Option<String>, CoreError>(None)
            } else {
                Ok(queries::validate_function(driver, &ddl).await?)
            }
        });

        let status_label_for_task = status_label.clone();
        let btn_for_task = btn.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(None)) => status_label_for_task.set_label("Saved"),
                Ok(Ok(Some(err))) => status_label_for_task.set_label(&format!("Invalid: {err}")),
                Ok(Err(err)) => status_label_for_task.set_label(&format!("Error: {err}")),
                Err(_) => {}
            }
            btn_for_task.set_sensitive(true);
        });
    }
}
