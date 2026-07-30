//! SQL query editor tab: `GtkSourceView5` (syntax highlighting, no Monaco/CDN) + Run button +
//! results grid, bound to one connection picked from a dropdown. Also drives query history
//! (auto-recorded, capped at 50 by `draco-core::store`) and named snippets — both local TOML,
//! read/written synchronously on the GTK thread like every other `store::*` call in this app.

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::connection::DbConnection;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries;
use draco_core::store;
use gtk::{gio, glib};
use gtk::glib::clone;
use sourceview5::prelude::*;
use tokio::sync::{watch, Mutex};

use crate::results_grid::{ExportSnapshot, ResultsGrid};

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

async fn wait_for_cancel(mut receiver: watch::Receiver<bool>) {
    while !*receiver.borrow() {
        if receiver.changed().await.is_err() {
            return;
        }
    }
}

fn refresh_completion_words(
    conn: Option<DbConnection>,
    runtime: &tokio::runtime::Handle,
    manager: &SharedManager,
    completion_buffer: &sourceview5::Buffer,
) {
    let Some(conn) = conn else {
        completion_buffer.set_text("");
        return;
    };
    let conn_id = conn.id.clone();
    let task_manager = manager.clone();
    let handle = runtime.spawn(async move {
        let mgr = task_manager.lock().await;
        let driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
        queries::get_completion_data(driver).await
    });
    let completion_buffer = completion_buffer.clone();
    glib::MainContext::default().spawn_local(async move {
        let Ok(Ok(data)) = handle.await else {
            return;
        };
        let mut words = String::new();
        for schema in data.schemas {
            words.push_str(&schema);
            words.push(' ');
        }
        for table in data.tables {
            words.push_str(&table.name);
            words.push(' ');
            words.push_str(&table.schema);
            words.push(' ');
            words.push_str(&format!("{}.{}", table.schema, table.name));
            words.push(' ');
        }
        for column in data.columns {
            words.push_str(&column.name);
            words.push(' ');
            words.push_str(&format!("{}.{}", column.table, column.name));
            words.push(' ');
            words.push_str(&format!("{}.{}.{}", column.schema, column.table, column.name));
            words.push(' ');
        }
        for function in data.functions {
            words.push_str(&function.name);
            words.push(' ');
            words.push_str(&function.schema);
            words.push(' ');
            words.push_str(&format!("{}.{}", function.schema, function.name));
            words.push(' ');
        }
        completion_buffer.set_text(&words);
    });
}

#[derive(Clone, Copy)]
enum ExportFormat {
    Csv,
    Json,
}

fn csv_field(value: &serde_json::Value) -> String {
    let raw = match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    };
    if raw.contains(',') || raw.contains('"') || raw.contains('\n') || raw.contains('\r') {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw
    }
}

fn write_export(path: PathBuf, snapshot: ExportSnapshot, format: ExportFormat) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    match format {
        ExportFormat::Csv => {
            let header = snapshot.columns.iter().map(|column| csv_field(&serde_json::Value::String(column.clone()))).collect::<Vec<_>>().join(",");
            writeln!(file, "{header}")?;
            for row in snapshot.rows {
                let line = snapshot.columns.iter().map(|column| csv_field(row.get(column).unwrap_or(&serde_json::Value::Null))).collect::<Vec<_>>().join(",");
                writeln!(file, "{line}")?;
            }
        }
        ExportFormat::Json => {
            serde_json::to_writer_pretty(&mut file, &snapshot.rows).map_err(std::io::Error::other)?;
            writeln!(file)?;
        }
    }
    Ok(())
}

fn export_results(
    results: &Rc<ResultsGrid>,
    format: ExportFormat,
    runtime: &tokio::runtime::Handle,
    toasts: &adw::ToastOverlay,
    source_btn: &gtk::Button,
) {
    if !results.has_rows() {
        toasts.add_toast(adw::Toast::new("No results to export"));
        return;
    }
    let Some(window) = source_btn.root().and_downcast::<gtk::Window>() else {
        toasts.add_toast(adw::Toast::new("Cannot open export dialog"));
        return;
    };
    let (title, accept_label, initial_name) = match format {
        ExportFormat::Csv => ("Export results as CSV", "Export", "query-results.csv"),
        ExportFormat::Json => ("Export results as JSON", "Export", "query-results.json"),
    };
    let dialog = gtk::FileDialog::builder().title(title).accept_label(accept_label).initial_name(initial_name).build();
    let snapshot = results.export_snapshot();
    let runtime = runtime.clone();
    let toasts = toasts.clone();
    dialog.save(Some(&window), None::<&gio::Cancellable>, move |result| {
        // User dismissed the file chooser — a normal no-op, not worth a toast.
        let Ok(file) = result else {
            return;
        };
        let Some(path) = file.path() else {
            toasts.add_toast(adw::Toast::new("Export failed: invalid destination"));
            return;
        };
        let handle = runtime.spawn_blocking(move || write_export(path, snapshot, format));
        let toasts = toasts.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(())) => toasts.add_toast(adw::Toast::new("Results exported successfully")),
                Ok(Err(err)) => toasts.add_toast(adw::Toast::new(&format!("Export failed: {err}"))),
                Err(err) => toasts.add_toast(adw::Toast::new(&format!("Export failed: {err}"))),
            }
        });
    });
}

