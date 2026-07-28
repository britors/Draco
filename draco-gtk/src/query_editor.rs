//! SQL query editor tab: `GtkSourceView5` (syntax highlighting, no Monaco/CDN) + Run button +
//! results grid, bound to one connection picked from a dropdown. Also drives query history
//! (auto-recorded, capped at 50 by `draco-core::store`) and named snippets — both local TOML,
//! read/written synchronously on the GTK thread like every other `store::*` call in this app.

use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::connection::DbConnection;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries;
use draco_core::secrets;
use draco_core::store;
use gtk::glib;
use gtk::glib::clone;
use sourceview5::prelude::*;
use tokio::sync::Mutex;

use crate::results_grid::ResultsGrid;

type SharedManager = Arc<Mutex<ConnectionManager>>;

fn now_millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn first_line(sql: &str) -> String {
    let line = sql.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(80).collect::<String>())
    } else {
        line.to_string()
    }
}

/// Rebuilt from scratch every time the popover opens (`connect_show`) — same "wholesale replace"
/// philosophy as `Explorer::set_connections`/`admin::Section::set_rows`, and cheap here since
/// `store::list_history`/`list_snippets` are just local TOML reads.
fn build_history_content(buffer: &sourceview5::Buffer, popover: &gtk::Popover) -> gtk::Widget {
    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).width_request(420).build();

    let clear_btn = gtk::Button::builder().label("Clear History").halign(gtk::Align::Start).css_classes(["flat"]).build();
    clear_btn.connect_clicked(clone!(
        #[strong]
        buffer,
        #[weak]
        popover,
        move |_| {
            let _ = store::clear_history();
            popover.set_child(Some(&build_history_content(&buffer, &popover)));
        }
    ));
    content.append(&clear_btn);

    let entries = store::list_history();
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    if entries.is_empty() {
        list.append(&adw::ActionRow::builder().title("No history yet").build());
    }
    for entry in &entries {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&first_line(&entry.sql)))
            .subtitle(format!("{} · {} rows · {}ms", entry.conn_label, entry.row_count, entry.duration_ms))
            .activatable(true)
            .build();
        let sql = entry.sql.clone();
        row.connect_activated(clone!(
            #[strong]
            buffer,
            #[weak]
            popover,
            move |_| {
                buffer.set_text(&sql);
                popover.popdown();
            }
        ));

        let delete_btn = gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Delete entry").valign(gtk::Align::Center).css_classes(["flat"]).build();
        row.add_suffix(&delete_btn);
        let id = entry.id.clone();
        delete_btn.connect_clicked(clone!(
            #[strong]
            buffer,
            #[weak]
            popover,
            move |_| {
                let _ = store::delete_history_entry(&id);
                popover.set_child(Some(&build_history_content(&buffer, &popover)));
            }
        ));
        list.append(&row);
    }
    let scroller = gtk::ScrolledWindow::builder().child(&list).max_content_height(360).propagate_natural_height(true).build();
    content.append(&scroller);

    content.upcast()
}

