//! UI for PostgreSQL backups and restores. The actual child-process and SSH-aware execution
//! lives in `draco-core::postgres::backup`; this module owns only GTK widgets and lifecycle.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::connection::DbConnection;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::backup::{
    self, DumpFormat, DumpOptions, RestoreOptions, ToolConnection, ToolEvent, ToolResult,
};
use draco_core::secrets;
use gtk::gio;
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::{mpsc, watch, Mutex};

use crate::confirm::confirm_destructive;

type SharedManager = Arc<Mutex<ConnectionManager>>;

#[derive(Clone)]
enum Operation {
    Dump(DumpOptions),
    Restore(RestoreOptions),
}

pub fn open(
    parent: &impl IsA<gtk::Window>,
    conn: DbConnection,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
) {
    let dialog = adw::Dialog::builder()
        .title(format!("Backup / Restore — {}", conn.label))
        .content_width(680)
        .content_height(660)
        .build();

    let stack = adw::ViewStack::builder().vexpand(true).build();
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();

    let (backup_page, backup_controls) = build_backup_page(
        parent,
        &stack,
        &dialog,
        conn.clone(),
        runtime.clone(),
        manager.clone(),
    );
    let (restore_page, restore_controls) = build_restore_page(
        parent,
        &stack,
        &dialog,
        conn.clone(),
        runtime.clone(),
        manager.clone(),
    );
    stack.add_titled(&backup_page, Some("backup"), "Backup");
    stack.add_titled(&restore_page, Some("restore"), "Restore");

    let header = adw::HeaderBar::new();
    let close_btn = gtk::Button::builder().label("Close").build();
    header.pack_start(&close_btn);
    header.set_title_widget(Some(&switcher));
    close_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));

    let root = adw::ToolbarView::new();
    root.add_top_bar(&header);
    root.set_content(Some(&stack));
    dialog.set_child(Some(&root));

    // Keep the control structs alive until after the signal connections are installed. Their
    // widgets are owned by the pages, while these values contain the shared process state.
    backup_controls.install(&dialog);
    restore_controls.install(&dialog);
    dialog.present(Some(parent.as_ref().upcast_ref::<gtk::Widget>()));
}

struct PageControls {
    run_btn: gtk::Button,
    cancel_btn: gtk::Button,
    progress: gtk::ProgressBar,
    status: gtk::Label,
    error: gtk::Label,
    log: gtk::TextBuffer,
    cancel_tx: Rc<RefCell<Option<watch::Sender<bool>>>>,
    conn: DbConnection,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    operation: Rc<dyn Fn() -> Option<Operation>>,
}

impl PageControls {
    fn install(self, dialog: &adw::Dialog) {
        let controls = Rc::new(self);
        controls.cancel_btn.set_visible(false);

        controls.run_btn.connect_clicked(clone!(
            #[strong]
            controls,
            #[weak]
            dialog,
            move |_| {
                let Some(operation) = (controls.operation)() else {
                    controls
                        .error
                        .set_label("Choose a valid file or output location first.");
                    controls.error.set_visible(true);
                    return;
                };
                controls.error.set_visible(false);
                start_operation(&controls, &dialog, operation);
            }
        ));

        controls.cancel_btn.connect_clicked(clone!(
            #[strong]
            controls,
            move |_| {
                if let Some(sender) = controls.cancel_tx.borrow().as_ref() {
                    let _ = sender.send(true);
                }
                controls.cancel_btn.set_sensitive(false);
                controls.status.set_label("Cancelling…");
            }
        ));
    }
}

