//! Visual table creator: pick a schema (existing or new), add columns, preview and run the
//! generated `CREATE TABLE`. No SQL knowledge required, matching the old visual creator.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries;
use gtk::glib;
use gtk::glib::clone;
use sourceview5::prelude::*;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

const COLUMN_TYPES: &[&str] =
    &["text", "varchar(255)", "integer", "bigint", "smallint", "serial", "bigserial", "boolean", "numeric(10,2)", "real", "double precision", "date", "timestamp with time zone", "uuid", "jsonb"];

struct ColumnRow {
    root: gtk::Box,
    name: gtk::Entry,
    type_dropdown: gtk::DropDown,
    nullable: gtk::CheckButton,
    primary_key: gtk::CheckButton,
    unique: gtk::CheckButton,
    default: gtk::Entry,
    remove_btn: gtk::Button,
}

impl ColumnRow {
    fn new() -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(6).build();

        let name = gtk::Entry::builder().placeholder_text("column_name").hexpand(true).build();
        let type_model = gtk::StringList::new(COLUMN_TYPES);
        let type_dropdown = gtk::DropDown::builder().model(&type_model).build();
        let nullable = gtk::CheckButton::builder().label("Null").active(true).build();
        let primary_key = gtk::CheckButton::builder().label("PK").build();
        let unique = gtk::CheckButton::builder().label("Unique").build();
        let default = gtk::Entry::builder().placeholder_text("default (optional)").width_chars(12).build();
        let remove_btn = gtk::Button::builder().icon_name("user-trash-symbolic").css_classes(["flat"]).build();

        root.append(&name);
        root.append(&type_dropdown);
        root.append(&nullable);
        root.append(&primary_key);
        root.append(&unique);
        root.append(&default);
        root.append(&remove_btn);

        Self { root, name, type_dropdown, nullable, primary_key, unique, default, remove_btn }
    }

    fn column_sql(&self) -> Option<String> {
        let name = self.name.text().to_string();
        if name.trim().is_empty() {
            return None;
        }
        let ty = COLUMN_TYPES.get(self.type_dropdown.selected() as usize).copied().unwrap_or("text");
        let mut parts = vec![format!("\"{}\" {}", name.trim().replace('"', "\"\""), ty)];
        if !self.nullable.is_active() {
            parts.push("NOT NULL".to_string());
        }
        if self.unique.is_active() {
            parts.push("UNIQUE".to_string());
        }
        let default = self.default.text().to_string();
        if !default.trim().is_empty() {
            parts.push(format!("DEFAULT {}", default.trim()));
        }
        Some(parts.join(" "))
    }

    fn is_primary_key(&self) -> bool {
        self.primary_key.is_active() && !self.name.text().is_empty()
    }

    fn column_name(&self) -> String {
        self.name.text().to_string()
    }
}