fn build_snippets_content(buffer: &sourceview5::Buffer, popover: &gtk::Popover, connections: Rc<Vec<DbConnection>>, conn_dropdown: &gtk::DropDown) -> gtk::Widget {
    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).width_request(420).build();

    let name_row = adw::EntryRow::builder().title("Snippet name").build();
    content.append(&name_row);
    let save_btn = gtk::Button::builder().label("Save Current Query").halign(gtk::Align::Start).css_classes(["flat"]).build();
    content.append(&save_btn);
    save_btn.connect_clicked(clone!(
        #[strong]
        buffer,
        #[strong]
        name_row,
        #[weak]
        popover,
        #[strong]
        connections,
        #[strong]
        conn_dropdown,
        move |_| {
            let name = name_row.text().to_string();
            let (start, end) = buffer.bounds();
            let sql = buffer.text(&start, &end, false).to_string();
            if name.trim().is_empty() || sql.trim().is_empty() {
                return;
            }
            let conn = connections.get(conn_dropdown.selected() as usize);
            let _ = store::save_snippet(store::Snippet {
                id: String::new(),
                name: name.trim().to_string(),
                sql,
                created_at: 0,
                conn_id: conn.map(|c| c.id.clone()),
                conn_label: conn.map(|c| c.label.clone()),
            });
            popover.popdown();
        }
    ));

    let snippets = store::list_snippets();
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    if snippets.is_empty() {
        list.append(&adw::ActionRow::builder().title("No snippets yet").build());
    }
    for s in &snippets {
        let subtitle = match &s.conn_label {
            Some(label) => format!("{label} · {}", first_line(&s.sql)),
            None => first_line(&s.sql),
        };
        let row = adw::ActionRow::builder().title(glib::markup_escape_text(&s.name)).subtitle(glib::markup_escape_text(&subtitle)).activatable(true).build();
        let sql = s.sql.clone();
        row.connect_activated(clone!(
            #[strong]
            buffer,
            #[weak]
            popover,
            move |_| {
                buffer.set_text(&sql);
                popover.popdown();
            }
        ));

        let delete_btn = gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Delete snippet").valign(gtk::Align::Center).css_classes(["flat"]).build();
        row.add_suffix(&delete_btn);
        let id = s.id.clone();
        delete_btn.connect_clicked(clone!(
            #[strong]
            buffer,
            #[weak]
            popover,
            #[strong]
            connections,
            #[strong]
            conn_dropdown,
            move |_| {
                let _ = store::delete_snippet(&id);
                popover.set_child(Some(&build_snippets_content(&buffer, &popover, connections.clone(), &conn_dropdown)));
            }
        ));
        list.append(&row);
    }
    let scroller = gtk::ScrolledWindow::builder().child(&list).max_content_height(360).propagate_natural_height(true).build();
    content.append(&scroller);

    content.upcast()
}