/// Opens a `.sql` file and replaces the editor buffer's text with its contents — the read side
/// of `save_sql_to_file`, same `FileDialog` shape but `.open` instead of `.save`.
fn open_sql_file(buffer: &sourceview5::Buffer, runtime: &tokio::runtime::Handle, toasts: &adw::ToastOverlay, source_btn: &gtk::Button) {
    let Some(window) = source_btn.root().and_downcast::<gtk::Window>() else {
        toasts.add_toast(adw::Toast::new("Cannot open file dialog"));
        return;
    };
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("SQL files"));
    filter.add_suffix("sql");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder().title("Open SQL file").accept_label("Open").filters(&filters).build();
    let buffer = buffer.clone();
    let runtime = runtime.clone();
    let toasts = toasts.clone();
    dialog.open(Some(&window), None::<&gio::Cancellable>, move |result| {
        // User dismissed the file chooser — a normal no-op, not worth a toast.
        let Ok(file) = result else {
            return;
        };
        let Some(path) = file.path() else {
            toasts.add_toast(adw::Toast::new("Open failed: invalid file"));
            return;
        };
        let handle = runtime.spawn_blocking(move || std::fs::read_to_string(path));
        let toasts = toasts.clone();
        let buffer = buffer.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(content)) => buffer.set_text(&content),
                Ok(Err(err)) => toasts.add_toast(adw::Toast::new(&format!("Open failed: {err}"))),
                Err(err) => toasts.add_toast(adw::Toast::new(&format!("Open failed: {err}"))),
            }
        });
    });
}

/// Saves the editor buffer's raw text to a `.sql` file — same `FileDialog::save` shape as
/// `export_results`, minus the CSV/JSON formatting since this just writes the SQL as typed.
fn save_sql_to_file(sql: String, runtime: &tokio::runtime::Handle, toasts: &adw::ToastOverlay, source_btn: &gtk::Button) {
    if sql.trim().is_empty() {
        toasts.add_toast(adw::Toast::new("Nothing to save — the buffer is empty"));
        return;
    }
    let Some(window) = source_btn.root().and_downcast::<gtk::Window>() else {
        toasts.add_toast(adw::Toast::new("Cannot open save dialog"));
        return;
    };
    let dialog = gtk::FileDialog::builder().title("Save query as SQL").accept_label("Save").initial_name("query.sql").build();
    let runtime = runtime.clone();
    let toasts = toasts.clone();
    dialog.save(Some(&window), None::<&gio::Cancellable>, move |result| {
        // User dismissed the file chooser — a normal no-op, not worth a toast.
        let Ok(file) = result else {
            return;
        };
        let Some(path) = file.path() else {
            toasts.add_toast(adw::Toast::new("Save failed: invalid destination"));
            return;
        };
        let handle = runtime.spawn_blocking(move || std::fs::write(path, sql));
        let toasts = toasts.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(())) => toasts.add_toast(adw::Toast::new("Query saved")),
                Ok(Err(err)) => toasts.add_toast(adw::Toast::new(&format!("Save failed: {err}"))),
                Err(err) => toasts.add_toast(adw::Toast::new(&format!("Save failed: {err}"))),
            }
        });
    });
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

fn build_snippets_content(
    buffer: &sourceview5::Buffer,
    popover: &gtk::Popover,
    connections: Rc<Vec<DbConnection>>,
    conn_dropdown: &gtk::DropDown,
    toasts: &adw::ToastOverlay,
) -> gtk::Widget {
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
        #[strong]
        toasts,
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
            toasts.add_toast(adw::Toast::new("Snippet saved"));
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
            #[strong]
            toasts,
            move |_| {
                let _ = store::delete_snippet(&id);
                popover.set_child(Some(&build_snippets_content(&buffer, &popover, connections.clone(), &conn_dropdown, &toasts)));
            }
        ));
        list.append(&row);
    }
    let scroller = gtk::ScrolledWindow::builder().child(&list).max_content_height(360).propagate_natural_height(true).build();
    content.append(&scroller);

    content.upcast()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AiReviewFocus {
    General,
    Performance,
    Security,
    Readability,
}

impl AiReviewFocus {
    const ALL: [Self; 4] = [Self::General, Self::Performance, Self::Security, Self::Readability];

