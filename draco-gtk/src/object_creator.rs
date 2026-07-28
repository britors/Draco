//! Small visual creators for schema objects that do not need a full editor yet.

use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries;
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

pub fn open_sequence(
    parent: &impl IsA<gtk::Widget>,
    conn_id: String,
    schema: String,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_created: impl Fn() + 'static,
) {
    let on_created = Rc::new(on_created);
    let name_row = adw::EntryRow::builder().title("Sequence name").build();
    let error_label = error_label();
    let content = form_content(&error_label);
    let group = adw::PreferencesGroup::builder().title("Sequence").build();
    group.add(&name_row);
    content.append(&group);

    let (dialog, create_btn, cancel_btn) = dialog_parts("New Sequence", &content);
    cancel_btn.connect_clicked(clone!(#[weak] dialog, move |_| {
        dialog.close();
    }));

    create_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        name_row,
        #[strong]
        error_label,
        #[strong]
        on_created,
        move |button| {
            let name = name_row.text().to_string();
            if name.trim().is_empty() {
                show_error(&error_label, "Sequence name is required");
                return;
            }
            button.set_sensitive(false);
            let task_manager = manager.clone();
            let task_id = conn_id.clone();
            let task_schema = schema.clone();
            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
                let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
                queries::create_sequence(driver, &task_schema, &name).await
            });
            let button_for_task = button.clone();
            let error_for_task = error_label.clone();
            let on_created_for_task = on_created.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok(())) => {
                        dialog.close();
                        on_created_for_task();
                    }
                    Ok(Err(err)) => show_error(&error_for_task, &format!("Failed to create sequence: {err}")),
                    Err(err) => show_error(&error_for_task, &format!("Failed to create sequence: {err}")),
                }
                button_for_task.set_sensitive(true);
            });
        }
    ));

    dialog.present(Some(parent));
}

pub fn open_trigger(
    parent: &impl IsA<gtk::Widget>,
    conn_id: String,
    schema: String,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_created: impl Fn() + 'static,
) {
    let on_created = Rc::new(on_created);
    let name_row = adw::EntryRow::builder().title("Trigger name").build();
    let table_row = adw::EntryRow::builder().title("Table").build();
    let function_row = adw::EntryRow::builder()
        .title("Function")
        .text("function_name")
        .build();
    let events_row = adw::EntryRow::builder()
        .title("Events")
        .text("INSERT")
        .build();
    let timing_model = gtk::StringList::new(&["BEFORE", "AFTER", "INSTEAD OF"]);
    let timing_dropdown = gtk::DropDown::builder().model(&timing_model).build();
    let timing_row = adw::ActionRow::builder().title("Timing").build();
    timing_row.add_suffix(&timing_dropdown);

    let error_label = error_label();
    let content = form_content(&error_label);
    let group = adw::PreferencesGroup::builder().title("Trigger").build();
    group.add(&name_row);
    group.add(&table_row);
    group.add(&timing_row);
    group.add(&events_row);
    group.add(&function_row);
    content.append(&group);
    content.append(
        &gtk::Label::builder()
            .label("Events can be separated by spaces or commas. The function is called with no arguments.")
            .wrap(true)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build(),
    );

    let (dialog, create_btn, cancel_btn) = dialog_parts("New Trigger", &content);
    cancel_btn.connect_clicked(clone!(#[weak] dialog, move |_| {
        dialog.close();
    }));

    create_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[strong]
        name_row,
        #[strong]
        table_row,
        #[strong]
        timing_dropdown,
        #[strong]
        events_row,
        #[strong]
        function_row,
        #[strong]
        error_label,
        #[strong]
        on_created,
        move |button| {
            let name = name_row.text().to_string();
            let table = table_row.text().to_string();
            let events = events_row.text().to_string();
            let function = function_row.text().to_string();
            if name.trim().is_empty() || table.trim().is_empty() || function.trim().is_empty() {
                show_error(&error_label, "Trigger name, table, and function are required");
                return;
            }
            let timing = ["BEFORE", "AFTER", "INSTEAD OF"]
                .get(timing_dropdown.selected() as usize)
                .copied()
                .unwrap_or("BEFORE")
                .to_string();
            button.set_sensitive(false);
            let task_manager = manager.clone();
            let task_id = conn_id.clone();
            let task_schema = schema.clone();
            let handle = runtime.spawn(async move {
                let mut mgr = task_manager.lock().await;
                crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
                let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
                queries::create_trigger(driver, &task_schema, &name, &table, &timing, &events, &function).await
            });
            let button_for_task = button.clone();
            let error_for_task = error_label.clone();
            let on_created_for_task = on_created.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.await {
                    Ok(Ok(())) => {
                        dialog.close();
                        on_created_for_task();
                    }
                    Ok(Err(err)) => show_error(&error_for_task, &format!("Failed to create trigger: {err}")),
                    Err(err) => show_error(&error_for_task, &format!("Failed to create trigger: {err}")),
                }
                button_for_task.set_sensitive(true);
            });
        }
    ));

    dialog.present(Some(parent));
}

fn error_label() -> gtk::Label {
    gtk::Label::builder()
        .css_classes(["error"])
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .build()
}

fn show_error(label: &gtk::Label, message: &str) {
    label.set_label(message);
    label.set_visible(true);
}

fn form_content(error_label: &gtk::Label) -> gtk::Box {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(error_label);
    content
}

fn dialog_parts(title: &str, content: &gtk::Box) -> (adw::Dialog, gtk::Button, gtk::Button) {
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(560)
        .content_height(360)
        .build();
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let create_btn = gtk::Button::builder()
        .label("Create")
        .css_classes(["suggested-action"])
        .build();
    let cancel_btn = gtk::Button::builder().label("Cancel").build();
    header.pack_start(&cancel_btn);
    header.pack_end(&create_btn);
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&gtk::ScrolledWindow::builder().child(content).vexpand(true).build()));
    dialog.set_child(Some(&toolbar));
    (dialog, create_btn, cancel_btn)
}