fn build_backup_page(
    _parent: &impl IsA<gtk::Window>,
    _stack: &adw::ViewStack,
    _dialog: &adw::Dialog,
    conn: DbConnection,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
) -> (gtk::Widget, PageControls) {
    let format_model = gtk::StringList::new(&[
        "Plain SQL (.sql)",
        "Custom archive (.dump)",
        "Directory archive",
    ]);
    let format_row = adw::ComboRow::builder()
        .title("Format")
        .model(&format_model)
        .build();
    let compression_row = adw::SpinRow::builder()
        .title("Compression level (0–9)")
        .subtitle("0 disables compression")
        .adjustment(&gtk::Adjustment::new(6.0, 0.0, 9.0, 1.0, 2.0, 0.0))
        .build();
    let schema_row = adw::EntryRow::builder().title("Schemas (optional)").build();
    schema_row.set_tooltip_text(Some("Comma-separated schema names"));
    let table_row = adw::EntryRow::builder().title("Tables (optional)").build();
    table_row.set_tooltip_text(Some("Comma-separated schema.table names"));

    let output_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let output_row = adw::ActionRow::builder()
        .title("Output")
        .subtitle("No output selected")
        .build();
    let choose_btn = gtk::Button::with_label("Choose…");
    choose_btn.add_css_class("flat");
    output_row.add_suffix(&choose_btn);
    choose_btn.connect_clicked(clone!(
        #[strong]
        output_path,
        #[strong]
        format_row,
        #[strong]
        output_row,
        move |btn| {
            let Some(window) = btn.root().and_downcast::<gtk::Window>() else {
                return;
            };
            let dialog = gtk::FileDialog::builder()
                .title("Choose backup destination")
                .accept_label("Select")
                .build();
            if format_row.selected() == 2 {
                let output_path_for_result = output_path.clone();
                let output_row_for_result = output_row.clone();
                dialog.select_folder(Some(&window), None::<&gio::Cancellable>, move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            output_row_for_result.set_subtitle(&path.display().to_string());
                            *output_path_for_result.borrow_mut() = Some(path);
                        }
                    }
                });
            } else {
                let default_name = if format_row.selected() == 0 {
                    "draco-backup.sql"
                } else {
                    "draco-backup.dump"
                };
                dialog.set_initial_name(Some(default_name));
                let output_path_for_result = output_path.clone();
                let output_row_for_result = output_row.clone();
                dialog.save(Some(&window), None::<&gio::Cancellable>, move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            output_row_for_result.set_subtitle(&path.display().to_string());
                            *output_path_for_result.borrow_mut() = Some(path);
                        }
                    }
                });
            }
        }
    ));

    let group = adw::PreferencesGroup::builder()
        .title("Backup options")
        .build();
    group.add(&format_row);
    group.add(&compression_row);
    group.add(&schema_row);
    group.add(&table_row);
    group.add(&output_row);

    let (run_btn, cancel_btn, progress, status, error, log) = build_process_footer();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content.append(&group);
    content.append(&run_btn);
    content.append(&cancel_btn);
    content.append(&progress);
    content.append(&status);
    content.append(&error);
    content.append(&log.view);

    let operation = {
        let output_path = output_path.clone();
        let format_row = format_row.clone();
        let compression_row = compression_row.clone();
        let schema_row = schema_row.clone();
        let table_row = table_row.clone();
        Rc::new(move || {
            let output = output_path.borrow().clone()?;
            let format = match format_row.selected() {
                1 => DumpFormat::Custom,
                2 => DumpFormat::Directory,
                _ => DumpFormat::Plain,
            };
            let csv = |value: String| {
                value
                    .split(',')
                    .map(|part| part.trim().to_string())
                    .filter(|part| !part.is_empty())
                    .collect()
            };
            Some(Operation::Dump(DumpOptions {
                output,
                format,
                compression: (compression_row.value() as u8)
                    .gt(&0)
                    .then_some(compression_row.value() as u8),
                schemas: csv(schema_row.text().to_string()),
                tables: csv(table_row.text().to_string()),
            }))
        }) as Rc<dyn Fn() -> Option<Operation>>
    };

    let controls = PageControls {
        run_btn,
        cancel_btn,
        progress,
        status,
        error,
        log: log.buffer,
        cancel_tx: Rc::new(RefCell::new(None)),
        conn,
        runtime,
        manager,
        operation,
    };
    (content.upcast(), controls)
}