    fn label(self) -> &'static str {
        match self {
            Self::General => "Geral",
            Self::Performance => "Performance",
            Self::Security => "Segurança",
            Self::Readability => "Legibilidade",
        }
    }

    /// The instruction handed to the AI Assistant — narrows which angle it leads with, on top of
    /// the tuning-advisor persona already set by `draco_core::assistant`'s system prompt (query
    /// best practices, performance, security, indexing, EXPLAIN plans).
    fn instruction(self) -> &'static str {
        match self {
            Self::General => "Faça uma revisão geral desta query: performance, segurança e boas práticas de escrita.",
            Self::Performance => "Analise a performance desta query: rode EXPLAIN, avalie os índices existentes e sugira otimizações.",
            Self::Security => "Analise a segurança desta query: risco de SQL injection, exposição de dados sensíveis e permissões necessárias.",
            Self::Readability => "Avalie a legibilidade e as boas práticas de escrita desta query: nomenclatura, formatação e clareza.",
        }
    }
}

/// Modal for the toolbar's "Avaliar com IA" button: pick a focus (mutually exclusive, "Geral" by
/// default) and optionally add a free-text note, then hand the assembled message to `on_evaluate`
/// — the caller turns that into "open/refresh this connection's AI Assistant tab and send it".
fn open_ai_review_dialog(parent: &impl IsA<gtk::Widget>, sql: String, on_evaluate: impl Fn(String) + 'static) {
    let dialog = adw::Dialog::builder().title("Avaliar com IA").content_width(480).content_height(420).build();

    let cancel_btn = gtk::Button::builder().label("Cancelar").build();
    let evaluate_btn = gtk::Button::builder().label("Avaliar").css_classes(["suggested-action"]).build();
    let header = adw::HeaderBar::builder().show_start_title_buttons(false).show_end_title_buttons(false).build();
    header.pack_start(&cancel_btn);
    header.pack_end(&evaluate_btn);

    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    content.append(&gtk::Label::builder().label("Foco da análise").xalign(0.0).css_classes(["heading"]).build());

    let focus_box = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(6).build();
    let selected_focus: Rc<RefCell<AiReviewFocus>> = Rc::new(RefCell::new(AiReviewFocus::General));
    let mut group_source: Option<gtk::ToggleButton> = None;
    for focus in AiReviewFocus::ALL {
        let toggle = gtk::ToggleButton::builder().label(focus.label()).active(focus == AiReviewFocus::General).build();
        match &group_source {
            Some(group) => toggle.set_group(Some(group)),
            None => group_source = Some(toggle.clone()),
        }
        let selected_focus = selected_focus.clone();
        toggle.connect_toggled(move |btn| {
            if btn.is_active() {
                *selected_focus.borrow_mut() = focus;
            }
        });
        focus_box.append(&toggle);
    }
    content.append(&focus_box);

    content.append(&gtk::Label::builder().label("Mensagem adicional (opcional)").xalign(0.0).css_classes(["heading"]).margin_top(6).build());
    let message_view = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).top_margin(6).bottom_margin(6).left_margin(8).right_margin(8).build();
    let message_scroller = gtk::ScrolledWindow::builder().child(&message_view).min_content_height(100).vexpand(true).build();
    message_scroller.add_css_class("card");
    content.append(&message_scroller);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));

    cancel_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));

    evaluate_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        #[weak]
        message_view,
        move |_| {
            let buffer = message_view.buffer();
            let custom = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).trim().to_string();
            let mut message = format!("{}\n\n```sql\n{sql}\n```", selected_focus.borrow().instruction());
            if !custom.is_empty() {
                message.push_str(&format!("\n\n{custom}"));
            }
            dialog.close();
            on_evaluate(message);
        }
    ));

    dialog.present(Some(parent));
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
    run_script_btn: &gtk::Button,
    cancel_btn: &gtk::Button,
    cancel_state: &Rc<RefCell<Option<watch::Sender<bool>>>>,
    export_csv_btn: &gtk::Button,
    export_json_btn: &gtk::Button,
    banner: &adw::Banner,
    results_stack: &gtk::Stack,
    error_view: &gtk::TextView,
) {
    run_btn.set_sensitive(false);
    explain_btn.set_sensitive(false);
    run_script_btn.set_sensitive(false);
    export_csv_btn.set_sensitive(false);
    export_json_btn.set_sensitive(false);
    cancel_btn.set_visible(true);
    cancel_btn.set_sensitive(true);
    status_label.set_label("Running…");
    status_label.set_tooltip_text(None);
    banner.set_revealed(false);
    results_stack.set_visible_child_name("grid");

    let (cancel_sender, cancel_receiver) = watch::channel(false);
    *cancel_state.borrow_mut() = Some(cancel_sender);

    let conn_id = conn.id.clone();
    let conn_label = conn.label.clone();
    let task_manager = manager.clone();
    let sql_for_task = sql.clone();
    let started = std::time::Instant::now();
    let handle = runtime.spawn(async move {
        let mgr = task_manager.lock().await;
        let driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
        tokio::select! {
            result = queries::execute_query(driver, &sql_for_task) => result,
            _ = wait_for_cancel(cancel_receiver) => {
                driver.cancel_active().await?;
                Err(draco_core::error::CoreError::Other("Query cancelled".to_string()))
            }
        }
    });

    let conn_id_for_history = conn.id.clone();
    let results_for_task = results.clone();
    let status_label_for_task = status_label.clone();
    let run_btn_for_task = run_btn.clone();
    let explain_btn_for_task = explain_btn.clone();
    let run_script_btn_for_task = run_script_btn.clone();
    let cancel_btn_for_task = cancel_btn.clone();
    let cancel_state_for_task = cancel_state.clone();
    let export_csv_btn_for_task = export_csv_btn.clone();
    let export_json_btn_for_task = export_json_btn.clone();
    let banner_for_task = banner.clone();
    let results_stack_for_task = results_stack.clone();
    let error_view_for_task = error_view.clone();
    let show_row_count = store::get_settings().show_row_count;
    glib::MainContext::default().spawn_local(async move {
        let elapsed = started.elapsed();
        match handle.await {
            Ok(Ok(result)) => {
                let row_count = result.rows.len();
                results_for_task.set_data(&result.columns, result.rows);
                export_csv_btn_for_task.set_sensitive(true);
                export_json_btn_for_task.set_sensitive(true);
                let status = if show_row_count {
                    format!("{row_count} rows in {:.2}s", elapsed.as_secs_f64())
                } else {
                    format!("Completed in {:.2}s", elapsed.as_secs_f64())
                };
                status_label_for_task.set_label(&status);
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
                export_csv_btn_for_task.set_sensitive(false);
                export_json_btn_for_task.set_sensitive(false);
                if matches!(err, draco_core::error::CoreError::NotConnected) {
                    status_label_for_task.set_label("Connection is not open. Click Connect first.");
                    banner_for_task.set_revealed(true);
                } else if err.to_string() == "Query cancelled" {
                    status_label_for_task.set_label("Cancelled");
                } else {
                    status_label_for_task.set_label("Error — see details below");
                    error_view_for_task.buffer().set_text(&format!("Error: {}", err.detailed_message()));
                    results_stack_for_task.set_visible_child_name("error");
                }
            }
            Err(_) => {
                results_for_task.clear();
                export_csv_btn_for_task.set_sensitive(false);
                export_json_btn_for_task.set_sensitive(false);
                status_label_for_task.set_label("Cancelled");
            }
        }
        run_btn_for_task.set_sensitive(true);
        explain_btn_for_task.set_sensitive(true);
        run_script_btn_for_task.set_sensitive(true);
        cancel_btn_for_task.set_sensitive(false);
        cancel_btn_for_task.set_visible(false);
        cancel_state_for_task.borrow_mut().take();
    });
}