/// Runs `sql` against `conn`, showing the result in `results`/`status_label` — shared by the
/// plain Run action and the `EXPLAIN`-wrapped one, so only the latter's history bookkeeping
/// (`record_history`) differs.
#[allow(clippy::too_many_arguments)]
fn run_sql(
    sql: String,
    conn: &DbConnection,
    record_history: bool,
    runtime: &tokio::runtime::Handle,
    manager: &SharedManager,
    results: &Rc<ResultsGrid>,
    status_label: &gtk::Label,
    run_btn: &gtk::Button,
    explain_btn: &gtk::Button,
) {
    run_btn.set_sensitive(false);
    explain_btn.set_sensitive(false);
    status_label.set_label("Running…");

    let conn_id = conn.id.clone();
    let conn_label = conn.label.clone();
    let task_manager = manager.clone();
    let sql_for_task = sql.clone();
    let started = std::time::Instant::now();
    let handle = runtime.spawn(async move {
        let mut mgr = task_manager.lock().await;
        if mgr.get_driver(&conn_id).is_none() {
            let password = secrets::get_password(&conn_id).await.unwrap_or_default();
            mgr.connect(&conn_id, &password, 30_000, None, None).await?;
        }
        let driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
        queries::execute_query(driver, &sql_for_task).await
    });

    let conn_id_for_history = conn.id.clone();
    let results_for_task = results.clone();
    let status_label_for_task = status_label.clone();
    let run_btn_for_task = run_btn.clone();
    let explain_btn_for_task = explain_btn.clone();
    glib::MainContext::default().spawn_local(async move {
        let elapsed = started.elapsed();
        match handle.await {
            Ok(Ok(result)) => {
                let row_count = result.rows.len();
                results_for_task.set_data(&result.columns, result.rows);
                status_label_for_task.set_label(&format!("{row_count} rows in {:.2}s", elapsed.as_secs_f64()));
                if record_history {
                    let _ = store::add_history(store::HistoryEntry {
                        id: String::new(),
                        sql,
                        conn_id: conn_id_for_history,
                        conn_label,
                        timestamp: now_millis(),
                        duration_ms: elapsed.as_millis() as i64,
                        row_count: row_count as i64,
                    });
                }
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
        explain_btn_for_task.set_sensitive(true);
    });
}

pub struct QueryEditor {
    root: gtk::Box,
    run_action: Rc<dyn Fn(bool)>,
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

        let connections: Rc<Vec<DbConnection>> = Rc::new(connections);

        let labels: Vec<&str> = connections.iter().map(|c| c.label.as_str()).collect();
        let conn_model = gtk::StringList::new(&labels);
        let conn_dropdown = gtk::DropDown::builder().model(&conn_model).build();

        let run_btn = gtk::Button::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Run query (F8)")
            .css_classes(["suggested-action"])
            .build();

        let explain_btn = gtk::Button::builder().icon_name("view-list-symbolic").tooltip_text("Explain plan (F10)").css_classes(["flat"]).build();

        let history_popover = gtk::Popover::new();
        history_popover.connect_show(clone!(
            #[strong]
            buffer,
            move |popover| popover.set_child(Some(&build_history_content(&buffer, popover)))
        ));
        let history_btn = gtk::MenuButton::builder().icon_name("document-open-recent-symbolic").tooltip_text("Query History").css_classes(["flat"]).popover(&history_popover).build();

        let snippets_popover = gtk::Popover::new();
        snippets_popover.connect_show(clone!(
            #[strong]
            buffer,
            #[strong]
            connections,
            #[strong]
            conn_dropdown,
            move |popover| popover.set_child(Some(&build_snippets_content(&buffer, popover, connections.clone(), &conn_dropdown)))
        ));
        let snippets_btn = gtk::MenuButton::builder().icon_name("user-bookmarks-symbolic").tooltip_text("Snippets").css_classes(["flat"]).popover(&snippets_popover).build();

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
        toolbar.append(&explain_btn);
        toolbar.append(&history_btn);
        toolbar.append(&snippets_btn);
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

        // Shared by the Run button, the Explain button and the F8/F10 shortcuts below — `explain`
        // wraps the buffer's SQL in a plain `EXPLAIN` (never `ANALYZE`, which would actually
        // execute — and side-effect — a DML statement) and skips history, since it's a
        // meta-query about the real one, not a query a user would want to re-run from history.
        let do_run: Rc<dyn Fn(bool)> = Rc::new(clone!(
            #[strong]
            conn_dropdown,
            #[strong]
            connections,
            #[strong]
            buffer,
            #[strong]
            status_label,
            #[strong]
            results,
            #[strong]
            run_btn,
            #[strong]
            explain_btn,
            #[strong]
            runtime,
            #[strong]
            manager,
            move |explain: bool| {
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
                let sql = if explain { format!("EXPLAIN {sql}") } else { sql };
                run_sql(sql, conn, !explain, &runtime, &manager, &results, &status_label, &run_btn, &explain_btn);
            }
        ));

        run_btn.connect_clicked(clone!(
            #[strong]
            do_run,
            move |_| do_run(false)
        ));
        explain_btn.connect_clicked(clone!(
            #[strong]
            do_run,
            move |_| do_run(true)
        ));

        Self { root, run_action: do_run }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// `Fn(bool)` — `false` runs the buffer as-is, `true` wraps it in `EXPLAIN` first. Exposed so
    /// `window_main.rs` can wire the window-level `F8`/`F10` accelerators (the same
    /// `win.<action>` + `set_accels_for_action` mechanism already used for `Ctrl+P`/`Ctrl+T`) to
    /// whichever query tab is currently selected — a widget-local `EventControllerKey` on `root`
    /// turned out not to reliably receive `F8`/`F10` while the `GtkSourceView` had focus.
    pub fn run_action(&self) -> Rc<dyn Fn(bool)> {
        self.run_action.clone()
    }
}
