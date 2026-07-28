//! Global search (`Ctrl+P`): search tables, views, columns and functions across one connection
//! at a time, jumping straight to the table detail tab on click.

use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::connection::DbConnection;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, SearchKind, SearchResult};
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

pub fn open(
    parent: &impl IsA<gtk::Widget>,
    connections: Vec<DbConnection>,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
    on_open_table: impl Fn(String, String, String) + 'static,
) {
    if connections.is_empty() {
        return;
    }
    let on_open_table = Rc::new(on_open_table);

    let labels: Vec<&str> = connections.iter().map(|c| c.label.as_str()).collect();
    let conn_model = gtk::StringList::new(&labels);
    let conn_dropdown = gtk::DropDown::builder().model(&conn_model).build();

    let search_entry = gtk::SearchEntry::builder().placeholder_text("Search tables, columns, functions…").hexpand(true).build();

    let header_box = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(6).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).build();
    header_box.append(&conn_dropdown);
    header_box.append(&search_entry);

    let results_list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    let results_scroller = gtk::ScrolledWindow::builder().child(&results_list).vexpand(true).build();

    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    content.append(&header_box);
    content.append(&results_scroller);

    let dialog = adw::Dialog::builder().title("Search").content_width(560).content_height(520).build();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&adw::HeaderBar::new());
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let run_search = move |term: String, selected: u32, results_list: gtk::ListBox| {
        while let Some(child) = results_list.first_child() {
            results_list.remove(&child);
        }
        let Some(conn) = connections.get(selected as usize) else { return };
        if term.trim().len() < 2 {
            return;
        }
        let conn_id = conn.id.clone();
        let task_manager = manager.clone();
        let handle = runtime.spawn(async move {
            let mut mgr = task_manager.lock().await;
            crate::connection_runtime::ensure_connected(&mut mgr, &conn_id).await?;
            let driver = mgr.get_driver(&conn_id).ok_or(CoreError::NotConnected)?;
            queries::global_search(driver, &term).await
        });
        let conn_id_for_task = conn.id.clone();
        let on_open_table_for_task = on_open_table.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(Ok(results)) = handle.await {
                for r in results {
                    results_list.append(&search_result_row(&r, conn_id_for_task.clone(), on_open_table_for_task.clone()));
                }
            }
        });
    };
    let run_search = Rc::new(run_search);

    search_entry.connect_search_changed(clone!(
        #[strong]
        conn_dropdown,
        #[strong]
        results_list,
        #[strong]
        run_search,
        move |entry| {
            run_search(entry.text().to_string(), conn_dropdown.selected(), results_list.clone());
        }
    ));
    conn_dropdown.connect_selected_notify(clone!(
        #[strong]
        search_entry,
        #[strong]
        results_list,
        #[strong]
        run_search,
        move |dropdown| {
            run_search(search_entry.text().to_string(), dropdown.selected(), results_list.clone());
        }
    ));

    dialog.present(Some(parent));
    search_entry.grab_focus();
}

fn search_result_row(r: &SearchResult, conn_id: String, on_open_table: Rc<dyn Fn(String, String, String)>) -> adw::ActionRow {
    let (icon, title, subtitle, target_table) = match r.kind {
        SearchKind::Table => ("view-grid-symbolic", format!("{}.{}", r.schema, r.name), "table".to_string(), Some(r.name.clone())),
        SearchKind::View => ("view-list-symbolic", format!("{}.{}", r.schema, r.name), "view".to_string(), Some(r.name.clone())),
        SearchKind::Column => (
            "text-x-generic-symbolic",
            format!("{}.{}.{}", r.schema, r.table.clone().unwrap_or_default(), r.name),
            format!("column · {}", r.detail.clone().unwrap_or_default()),
            r.table.clone(),
        ),
        SearchKind::Function => ("system-run-symbolic", format!("{}.{}", r.schema, r.name), "function".to_string(), None),
    };
    let row = adw::ActionRow::builder().title(glib::markup_escape_text(&title)).subtitle(subtitle).activatable(target_table.is_some()).build();
    row.add_prefix(&gtk::Image::from_icon_name(icon));
    if let Some(table) = target_table {
        let schema = r.schema.clone();
        row.connect_activated(move |_| {
            on_open_table(conn_id.clone(), schema.clone(), table.clone());
        });
    }
    row
}