#[allow(clippy::too_many_arguments)]
fn run_script_sql(
    sql: String,
    conn: &DbConnection,
    runtime: &tokio::runtime::Handle,
    manager: &SharedManager,
    results: &Rc<ResultsGrid>,
    status_label: &gtk::Label,
    run_btn: &gtk::Button,
    explain_btn: &gtk::Button,
    run_script_btn: &gtk::Button,
    cancel_btn: &gtk::Button,
    cancel_state: &Rc<RefCell<Option<watch::Sender<bool>>>>,
    export_csv_btn: &gtk::Button,
    export_json_btn: &gtk::Button,
    banner: &adw::Banner,
    results_stack: &gtk::Stack,
    error_view: &gtk::TextView,
) {
    run_btn.set_sensitive(false);
    explain_btn.set_sensitive(false);
    run_script_btn.set_sensitive(false);
    export_csv_btn.set_sensitive(false);
    export_json_btn.set_sensitive(false);
    cancel_btn.set_visible(true);
    cancel_btn.set_sensitive(true);
    status_label.set_label("Running script…");
    status_label.set_tooltip_text(None);
    banner.set_revealed(false);
    results.clear();
    results_stack.set_visible_child_name("grid");

    let (cancel_sender, cancel_receiver) = watch::channel(false);
    *cancel_state.borrow_mut() = Some(cancel_sender);

    let conn_id = conn.id.clone();
    let conn_label = conn.label.clone();
    let task_manager = manager.clone();
    let sql_for_task = sql.clone();
    let started = std::time::Instant::now();
    let handle = runtime.spawn(async move {
        let mgr = task_manager.lock().await;
        let driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
        tokio::select! {
            result = queries::execute_script(driver, &sql_for_task) => result,
            _ = wait_for_cancel(cancel_receiver) => {
                driver.cancel_active().await?;
                Err(draco_core::error::CoreError::Other("Query cancelled".to_string()))
            }
        }
    });

    let conn_id_for_history = conn.id.clone();
    let results_for_task = results.clone();
    let status_label_for_task = status_label.clone();
    let run_btn_for_task = run_btn.clone();
    let explain_btn_for_task = explain_btn.clone();
    let run_script_btn_for_task = run_script_btn.clone();
    let cancel_btn_for_task = cancel_btn.clone();
    let cancel_state_for_task = cancel_state.clone();
    let export_csv_btn_for_task = export_csv_btn.clone();
    let export_json_btn_for_task = export_json_btn.clone();
    let banner_for_task = banner.clone();
    let results_stack_for_task = results_stack.clone();
    let error_view_for_task = error_view.clone();
    let show_row_count = store::get_settings().show_row_count;
    glib::MainContext::default().spawn_local(async move {
        let elapsed = started.elapsed();
        match handle.await {
            Ok(Ok(result)) => {
                let row_count = result.rows.len();
                results_for_task.set_data(&result.columns, result.rows);
                export_csv_btn_for_task.set_sensitive(true);
                export_json_btn_for_task.set_sensitive(true);
                let status = if show_row_count {
                    format!("Script: {row_count} rows in {:.2}s", elapsed.as_secs_f64())
                } else {
                    format!("Script completed in {:.2}s", elapsed.as_secs_f64())
                };
                status_label_for_task.set_label(&status);
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
            Ok(Err(err)) => {
                results_for_task.clear();
                export_csv_btn_for_task.set_sensitive(false);
                export_json_btn_for_task.set_sensitive(false);
                if matches!(err, draco_core::error::CoreError::NotConnected) {
                    status_label_for_task.set_label("Connection is not open. Click Connect first.");
                    banner_for_task.set_revealed(true);
                } else if err.to_string() == "Query cancelled" {
                    status_label_for_task.set_label("Cancelled");
                } else {
                    status_label_for_task.set_label("Error — see details below");
                    error_view_for_task.buffer().set_text(&format!("Error: {}", err.detailed_message()));
                    results_stack_for_task.set_visible_child_name("error");
                }
            }
            Err(_) => {
                results_for_task.clear();
                export_csv_btn_for_task.set_sensitive(false);
                export_json_btn_for_task.set_sensitive(false);
                status_label_for_task.set_label("Cancelled");
            }
        }
        run_btn_for_task.set_sensitive(true);
        explain_btn_for_task.set_sensitive(true);
        run_script_btn_for_task.set_sensitive(true);
        cancel_btn_for_task.set_sensitive(false);
        cancel_btn_for_task.set_visible(false);
        cancel_state_for_task.borrow_mut().take();
    });
}

pub struct QueryEditor {
    root: gtk::Box,
    run_action: Rc<dyn Fn(bool)>,
    script_action: Rc<dyn Fn()>,
    /// Filled in by `bind_tab_page` right after `window_main.rs` creates this editor's
    /// `AdwTabPage` — the page (and its `title`) live outside the widget tree `QueryEditor`
    /// builds, so the rename button needs this handed in rather than looking it up itself.
    tab_page: Rc<RefCell<Option<adw::TabPage>>>,
}

impl QueryEditor {
    /// `initial` — `(connection id, SQL)` — pre-selects that connection and pre-fills the buffer,
    /// used by the Explorer table row's "Open SELECT * in New Query" shortcut. `None` behaves
    /// exactly like before: first connection selected, empty buffer.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connections: Vec<DbConnection>,
        runtime: tokio::runtime::Handle,
        manager: SharedManager,
        toasts: adw::ToastOverlay,
        initial: Option<(String, String)>,
        on_evaluate_with_ai: impl Fn(String, String) + 'static,
    ) -> Self {
        // `Fn` (not `FnOnce`) since the button can be clicked more than once — wrapped in `Rc` so
        // each click's closure (built fresh below and handed to the modal) can clone its own copy
        // to move into the dialog's own one-shot submit handler.
        let on_evaluate_with_ai: Rc<dyn Fn(String, String)> = Rc::new(on_evaluate_with_ai);
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

        let completion_words = sourceview5::CompletionWords::builder()
            .title("Database objects")
            .minimum_word_size(2)
            .build();
        view.completion().add_provider(&completion_words);
        completion_words.register(&buffer);
        let completion_buffer = sourceview5::Buffer::new(None);
        completion_words.register(&completion_buffer);

        let connections: Rc<Vec<DbConnection>> = Rc::new(connections);
        let initial_index = initial.as_ref().and_then(|(conn_id, _)| connections.iter().position(|c| &c.id == conn_id));

        let labels: Vec<&str> = connections.iter().map(|c| c.label.as_str()).collect();
        let conn_model = gtk::StringList::new(&labels);
        let conn_dropdown = gtk::DropDown::builder().model(&conn_model).build();
        if let Some(index) = initial_index {
            conn_dropdown.set_selected(index as u32);
        }
        let initial_conn = initial_index.and_then(|index| connections.get(index)).or_else(|| connections.first());
        if let Some(conn) = initial_conn {
            refresh_completion_words(Some(conn.clone()), &runtime, &manager, &completion_buffer);
        }
        if let Some((_, sql)) = &initial {
            buffer.set_text(sql);
        }
        conn_dropdown.connect_selected_notify(clone!(
            #[strong]
            connections,
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            completion_buffer,
            move |dropdown| {
                refresh_completion_words(
                    connections.get(dropdown.selected() as usize).cloned(),
                    &runtime,
                    &manager,
                    &completion_buffer,
                );
            }
        ));

        let run_btn = gtk::Button::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Run query (F8)")
            .css_classes(["suggested-action"])
            .build();

        let explain_btn = gtk::Button::builder().icon_name("view-list-symbolic").tooltip_text("Explain plan (F10)").css_classes(["flat"]).build();

        // Runs the whole buffer via the simple query protocol (`execute_script`, backed by
        // `simple_query`) instead of a single prepared statement — so a buffer with several
        // `;`-separated statements (a migration-style script) runs in one shot instead of
        // erroring out.
        let run_script_btn = gtk::Button::builder().icon_name("system-run-symbolic").tooltip_text("Run as script (F5)").css_classes(["flat"]).build();

        let cancel_btn = gtk::Button::builder()
            .icon_name("media-playback-stop-symbolic")
            .tooltip_text("Cancel query")
            .css_classes(["destructive-action"])
            .visible(false)
            .sensitive(false)
            .build();
        let cancel_state: Rc<RefCell<Option<watch::Sender<bool>>>> = Rc::new(RefCell::new(None));
        cancel_btn.connect_clicked(clone!(
            #[strong]
            cancel_state,
            move |_| {
                if let Some(sender) = cancel_state.borrow().as_ref() {
                    let _ = sender.send(true);
                }
            }
        ));

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
            #[strong]
            toasts,
            move |popover| popover.set_child(Some(&build_snippets_content(&buffer, popover, connections.clone(), &conn_dropdown, &toasts)))
        ));
        let snippets_btn = gtk::MenuButton::builder().icon_name("user-bookmarks-symbolic").tooltip_text("Snippets").css_classes(["flat"]).popover(&snippets_popover).build();

        // Bound after construction via `bind_tab_page` (the `AdwTabPage` doesn't exist yet at
        // this point — `window_main.rs` creates it from `widget()`'s return value).
        let tab_page: Rc<RefCell<Option<adw::TabPage>>> = Rc::new(RefCell::new(None));
        let rename_entry = gtk::Entry::builder().placeholder_text("Tab name").build();
        let rename_apply_btn = gtk::Button::builder().label("Rename").css_classes(["suggested-action"]).build();
        let rename_box = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(6).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).build();
        rename_box.append(&rename_entry);
        rename_box.append(&rename_apply_btn);
        let rename_popover = gtk::Popover::new();
        rename_popover.set_child(Some(&rename_box));
        rename_popover.connect_show(clone!(
            #[strong]
            tab_page,
            #[strong]
            rename_entry,
            move |_| {
                if let Some(page) = tab_page.borrow().as_ref() {
                    rename_entry.set_text(&page.title());
                }
                rename_entry.grab_focus();
            }
        ));
        rename_apply_btn.connect_clicked(clone!(
            #[strong]
            tab_page,
            #[strong]
            rename_entry,
            #[strong]
            rename_popover,
            move |_| {
                let title = rename_entry.text().to_string();
                if let Some(page) = tab_page.borrow().as_ref() {
                    if !title.trim().is_empty() {
                        page.set_title(&title);
                    }
                }
                rename_popover.popdown();
            }
        ));
        rename_entry.connect_activate(clone!(
            #[strong]
            rename_apply_btn,
            move |_| rename_apply_btn.emit_clicked()
        ));
        let rename_btn = gtk::MenuButton::builder().icon_name("document-edit-symbolic").tooltip_text("Rename tab").css_classes(["flat"]).popover(&rename_popover).build();

        let open_sql_btn = gtk::Button::builder().icon_name("document-open-symbolic").tooltip_text("Open .sql file").css_classes(["flat"]).build();
        open_sql_btn.connect_clicked(clone!(
            #[strong]
            buffer,
            #[strong]
            runtime,
            #[strong]
            toasts,
            move |btn| open_sql_file(&buffer, &runtime, &toasts, btn)
        ));

        let save_sql_btn = gtk::Button::builder().icon_name("media-floppy-symbolic").tooltip_text("Save query to .sql file").css_classes(["flat"]).build();
        save_sql_btn.connect_clicked(clone!(
            #[strong]
            buffer,
            #[strong]
            runtime,
            #[strong]
            toasts,
            move |btn| {
                let (start, end) = buffer.bounds();
                let sql = buffer.text(&start, &end, false).to_string();
                save_sql_to_file(sql, &runtime, &toasts, btn);
            }
        ));

        let status_label = gtk::Label::builder().xalign(0.0).css_classes(["dim-label"]).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::End).build();

        let ai_review_btn = gtk::Button::builder().icon_name("chat-message-new-symbolic").tooltip_text("Avaliar com IA (performance, segurança, legibilidade)").css_classes(["flat"]).build();
        ai_review_btn.connect_clicked(clone!(
            #[strong]
            connections,
            #[strong]
            conn_dropdown,
            #[strong]
            buffer,
            #[strong]
            status_label,
            #[strong]
            on_evaluate_with_ai,
            move |btn| {
                let Some(conn) = connections.get(conn_dropdown.selected() as usize) else {
                    status_label.set_label("No connection selected");
                    return;
                };
                let (start, end) = buffer.bounds();
                let sql = buffer.text(&start, &end, false).to_string();
                if sql.trim().is_empty() {
                    status_label.set_label("Write a query before evaluating it with AI");
                    return;
                }
                let Some(parent) = btn.root().and_downcast::<gtk::Window>() else { return };
                let conn_id = conn.id.clone();
                let on_evaluate_with_ai = on_evaluate_with_ai.clone();
                open_ai_review_dialog(&parent, sql, move |message| on_evaluate_with_ai(conn_id.clone(), message));
            }
        ));

        let toolbar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        toolbar.append(&conn_dropdown);
        toolbar.append(&rename_btn);
        toolbar.append(&run_btn);
        toolbar.append(&explain_btn);
        toolbar.append(&run_script_btn);
        toolbar.append(&cancel_btn);
        toolbar.append(&history_btn);
        toolbar.append(&snippets_btn);
        toolbar.append(&open_sql_btn);
        toolbar.append(&save_sql_btn);
        toolbar.append(&ai_review_btn);
        toolbar.append(&status_label);

        let results = Rc::new(ResultsGrid::new());
        let results_scroller = gtk::ScrolledWindow::builder().child(results.widget()).vexpand(true).build();

        // Shown instead of the results grid when a query/script fails — a `gtk::Label` truncates
        // long Postgres error text (that's what sent users hunting for the rest of "db error:
        // ..."), so this gives the full, selectable message room to wrap and scroll.
        let error_view = gtk::TextView::builder().editable(false).cursor_visible(false).monospace(true).wrap_mode(gtk::WrapMode::WordChar).top_margin(6).left_margin(6).right_margin(6).bottom_margin(6).build();
        error_view.add_css_class("error");
        let error_scroller = gtk::ScrolledWindow::builder().child(&error_view).vexpand(true).build();

        let results_stack = gtk::Stack::new();
        results_stack.add_named(&results_scroller, Some("grid"));
        results_stack.add_named(&error_scroller, Some("error"));
        results_stack.set_visible_child_name("grid");

        let export_csv_btn = gtk::Button::builder()
            .label("Export CSV")
            .icon_name("document-save-symbolic")
            .tooltip_text("Export complete results as CSV")
            .css_classes(["flat"])
            .sensitive(false)
            .build();
        let export_json_btn = gtk::Button::builder()
            .label("Export JSON")
            .icon_name("document-save-symbolic")
            .tooltip_text("Export complete results as JSON")
            .css_classes(["flat"])
            .sensitive(false)
            .build();
        export_csv_btn.connect_clicked(clone!(
            #[strong]
            results,
            #[strong]
            runtime,
            #[strong]
            toasts,
            move |button| export_results(&results, ExportFormat::Csv, &runtime, &toasts, button)
        ));
        export_json_btn.connect_clicked(clone!(
            #[strong]
            results,
            #[strong]
            runtime,
            #[strong]
            toasts,
            move |button| export_results(&results, ExportFormat::Json, &runtime, &toasts, button)
        ));
        let results_toolbar = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(6).margin_top(6).margin_start(6).margin_end(6).build();
        results_toolbar.append(&gtk::Label::builder().label("Results").hexpand(true).xalign(0.0).build());
        results_toolbar.append(&export_csv_btn);
        results_toolbar.append(&export_json_btn);
        let results_panel = gtk::Box::builder().orientation(gtk::Orientation::Vertical).vexpand(true).build();
        results_panel.append(&results_toolbar);
        results_panel.append(&results_stack);

        let paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .start_child(&editor_scroller)
            .end_child(&results_panel)
            .resize_start_child(true)
            .resize_end_child(true)
            .position(280)
            .build();

        let banner = adw::Banner::new("Connection lost");
        banner.set_button_label(Some("Reconnect"));
        banner.set_revealed(false);
        banner.connect_button_clicked(clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            connections,
            #[strong]
            conn_dropdown,
            #[strong]
            status_label,
            move |banner| {
                let Some(conn) = connections.get(conn_dropdown.selected() as usize) else { return };
                let conn_id = conn.id.clone();
                let task_manager = manager.clone();
                let handle = runtime.spawn(async move {
                    let mut mgr = task_manager.lock().await;
                    crate::connection_runtime::ensure_connected(&mut mgr, &conn_id).await
                });
                let banner = banner.clone();
                let status_label = status_label.clone();
                glib::MainContext::default().spawn_local(async move {
                    match handle.await {
                        Ok(Ok(())) => {
                            banner.set_revealed(false);
                            status_label.set_label("Reconnected");
                        }
                        Ok(Err(err)) => banner.set_title(&format!("Reconnect failed: {err}")),
                        Err(_) => banner.set_title("Reconnect task failed"),
                    }
                });
            }
        ));

        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        root.append(&banner);
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
            run_script_btn,
            #[strong]
            cancel_btn,
            #[strong]
            cancel_state,
            #[strong]
            export_csv_btn,
            #[strong]
            export_json_btn,
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            banner,
            #[strong]
            results_stack,
            #[strong]
            error_view,
            move |explain: bool| {
                let selected = conn_dropdown.selected();
                if !run_btn.is_sensitive() {
                    return;
                }
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
                run_sql(
                    sql,
                    conn,
                    !explain,
                    &runtime,
                    &manager,
                    &results,
                    &status_label,
                    &run_btn,
                    &explain_btn,
                    &run_script_btn,
                    &cancel_btn,
                    &cancel_state,
                    &export_csv_btn,
                    &export_json_btn,
                    &banner,
                    &results_stack,
                    &error_view,
                );
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

        // Separate from `do_run` (rather than a third `bool`/enum branch) since it takes no
        // `explain` parameter and runs via `execute_script` instead of `execute_query` — the
        // grid still gets populated, from whichever statement in the script ran last.
        let do_run_script: Rc<dyn Fn()> = Rc::new(clone!(
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
            run_script_btn,
            #[strong]
            cancel_btn,
            #[strong]
            cancel_state,
            #[strong]
            export_csv_btn,
            #[strong]
            export_json_btn,
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            banner,
            #[strong]
            results_stack,
            #[strong]
            error_view,
            move || {
                let selected = conn_dropdown.selected();
                if !run_script_btn.is_sensitive() {
                    return;
                }
                let Some(conn) = connections.get(selected as usize) else {
                    status_label.set_label("No connection selected");
                    return;
                };
                let (start, end) = buffer.bounds();
                let sql = buffer.text(&start, &end, false).to_string();
                if sql.trim().is_empty() {
                    return;
                }
                run_script_sql(
                    sql,
                    conn,
                    &runtime,
                    &manager,
                    &results,
                    &status_label,
                    &run_btn,
                    &explain_btn,
                    &run_script_btn,
                    &cancel_btn,
                    &cancel_state,
                    &export_csv_btn,
                    &export_json_btn,
                    &banner,
                    &results_stack,
                    &error_view,
                );
            }
        ));

        run_script_btn.connect_clicked(clone!(
            #[strong]
            do_run_script,
            move |_| do_run_script()
        ));

        Self { root, run_action: do_run, script_action: do_run_script, tab_page }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Hands this editor its own `AdwTabPage` — call right after `tab_view.append(widget())`, so
    /// the rename button (pencil icon) can read/set the tab's title. The page can't be passed
    /// into `new` itself since it's only created from `widget()`'s return value.
    pub fn bind_tab_page(&self, page: adw::TabPage) {
        *self.tab_page.borrow_mut() = Some(page);
    }

    /// `Fn(bool)` — `false` runs the buffer as-is, `true` wraps it in `EXPLAIN` first. Exposed so
    /// `window_main.rs` can wire the window-level `F8`/`F10` accelerators (the same
    /// `win.<action>` + `set_accels_for_action` mechanism already used for `Ctrl+P`/`Ctrl+T`) to
    /// whichever query tab is currently selected — a widget-local `EventControllerKey` on `root`
    /// turned out not to reliably receive `F8`/`F10` while the `GtkSourceView` had focus.
    pub fn run_action(&self) -> Rc<dyn Fn(bool)> {
        self.run_action.clone()
    }

    /// Same window-level-accelerator story as `run_action`, for `F5` ("Run as script").
    pub fn script_action(&self) -> Rc<dyn Fn()> {
        self.script_action.clone()
    }
}
