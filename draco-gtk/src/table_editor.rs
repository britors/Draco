//! ALTER TABLE editor: pre-filled from the table's current columns (as already loaded by the
//! Detail view), lets you rename the table, rename/retype/add/drop columns, preview the
//! generated `ALTER TABLE` statements and apply them atomically. Opened from the "Edit" button
//! in `table_detail.rs`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, ColumnEdit, TableDetail, TableDetailColumn};
use gtk::glib;
use gtk::glib::clone;
use sourceview5::prelude::*;
use tokio::sync::Mutex;

use crate::confirm::confirm_destructive;

type SharedManager = Arc<Mutex<ConnectionManager>>;

struct EditableColumnRow {
    root: gtk::Box,
    original_name: Option<String>,
    name: gtk::Entry,
    data_type: gtk::Entry,
    nullable: gtk::CheckButton,
    primary_key: gtk::CheckButton,
    default: gtk::Entry,
    removed: RefCell<bool>,
    remove_btn: gtk::Button,
}

impl EditableColumnRow {
    fn new(existing: Option<&TableDetailColumn>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        let name = gtk::Entry::builder()
            .placeholder_text("column_name")
            .hexpand(true)
            .build();
        let data_type = gtk::Entry::builder()
            .placeholder_text("type (e.g. text, integer)")
            .width_chars(18)
            .build();
        let nullable = gtk::CheckButton::builder()
            .label("Null")
            .active(true)
            .build();
        let primary_key = gtk::CheckButton::builder().label("PK").build();
        let default = gtk::Entry::builder()
            .placeholder_text("default (optional)")
            .width_chars(12)
            .build();
        let remove_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat"])
            .build();

        if let Some(c) = existing {
            name.set_text(&c.name);
            data_type.set_text(&c.full_type);
            nullable.set_active(c.is_nullable);
            primary_key.set_active(c.is_primary_key);
            if let Some(d) = &c.column_default {
                default.set_text(d);
            }
        }

        root.append(&name);
        root.append(&data_type);
        root.append(&nullable);
        root.append(&primary_key);
        root.append(&default);
        root.append(&remove_btn);

        Self {
            root,
            original_name: existing.map(|c| c.name.clone()),
            name,
            data_type,
            nullable,
            primary_key,
            default,
            removed: RefCell::new(false),
            remove_btn,
        }
    }

    fn set_removed_style(&self, removed: bool) {
        self.name.set_sensitive(!removed);
        self.data_type.set_sensitive(!removed);
        self.nullable.set_sensitive(!removed);
        self.primary_key.set_sensitive(!removed);
        self.default.set_sensitive(!removed);
        self.remove_btn.set_icon_name(if removed {
            "edit-undo-symbolic"
        } else {
            "user-trash-symbolic"
        });
    }

