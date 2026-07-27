//! Entity-Relationship Diagram: a `gtk::DrawingArea` canvas drawing tables as boxes and foreign
//! keys as lines between them, with click-drag to move a table or pan the canvas.
//!
//! No precedent in the sibling Lyra OS apps (none of Vega/Beam/Sulafat/Chord has an interactive
//! node-graph widget) — built from scratch on top of `cairo` + `GestureDrag`, the same primitives
//! `dashboard.rs`'s gauges and `sulafat-gtk`'s status dots already use for custom drawing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, ErdData};
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

const NODE_WIDTH: f64 = 200.0;
const ROW_HEIGHT: f64 = 18.0;
const HEADER_HEIGHT: f64 = 28.0;
const MAX_VISIBLE_COLS: usize = 10;
const GRID_GAP_X: f64 = 260.0;
const GRID_GAP_Y: f64 = 220.0;

#[derive(Clone, Copy)]
struct NodeLayout {
    x: f64,
    y: f64,
    h: f64,
}

struct Model {
    data: ErdData,
    positions: HashMap<String, NodeLayout>,
}

impl Model {
    fn node_height(cols: usize) -> f64 {
        HEADER_HEIGHT + ROW_HEIGHT * (cols.min(MAX_VISIBLE_COLS) as f64) + if cols > MAX_VISIBLE_COLS { ROW_HEIGHT } else { 0.0 }
    }

    fn hit_test(&self, x: f64, y: f64) -> Option<String> {
        self.data.tables.iter().find_map(|t| {
            let layout = self.positions.get(&t.name)?;
            if x >= layout.x && x <= layout.x + NODE_WIDTH && y >= layout.y && y <= layout.y + layout.h {
                Some(t.name.clone())
            } else {
                None
            }
        })
    }
}

pub struct ErdView {
    root: gtk::Box,
}

impl ErdView {
    pub fn new(conn_id: String, schema: String, runtime: tokio::runtime::Handle, manager: SharedManager) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        let spinner = gtk::Spinner::builder().spinning(true).margin_top(24).margin_bottom(24).halign(gtk::Align::Center).build();
        root.append(&spinner);

        let task_id = conn_id.clone();
        let task_schema = schema.clone();
        let task_manager = manager.clone();
        let handle = runtime.spawn(async move {
            let mut mgr = task_manager.lock().await;
            if mgr.get_driver(&task_id).is_none() {
                let password = draco_core::secrets::get_password(&task_id).await.unwrap_or_default();
                mgr.connect(&task_id, &password, 30_000, None, None).await?;
            }
            let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
            queries::get_erd_data(driver, &task_schema).await
        });

        let root_for_task = root.clone();
        glib::MainContext::default().spawn_local(async move {
            root_for_task.remove(&spinner);
            match handle.await {
                Ok(Ok(data)) => build_canvas(&root_for_task, data),
                Ok(Err(err)) => {
                    root_for_task.append(&adw::StatusPage::builder().icon_name("dialog-error-symbolic").title("Failed to load ERD").description(err.to_string()).build());
                }
                Err(_) => {}
            }
        });