fn build_restore_page(
    _parent: &impl IsA<gtk::Window>,
    _stack: &adw::ViewStack,
    _dialog: &adw::Dialog,
    conn: DbConnection,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
) -> (gtk::Widget, PageControls) {
    let input_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let input_row = adw::ActionRow::builder()
        .title("Dump file")
        .subtitle("No file selected")
        .build();
    let choose_btn = gtk::Button::with_label("Choose…");
    choose_btn.add_css_class("flat");
    input_row.add_suffix(&choose_btn);
    choose_btn.connect_clicked(clone!(
        #[strong]
        input_path,
        #[strong]
        input_row,
        move |btn| {
            let Some(window) = btn.root().and_downcast::<gtk::Window>() else {
                return;
            };
            let dialog = gtk::FileDialog::builder()
                .title("Choose PostgreSQL dump")
                .accept_label("Open")
                .build();
            let input_path_for_result = input_path.clone();
            let input_row_for_result = input_row.clone();
            dialog.open(Some(&window), None::<&gio::Cancellable>, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        input_row_for_result.set_subtitle(&path.display().to_string());
                        *input_path_for_result.borrow_mut() = Some(path);
                    }
                }
            });
        }
    ));

    let target_db_row = adw::EntryRow::builder()
        .title("Target database")
        .text(&conn.database)
        .build();
    let clean_row = adw::SwitchRow::builder()
        .title("Clean before restore")
        .subtitle("Drop database objects before recreating them")
        .build();
    let transaction_row = adw::SwitchRow::builder()
        .title("Single transaction")
        .subtitle("Use one transaction when supported by the dump format")
        .active(true)
        .build();

    let group = adw::PreferencesGroup::builder()
        .title("Restore options")
        .build();
    group.add(&input_row);
    group.add(&target_db_row);
    group.add(&clean_row);
    group.add(&transaction_row);

    let (run_btn, cancel_btn, progress, status, error, log) = build_process_footer();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content.append(&group);
    content.append(&run_btn);
    content.append(&cancel_btn);
    content.append(&progress);
    content.append(&status);
    content.append(&error);
    content.append(&log.view);

    let operation = {
        let input_path = input_path.clone();
        let target_db_row = target_db_row.clone();
        let clean_row = clean_row.clone();
        let transaction_row = transaction_row.clone();
        Rc::new(move || {
            let input = input_path.borrow().clone()?;
            let target_database = target_db_row.text().to_string();
            if target_database.trim().is_empty() {
                return None;
            }
            Some(Operation::Restore(RestoreOptions {
                input,
                target_database,
                clean: clean_row.is_active(),
                single_transaction: transaction_row.is_active(),
            }))
        }) as Rc<dyn Fn() -> Option<Operation>>
    };

    let controls = PageControls {
        run_btn,
        cancel_btn,
        progress,
        status,
        error,
        log: log.buffer,
        cancel_tx: Rc::new(RefCell::new(None)),
        conn,
        runtime,
        manager,
        operation,
    };
    (content.upcast(), controls)
}

struct LogWidgets {
    view: gtk::ScrolledWindow,
    buffer: gtk::TextBuffer,
}

