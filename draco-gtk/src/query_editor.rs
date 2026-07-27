//! SQL query editor tab: `GtkSourceView5` (syntax highlighting, no Monaco/CDN) + Run button +
//! results grid, bound to one connection picked from a dropdown.

use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::connection::DbConnection;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries;
use draco_core::secrets;
use gtk::glib;
use sourceview5::prelude::*;
use tokio::sync::Mutex;

use crate::results_grid::ResultsGrid;

type SharedManager = Arc<Mutex<ConnectionManager>>;

pub struct QueryEditor {
    root: gtk::Box,
}

impl QueryEditor {
    pub fn new(connections: Vec<DbConnection>, runtime: tokio::runtime::Handle, manager: SharedManager) -> Self {
        let buffer = sourceview5::Buffer::new(None);
        if let Some(lang) = sourceview5::LanguageManager::default().language("sql") {
            buffer.set_language(Some(&lang));
        }
        let scheme_name = if adw::StyleManager::default().is_dark() { "Adwaita-dark" } else { "Adwaita" };
        if let Some(scheme) = sourceview5::StyleSchemeManager::default().scheme(scheme_name) {
            buffer.set_style_scheme(Some(&scheme));
        }

        let view = sourceview5::View::with_buffer(&buffer);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        view.set_highlight_current_line(true);
        view.set_top_margin(6);
        view.set_left_margin(6);
        view.set_bottom_margin(6);
        let editor_scroller = gtk::ScrolledWindow::builder().child(&view).vexpand(true).build();

        let labels: Vec<&str> = connections.iter().map(|c| c.label.as_str()).collect();
        let conn_model = gtk::StringList::new(&labels);
        let conn_dropdown = gtk::DropDown::builder().model(&conn_model).build();

        let run_btn = gtk::Button::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Run query")
            .css_classes(["suggested-action"])
            .build();
        let status_label = gtk::Label::builder().xalign(0.0).css_classes(["dim-label"]).hexpand(true).build();

        let toolbar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        toolbar.append(&conn_dropdown);
        toolbar.append(&run_btn);
        toolbar.append(&status_label);

        let results = Rc::new(ResultsGrid::new());
        let results_scroller = gtk::ScrolledWindow::builder().child(results.widget()).vexpand(true).build();

        let paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .start_child(&editor_scroller)
            .end_child(&results_scroller)
            .resize_start_child(true)
            .resize_end_child(true)
            .position(280)
            .build();

        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        root.append(&toolbar);
        root.append(&paned);

        run_btn.connect_clicked(move |btn| {
            let selected = conn_dropdown.selected();
            let Some(conn) = connections.get(selected as usize) else {
                status_label.set_label("No connection selected");
                return;
            };
            let (start, end) = buffer.bounds();
            let sql = buffer.text(&start, &end, false).to_string();
            if sql.trim().is_empty() {
                return;
            }

            btn.set_sensitive(false);
            status_label.set_label("Running…");

            let conn_id = conn.id.clone();
            let task_manager = manager.clone();
            let started = std::time::Instant::now();
            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                if mgr.get_driver(&conn_id).is_none() {
                    let password = secrets::get_password(&conn_id).await.unwrap_or_default();
                    mgr.connect(&conn_id, &password, 30_000, None, None).await?;
                }
                let driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
                queries::execute_query(driver, &sql).await
            });

            let results_for_task = results.clone();
            let status_label_for_task = status_label.clone();
            let run_btn_for_task = btn.clone();
            glib::MainContext::default().spawn_local(async move {
                let elapsed = started.elapsed();
                match handle.await {
                    Ok(Ok(result)) => {
                        let row_count = result.rows.len();
                        results_for_task.set_data(&result.columns, result.rows);
                        status_label_for_task.set_label(&format!("{row_count} rows in {:.2}s", elapsed.as_secs_f64()));
                    }
                    Ok(Err(err)) => {
                        results_for_task.clear();
                        status_label_for_task.set_label(&format!("Error: {err}"));
                    }
                    Err(_) => {
                        status_label_for_task.set_label("Cancelled");
                    }
                }
                run_btn_for_task.set_sensitive(true);
            });
        });

        Self { root }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