        Self { root }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

fn build_canvas(root: &gtk::Box, data: ErdData) {
    if data.tables.is_empty() {
        root.append(&adw::StatusPage::builder().icon_name("network-workgroup-symbolic").title("No tables in this schema").build());
        return;
    }

    let cols = (data.tables.len() as f64).sqrt().ceil() as usize;
    let mut positions = HashMap::new();
    for (i, t) in data.tables.iter().enumerate() {
        let h = Model::node_height(t.columns.len());
        let col = i % cols.max(1);
        let row = i / cols.max(1);
        positions.insert(t.name.clone(), NodeLayout { x: 40.0 + col as f64 * GRID_GAP_X, y: 40.0 + row as f64 * GRID_GAP_Y, h });
    }

    let model = Rc::new(RefCell::new(Model { data, positions }));
    let pan: Rc<RefCell<(f64, f64)>> = Rc::new(RefCell::new((0.0, 0.0)));

    let area = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();

    area.set_draw_func(clone!(
        #[strong]
        model,
        #[strong]
        pan,
        move |_, cr, _w, _h| {
            let model = model.borrow();
            let (pan_x, pan_y) = *pan.borrow();
            cr.save().ok();
            cr.translate(pan_x, pan_y);

            // Style follows the current libadwaita color scheme rather than hardcoded colors.
            let dark = adw::StyleManager::default().is_dark();
            let (bg, border, text, edge) =
                if dark { ((0.16, 0.16, 0.18), (0.35, 0.35, 0.4), (0.9, 0.9, 0.9), (0.4, 0.6, 0.9)) } else { ((0.98, 0.98, 0.99), (0.7, 0.7, 0.75), (0.1, 0.1, 0.1), (0.2, 0.4, 0.8)) };

            cr.set_line_width(1.5);
            cr.set_source_rgb(edge.0, edge.1, edge.2);
            for rel in &model.data.relations {
                if let (Some(from), Some(to)) = (model.positions.get(&rel.from_table), model.positions.get(&rel.to_table)) {
                    let (fx, fy) = (from.x + NODE_WIDTH / 2.0, from.y + from.h / 2.0);
                    let (tx, ty) = (to.x + NODE_WIDTH / 2.0, to.y + to.h / 2.0);
                    cr.move_to(fx, fy);
                    cr.line_to(tx, ty);
                    let _ = cr.stroke();
                }
            }

            cr.select_font_face("monospace", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
            for t in &model.data.tables {
                let Some(layout) = model.positions.get(&t.name) else { continue };

                cr.set_source_rgb(bg.0, bg.1, bg.2);
                cr.rectangle(layout.x, layout.y, NODE_WIDTH, layout.h);
                let _ = cr.fill_preserve();
                cr.set_source_rgb(border.0, border.1, border.2);
                let _ = cr.stroke();

                cr.set_source_rgb(edge.0, edge.1, edge.2);
                cr.rectangle(layout.x, layout.y, NODE_WIDTH, HEADER_HEIGHT);
                let _ = cr.fill();

                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_font_size(13.0);
                cr.move_to(layout.x + 8.0, layout.y + 19.0);
                let _ = cr.show_text(&t.name);

                cr.set_source_rgb(text.0, text.1, text.2);
                cr.set_font_size(11.0);
                for (i, col) in t.columns.iter().take(MAX_VISIBLE_COLS).enumerate() {
                    let y = layout.y + HEADER_HEIGHT + ROW_HEIGHT * (i as f64) + 13.0;
                    let mark = if col.is_pk { "PK " } else if col.is_fk { "FK " } else { "   " };
                    cr.move_to(layout.x + 8.0, y);
                    let _ = cr.show_text(&format!("{mark}{}", col.name));
                }
                if t.columns.len() > MAX_VISIBLE_COLS {
                    let y = layout.y + HEADER_HEIGHT + ROW_HEIGHT * (MAX_VISIBLE_COLS as f64) + 13.0;
                    cr.move_to(layout.x + 8.0, y);
                    let _ = cr.show_text(&format!("… +{} more", t.columns.len() - MAX_VISIBLE_COLS));
                }
            }

            cr.restore().ok();
        }
    ));

    let dragging: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let drag_start_layout: Rc<RefCell<(f64, f64)>> = Rc::new(RefCell::new((0.0, 0.0)));
    let drag_start_pan: Rc<RefCell<(f64, f64)>> = Rc::new(RefCell::new((0.0, 0.0)));

    let gesture = gtk::GestureDrag::new();
    gesture.connect_drag_begin(clone!(
        #[strong]
        model,
        #[strong]
        pan,
        #[strong]
        dragging,
        #[strong]
        drag_start_layout,
        #[strong]
        drag_start_pan,
        move |_, x, y| {
            let (pan_x, pan_y) = *pan.borrow();
            let hit = model.borrow().hit_test(x - pan_x, y - pan_y);
            *drag_start_pan.borrow_mut() = (pan_x, pan_y);
            if let Some(name) = hit {
                let layout = model.borrow().positions[&name];
                *drag_start_layout.borrow_mut() = (layout.x, layout.y);
                *dragging.borrow_mut() = Some(name);
            } else {
                *dragging.borrow_mut() = None;
            }
        }
    ));

    gesture.connect_drag_update(clone!(
        #[strong]
        model,
        #[strong]
        pan,
        #[strong]
        dragging,
        #[strong]
        drag_start_layout,
        #[strong]
        drag_start_pan,
        #[weak]
        area,
        move |_, dx, dy| {
            if let Some(name) = dragging.borrow().clone() {
                let (sx, sy) = *drag_start_layout.borrow();
                if let Some(layout) = model.borrow_mut().positions.get_mut(&name) {
                    layout.x = sx + dx;
                    layout.y = sy + dy;
                }
            } else {
                let (sx, sy) = *drag_start_pan.borrow();
                *pan.borrow_mut() = (sx + dx, sy + dy);
            }
            area.queue_draw();
        }
    ));

    area.add_controller(gesture);

    let toolbar = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(6).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).build();
    toolbar.append(&gtk::Label::builder().label(format!("{} tables · {} relations — drag a table to move it, drag empty space to pan", model.borrow().data.tables.len(), model.borrow().data.relations.len())).css_classes(["dim-label"]).build());

    root.append(&toolbar);
    root.append(&area);
}