pub fn open(parent: &impl IsA<gtk::Widget>, conn_id: String, schemas: Vec<String>, runtime: tokio::runtime::Handle, manager: SharedManager, on_created: impl Fn() + 'static) {
    let on_created = Rc::new(on_created);

    let schema_names: Vec<String> = if schemas.is_empty() { vec!["public".to_string()] } else { schemas };
    let schema_model = gtk::StringList::new(&schema_names.iter().map(String::as_str).collect::<Vec<_>>());
    let schema_dropdown = gtk::DropDown::builder().model(&schema_model).build();
    let table_name_row = adw::EntryRow::builder().title("Table name").build();

    let basics_group = adw::PreferencesGroup::builder().title("Table").build();
    let schema_row = adw::ActionRow::builder().title("Schema").build();
    schema_row.add_suffix(&schema_dropdown);
    basics_group.add(&schema_row);
    basics_group.add(&table_name_row);

    let columns_list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    let columns: Rc<RefCell<Vec<Rc<ColumnRow>>>> = Rc::new(RefCell::new(Vec::new()));

    let add_column = {
        let columns = columns.clone();
        let columns_list = columns_list.clone();
        move || {
            let col = Rc::new(ColumnRow::new());
            columns_list.append(&col.root);
            columns.borrow_mut().push(col.clone());

            col.remove_btn.connect_clicked(clone!(
                #[strong]
                columns,
                #[strong]
                columns_list,
                #[strong]
                col,
                move |_| {
                    columns_list.remove(&col.root);
                    columns.borrow_mut().retain(|c| !Rc::ptr_eq(c, &col));
                }
            ));
        }
    };
    add_column();
    add_column();

    let add_col_btn = gtk::Button::builder().label("+ Add Column").halign(gtk::Align::Start).build();
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
    let preview_scroller = gtk::ScrolledWindow::builder().child(&preview_view).min_content_height(120).max_content_height(160).build();
    let refresh_preview_btn = gtk::Button::builder().label("Refresh Preview").halign(gtk::Align::Start).build();

    let error_label = gtk::Label::builder().css_classes(["error"]).wrap(true).visible(false).build();

    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    content.append(&error_label);
    content.append(&basics_group);
    content.append(&gtk::Label::builder().label("Columns").xalign(0.0).css_classes(["heading"]).build());
    content.append(&columns_list);
    content.append(&add_col_btn);

    let scroller = gtk::ScrolledWindow::builder().child(&content).vexpand(true).min_content_width(560).build();

    // Pinned to the bottom via `add_bottom_bar` (outside the scroller above) so the generated SQL
    // stays visible no matter how far the column list is scrolled.
    let preview_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(6).margin_bottom(12).margin_start(12).margin_end(12).build();
    preview_box.append(&gtk::Label::builder().label("SQL Preview").xalign(0.0).css_classes(["heading"]).build());
    preview_box.append(&refresh_preview_btn);
    preview_box.append(&preview_scroller);

    let dialog = adw::Dialog::builder().title("New Table").content_width(600).content_height(680).build();
    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let create_btn = gtk::Button::builder().label("Create").css_classes(["suggested-action"]).build();
    let cancel_btn = gtk::Button::builder().label("Cancel").build();
    header.pack_start(&cancel_btn);
    header.pack_end(&create_btn);
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

    let build_sql = {
        let columns = columns.clone();
        let schema_names = schema_names.clone();
        move |schema_dropdown: &gtk::DropDown, table_name_row: &adw::EntryRow| -> Result<String, String> {
            let schema = schema_names.get(schema_dropdown.selected() as usize).cloned().unwrap_or_else(|| "public".to_string());
            let table = table_name_row.text().to_string();
            if table.trim().is_empty() {
                return Err("Table name is required".to_string());
            }
            let cols = columns.borrow();
            let col_defs: Vec<String> = cols.iter().filter_map(|c| c.column_sql()).collect();
            if col_defs.is_empty() {
                return Err("At least one column is required".to_string());
            }
            let mut lines = col_defs;
            let pk_cols: Vec<String> = cols.iter().filter(|c| c.is_primary_key()).map(|c| c.column_name()).collect();
            if !pk_cols.is_empty() {
                let quoted = pk_cols.iter().map(|c| format!("\"{}\"", c.replace('"', "\"\""))).collect::<Vec<_>>().join(", ");
                lines.push(format!("PRIMARY KEY ({quoted})"));
            }
            Ok(format!("CREATE TABLE \"{}\".\"{}\" (\n  {}\n);", schema.replace('"', "\"\""), table.trim().replace('"', "\"\""), lines.join(",\n  ")))
        }
    };

    refresh_preview_btn.connect_clicked(clone!(
        #[strong]
        schema_dropdown,
        #[strong]
        table_name_row,
        #[strong]
        preview_buffer,
        #[strong]
        error_label,
        #[strong]
        build_sql,
        move |_| match build_sql(&schema_dropdown, &table_name_row) {
            Ok(sql) => {
                error_label.set_visible(false);
                preview_buffer.set_text(&sql);
            }
            Err(err) => {
                error_label.set_label(&err);
                error_label.set_visible(true);
            }
        }
    ));

    let dialog_for_create = dialog.clone();
    create_btn.connect_clicked(move |btn| {
        let sql = match build_sql(&schema_dropdown, &table_name_row) {
            Ok(sql) => sql,
            Err(err) => {
                error_label.set_label(&err);
                error_label.set_visible(true);
                return;
            }
        };
        preview_buffer.set_text(&sql);

        btn.set_sensitive(false);
        let task_id = conn_id.clone();
        let task_manager = manager.clone();
        let task_sql = sql.clone();
        let handle = runtime.spawn(async move {
            let mut mgr = task_manager.lock().await;
            crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
            let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
            queries::execute_query(driver, &task_sql).await
        });

        let dialog_for_task = dialog_for_create.clone();
        let error_label_for_task = error_label.clone();
        let btn_for_task = btn.clone();
        let on_created_for_task = on_created.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(_)) => {
                    dialog_for_task.close();
                    on_created_for_task();
                }
                Ok(Err(err)) => {
                    error_label_for_task.set_label(&format!("Failed to create table: {err}"));
                    error_label_for_task.set_visible(true);
                }
                Err(_) => {}
            }
            btn_for_task.set_sensitive(true);
        });
    });

    dialog.present(Some(parent));
}