fn build_process_footer() -> (
    gtk::Button,
    gtk::Button,
    gtk::ProgressBar,
    gtk::Label,
    gtk::Label,
    LogWidgets,
) {
    let run_btn = gtk::Button::builder()
        .label("Start")
        .css_classes(["suggested-action"])
        .build();
    let cancel_btn = gtk::Button::builder()
        .label("Cancel operation")
        .css_classes(["destructive-action"])
        .build();
    let progress = gtk::ProgressBar::builder()
        .show_text(true)
        .visible(false)
        .build();
    let status = gtk::Label::builder().xalign(0.0).visible(false).build();
    let error = gtk::Label::builder()
        .css_classes(["error"])
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .build();
    let buffer = gtk::TextBuffer::new(None);
    let view = gtk::TextView::builder()
        .buffer(&buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .height_request(150)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .child(&view)
        .vexpand(true)
        .visible(true)
        .build();
    (
        run_btn,
        cancel_btn,
        progress,
        status,
        error,
        LogWidgets {
            view: scroller,
            buffer,
        },
    )
}

fn start_operation(controls: &Rc<PageControls>, dialog: &adw::Dialog, operation: Operation) {
    if let Operation::Restore(options) = &operation {
        let target = options.target_database.clone();
        confirm_destructive(
            dialog,
            "Restore database?",
            &format!("This can replace objects in the database “{target}”. Make sure you have a backup before continuing."),
            "Restore Database",
            clone!(#[strong] controls, #[weak] dialog, move || start_process(&controls, &dialog, operation.clone())),
        );
    } else {
        start_process(controls, dialog, operation);
    }
}

fn start_process(controls: &Rc<PageControls>, _dialog: &adw::Dialog, operation: Operation) {
    controls.run_btn.set_sensitive(false);
    controls.cancel_btn.set_sensitive(true);
    controls.cancel_btn.set_visible(true);
    controls.progress.set_visible(true);
    controls.progress.set_fraction(0.0);
    controls.status.set_visible(true);
    controls.status.set_label("Starting…");
    controls.error.set_visible(false);
    controls.log.set_text("");

    let (cancel_tx, cancel_rx) = watch::channel(false);
    *controls.cancel_tx.borrow_mut() = Some(cancel_tx);
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let operation_for_task = operation.clone();
    let manager = controls.manager.clone();
    let conn = controls.conn.clone();
    let runtime = controls.runtime.clone();
    let task = runtime.spawn(async move {
        let password = secrets::get_password(&conn.id).await?;
        let mut mgr = manager.lock().await;
        crate::connection_runtime::ensure_connected(&mut mgr, &conn.id).await?;
        let driver = mgr
            .get_driver(&conn.id)
            .ok_or(draco_core::error::CoreError::NotConnected)?;
        match operation_for_task {
            Operation::Dump(options) => {
                backup::run_dump(
                    driver,
                    ToolConnection {
                        password: &password,
                        database: &conn.database,
                        user: &conn.user,
                        ssl: conn.ssl,
                    },
                    options,
                    cancel_rx,
                    events_tx,
                )
                .await
            }
            Operation::Restore(options) => {
                backup::run_restore(
                    driver,
                    ToolConnection {
                        password: &password,
                        database: &conn.database,
                        user: &conn.user,
                        ssl: conn.ssl,
                    },
                    options,
                    cancel_rx,
                    events_tx,
                )
                .await
            }
        }
    });

    let run_btn = controls.run_btn.clone();
    let cancel_btn = controls.cancel_btn.clone();
    let progress = controls.progress.clone();
    let status = controls.status.clone();
    let error = controls.error.clone();
    let buffer = controls.log.clone();
    let cancel_state = controls.cancel_tx.clone();
    glib::MainContext::default().spawn_local(async move {
        let mut final_event = None;
        while let Some(event) = events_rx.recv().await {
            match event {
                ToolEvent::Stdout(line) => append_log(&buffer, &line),
                ToolEvent::Stderr(line) => append_log(&buffer, &format!("stderr: {line}")),
                ToolEvent::Finished(result) => {
                    final_event = Some(result);
                    break;
                }
            }
        }
        let task_result = task.await;
        let result = match task_result {
            Ok(Ok(result)) => Some(result),
            Ok(Err(err)) => {
                error.set_label(&format!("Operation failed: {err}"));
                error.set_visible(true);
                None
            }
            Err(err) => {
                error.set_label(&format!("Operation task failed: {err}"));
                error.set_visible(true);
                None
            }
        };
        let result = result.or(final_event);
        match result {
            Some(ToolResult {
                cancelled: true, ..
            }) => status.set_label("Operation cancelled"),
            Some(result) if result.succeeded() => {
                progress.set_fraction(1.0);
                status.set_label("Operation completed successfully");
            }
            Some(result) => {
                status.set_label(&format!(
                    "Operation failed (exit code {:?})",
                    result.exit_code
                ));
                error.set_visible(true);
            }
            None => status.set_label("Operation failed"),
        }
        run_btn.set_sensitive(true);
        cancel_btn.set_sensitive(true);
        cancel_btn.set_visible(false);
        *cancel_state.borrow_mut() = None;
    });
}

fn append_log(buffer: &gtk::TextBuffer, line: &str) {
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, &format!("{line}\n"));
}
