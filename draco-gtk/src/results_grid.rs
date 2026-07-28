//! Query result grid: a native `gtk::ColumnView` (virtualized, handles large result sets far
//! better than a DOM table) whose columns are rebuilt for every query, since each query has a
//! different shape. Rows are stored as `glib::BoxedAnyObject`-wrapped JSON maps rather than a
//! hand-written `GObject` subclass — the column count/order is dynamic per query, so a fixed
//! set of typed properties wouldn't fit.

use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct ExportSnapshot {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}

pub struct ResultsGrid {
    column_view: gtk::ColumnView,
    store: gio::ListStore,
    export_columns: RefCell<Vec<String>>,
    export_rows: RefCell<Vec<serde_json::Map<String, serde_json::Value>>>,
}

impl ResultsGrid {
    pub fn new() -> Self {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::NoSelection::new(Some(store.clone()));
        let column_view = gtk::ColumnView::builder()
            .model(&selection)
            .show_row_separators(true)
            .show_column_separators(true)
            .vexpand(true)
            .build();
        Self {
            column_view,
            store,
            export_columns: RefCell::new(Vec::new()),
            export_rows: RefCell::new(Vec::new()),
        }
    }

    pub fn widget(&self) -> &gtk::ColumnView {
        &self.column_view
    }

    pub fn clear(&self) {
        let columns = self.column_view.columns();
        while columns.n_items() > 0 {
            let col = columns.item(0).and_downcast::<gtk::ColumnViewColumn>().expect("ColumnView column");
            self.column_view.remove_column(&col);
        }
        self.store.remove_all();
        self.export_columns.borrow_mut().clear();
        self.export_rows.borrow_mut().clear();
    }

    pub fn set_data(&self, columns: &[String], rows: Vec<serde_json::Map<String, serde_json::Value>>) {
        self.clear();
        *self.export_columns.borrow_mut() = columns.to_vec();
        *self.export_rows.borrow_mut() = rows.clone();

        for col_name in columns {
            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(|_, list_item| {
                let label = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
                list_item.downcast_ref::<gtk::ListItem>().expect("ListItem").set_child(Some(&label));
            });
            {
                let col_name = col_name.clone();
                factory.connect_bind(move |_, list_item| {
                    let list_item = list_item.downcast_ref::<gtk::ListItem>().expect("ListItem");
                    let obj = list_item.item().and_downcast::<glib::BoxedAnyObject>().expect("BoxedAnyObject row");
                    let map = obj.borrow::<serde_json::Map<String, serde_json::Value>>();
                    let text = map.get(&col_name).map(format_json_value).unwrap_or_default();
                    let label = list_item.child().and_downcast::<gtk::Label>().expect("Label");
                    label.set_text(&text);
                });
            }
            let column = gtk::ColumnViewColumn::new(Some(col_name), Some(factory));
            column.set_resizable(true);
            self.column_view.append_column(&column);
        }

        for row in rows {
            self.store.append(&glib::BoxedAnyObject::new(row));
        }
    }

    pub fn export_snapshot(&self) -> ExportSnapshot {
        ExportSnapshot {
            columns: self.export_columns.borrow().clone(),
            rows: self.export_rows.borrow().clone(),
        }
    }

    pub fn has_rows(&self) -> bool {
        !self.export_rows.borrow().is_empty()
    }
}

impl Default for ResultsGrid {
    fn default() -> Self {
        Self::new()
    }
}

fn format_json_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