    fn to_edit(&self) -> ColumnEdit {
        let default = self.default.text().to_string();
        ColumnEdit {
            original_name: self.original_name.clone(),
            name: self.name.text().to_string(),
            data_type: self.data_type.text().to_string(),
            nullable: self.nullable.is_active(),
            default: if default.trim().is_empty() {
                None
            } else {
                Some(default)
            },
            primary_key: self.primary_key.is_active(),
            removed: *self.removed.borrow(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn open(
    parent: &impl IsA<gtk::Widget>,
    conn_id: String,
    schema: String,
    table: String,
    detail: TableDetail,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_altered: impl Fn() + 'static,
) {
    let on_altered = Rc::new(on_altered);
    let primary_key_constraint = detail
        .constraints
        .iter()
        .find(|constraint| constraint.kind == "PRIMARY KEY")
        .map(|constraint| constraint.name.clone());
    let original_columns = Rc::new(detail.columns);

    let table_name_row = adw::EntryRow::builder()
        .title("Table name")
        .text(table.as_str())
        .build();
    let basics_group = adw::PreferencesGroup::builder().title("Table").build();
    basics_group.add(&table_name_row);

    let columns_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let columns: Rc<RefCell<Vec<Rc<EditableColumnRow>>>> = Rc::new(RefCell::new(Vec::new()));

    for c in original_columns.iter() {
        let row = Rc::new(EditableColumnRow::new(Some(c)));
        columns_list.append(&row.root);
        row.remove_btn.connect_clicked(clone!(
            #[strong]
            row,
            move |_| {
                let mut removed = row.removed.borrow_mut();
                *removed = !*removed;
                let is_removed = *removed;
                drop(removed);
                row.set_removed_style(is_removed);
            }
        ));
        columns.borrow_mut().push(row);
    }

    let add_column = {
        let columns = columns.clone();
        let columns_list = columns_list.clone();
        move || {
            let row = Rc::new(EditableColumnRow::new(None));
            columns_list.append(&row.root);
            columns.borrow_mut().push(row.clone());

            row.remove_btn.connect_clicked(clone!(
                #[strong]
                columns,
                #[strong]
                columns_list,
                #[strong]
                row,
                move |_| {
                    columns_list.remove(&row.root);
                    columns.borrow_mut().retain(|c| !Rc::ptr_eq(c, &row));
                }
            ));
        }
    };

    let add_col_btn = gtk::Button::builder()
        .label("+ Add Column")
        .halign(gtk::Align::Start)
        .build();
    add_col_btn.connect_clicked(move |_| add_column());

    let preview_buffer = sourceview5::Buffer::new(None);
    if let Some(lang) = sourceview5::LanguageManager::default().language("sql") {
        preview_buffer.set_language(Some(&lang));
    }
    let preview_view = sourceview5::View::with_buffer(&preview_buffer);
    preview_view.set_monospace(true);
    preview_view.set_editable(false);
    preview_view.set_top_margin(6);
    preview_view.set_left_margin(6);
    preview_view.set_bottom_margin(6);
    let preview_scroller = gtk::ScrolledWindow::builder()
        .child(&preview_view)
        .min_content_height(120)
        .max_content_height(160)
        .build();
    let refresh_preview_btn = gtk::Button::builder()
        .label("Refresh Preview")
        .halign(gtk::Align::Start)
        .build();

    let error_label = gtk::Label::builder()
        .css_classes(["error"])
        .wrap(true)
        .visible(false)
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&error_label);
    content.append(&basics_group);
    content.append(
        &gtk::Label::builder()
            .label("Columns")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    content.append(&columns_list);
    content.append(&add_col_btn);

    let scroller = gtk::ScrolledWindow::builder()
        .child(&content)
        .vexpand(true)
        .min_content_width(620)
        .build();

    // Pinned to the bottom via `add_bottom_bar` (outside the scroller above) so the generated
    // ALTER TABLE statements stay visible no matter how far the column list is scrolled.
    let preview_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    preview_box.append(
        &gtk::Label::builder()
            .label("SQL Preview")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    preview_box.append(&refresh_preview_btn);
    preview_box.append(&preview_scroller);

    let dialog = adw::Dialog::builder()
        .title(format!("Edit Table · {schema}.{table}"))
        .content_width(660)
        .content_height(700)
        .build();
    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let apply_btn = gtk::Button::builder()
        .label("Apply")
        .css_classes(["suggested-action"])
        .build();
    let cancel_btn = gtk::Button::builder().label("Cancel").build();
    header.pack_start(&cancel_btn);
    header.pack_end(&apply_btn);
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));
    toolbar_view.add_bottom_bar(&preview_box);
    dialog.set_child(Some(&toolbar_view));

    cancel_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));

    let build_statements = {
        let columns = columns.clone();
        let original_columns = original_columns.clone();
        let primary_key_constraint = primary_key_constraint.clone();
        let schema = schema.clone();
        let table = table.clone();
        move |table_name_row: &adw::EntryRow| -> Result<Vec<String>, String> {
            let new_table_name = table_name_row.text().to_string();
            if new_table_name.trim().is_empty() {
                return Err("Table name is required".to_string());
            }
            let cols = columns.borrow();
            let mut edits = Vec::with_capacity(cols.len());
            for c in cols.iter() {
                let edit = c.to_edit();
                if !edit.removed {
                    if edit.name.trim().is_empty() {
                        return Err("Column name is required".to_string());
                    }
                    if edit.data_type.trim().is_empty() {
                        return Err(format!("Type is required for column \"{}\"", edit.name));
                    }
                }
                edits.push(edit);
            }
            Ok(queries::build_alter_table_statements(
                &schema,
                &table,
                new_table_name.trim(),
                &original_columns,
                &edits,
                primary_key_constraint.as_deref(),
            ))
        }
    };

    refresh_preview_btn.connect_clicked(clone!(
        #[strong]
        table_name_row,
        #[strong]
        preview_buffer,
        #[strong]
        error_label,
        #[strong]
        build_statements,
        move |_| match build_statements(&table_name_row) {
            Ok(statements) if statements.is_empty() => {
                error_label.set_visible(false);
                preview_buffer.set_text("-- No changes.");
            }
            Ok(statements) => {
                error_label.set_visible(false);
                preview_buffer.set_text(&format!("{};", statements.join(";\n")));
            }
            Err(err) => {
                error_label.set_label(&err);
                error_label.set_visible(true);
            }
        }
    ));

    let dialog_for_apply = dialog.clone();
    apply_btn.connect_clicked(move |btn| {
        let statements = match build_statements(&table_name_row) {
            Ok(statements) => statements,
            Err(err) => {
                error_label.set_label(&err);
                error_label.set_visible(true);
                return;
            }
        };
        if statements.is_empty() {
            error_label.set_label("No changes to apply.");
            error_label.set_visible(true);
            return;
        }
        preview_buffer.set_text(&format!("{};", statements.join(";\n")));

        let run = {
            let btn = btn.clone();
            let dialog = dialog_for_apply.clone();
            let error_label = error_label.clone();
            let conn_id = conn_id.clone();
            let manager = manager.clone();
            let runtime = runtime.clone();
            let statements = statements.clone();
            let on_altered = on_altered.clone();
            move || {
                btn.set_sensitive(false);
                let task_id = conn_id.clone();
                let task_manager = manager.clone();
                let task_statements = statements.clone();
                let handle = runtime.spawn(async move {
                    let mut mgr = task_manager.lock().await;
                    crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
                    let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
                    queries::alter_table(driver, &task_statements).await
                });

                let dialog = dialog.clone();
                let error_label = error_label.clone();
                let btn = btn.clone();
                let on_altered = on_altered.clone();
                glib::MainContext::default().spawn_local(async move {
                    match handle.await {
                        Ok(Ok(())) => {
                            dialog.close();
                            on_altered();
                        }
                        Ok(Err(err)) => {
                            error_label.set_label(&format!("Failed to alter table: {err}"));
                            error_label.set_visible(true);
                        }
                        Err(_) => {}
                    }
                    btn.set_sensitive(true);
                });
            }
        };

        if queries::alter_table_is_destructive(&statements) {
            let body = format!(
                "This will run the following statements against \"{schema}\".\"{table}\":\n\n{}\n\nDropping a column or changing its type can permanently lose data. This cannot be undone.",
                statements.join(";\n")
            );
            confirm_destructive(&dialog_for_apply, "Apply destructive changes?", &body, "Apply", run);
        } else {
            run();
        }
    });

    dialog.present(Some(parent));
}
