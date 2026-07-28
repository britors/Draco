//! Entity-Relationship Diagram: a `gtk::DrawingArea` canvas drawing tables as boxes and foreign
//! keys as lines between them, with click-drag to move a table or pan the canvas.
//!
//! No precedent in the sibling Lyra OS apps (none of Vega/Beam/Sulafat/Chord has an interactive
//! node-graph widget) — built from scratch on top of `cairo` + `GestureDrag`, the same primitives
//! `dashboard.rs`'s gauges and `sulafat-gtk`'s status dots already use for custom drawing.

use std::cell::{Cell, RefCell};
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

    fn hit_relation(&self, x: f64, y: f64) -> Option<String> {
        self.data.relations.iter().find_map(|rel| {
            let from = self.positions.get(&rel.from_table)?;
            let to = self.positions.get(&rel.to_table)?;
            let start = (from.x + NODE_WIDTH / 2.0, from.y + from.h / 2.0);
            let end = (to.x + NODE_WIDTH / 2.0, to.y + to.h / 2.0);
            if distance_to_segment((x, y), start, end) <= 10.0 {
                Some(rel.to_table.clone())
            } else {
                None
            }
        })
    }

    fn bounds(&self) -> (f64, f64, f64, f64) {
        let min_x = self.positions.values().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let min_y = self.positions.values().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_x = self.positions.values().map(|p| p.x + NODE_WIDTH).fold(0.0, f64::max);
        let max_y = self.positions.values().map(|p| p.y + p.h).fold(0.0, f64::max);
        (min_x, min_y, max_x, max_y)
    }
}

#[derive(Clone, Copy)]
struct Viewport {
    pan_x: f64,
    pan_y: f64,
    zoom: f64,
}

fn distance_to_segment(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
    let (px, py) = point;
    let (sx, sy) = start;
    let (ex, ey) = end;
    let dx = ex - sx;
    let dy = ey - sy;
    if dx == 0.0 && dy == 0.0 {
        return ((px - sx).powi(2) + (py - sy).powi(2)).sqrt();
    }
    let t = (((px - sx) * dx) + ((py - sy) * dy)) / (dx * dx + dy * dy);
    let t = t.clamp(0.0, 1.0);
    let closest = (sx + t * dx, sy + t * dy);
    ((px - closest.0).powi(2) + (py - closest.1).powi(2)).sqrt()
}

fn fit_view(area: &gtk::DrawingArea, model: &Model, viewport: &mut Viewport) {
    let (min_x, min_y, max_x, max_y) = model.bounds();
    let content_width = (max_x - min_x).max(NODE_WIDTH);
    let content_height = (max_y - min_y).max(HEADER_HEIGHT);
    let available_width = (area.width() as f64 - 48.0).max(160.0);
    let available_height = (area.height() as f64 - 48.0).max(120.0);
    viewport.zoom = (available_width / content_width).min(available_height / content_height).clamp(0.35, 1.5);
    viewport.pan_x = (area.width() as f64 - content_width * viewport.zoom) / 2.0 - min_x * viewport.zoom;
    viewport.pan_y = (area.height() as f64 - content_height * viewport.zoom) / 2.0 - min_y * viewport.zoom;
}

fn world_point(viewport: Viewport, x: f64, y: f64) -> (f64, f64) {
    ((x - viewport.pan_x) / viewport.zoom, (y - viewport.pan_y) / viewport.zoom)
}

pub struct ErdView {
    root: gtk::Box,
}

impl ErdView {
    pub fn new(
        conn_id: String,
        schema: String,
        runtime: tokio::runtime::Handle,
        manager: SharedManager,
        on_open_table: Rc<dyn Fn(String, String, String)>,
    ) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        let spinner = gtk::Spinner::builder().spinning(true).margin_top(24).margin_bottom(24).halign(gtk::Align::Center).build();
        root.append(&spinner);

        let task_id = conn_id.clone();
        let task_schema = schema.clone();
        let task_manager = manager.clone();
        let handle = runtime.spawn(async move {
            let mut mgr = task_manager.lock().await;
            crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
            let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
            queries::get_erd_data(driver, &task_schema).await
        });

        let root_for_task = root.clone();
        glib::MainContext::default().spawn_local(async move {
            root_for_task.remove(&spinner);
            match handle.await {
                Ok(Ok(data)) => build_canvas(&root_for_task, data, conn_id, schema, on_open_table),
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

fn build_canvas(root: &gtk::Box, data: ErdData, conn_id: String, schema: String, on_open_table: Rc<dyn Fn(String, String, String)>) {
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
    let viewport = Rc::new(RefCell::new(Viewport { pan_x: 0.0, pan_y: 0.0, zoom: 1.0 }));
    let selected = Rc::new(RefCell::new(None::<String>));

    let area = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();

    area.set_draw_func(clone!(
        #[strong]
        model,
        #[strong]
        viewport,
        #[strong]
        selected,
        move |_, cr, _w, _h| {
            let model = model.borrow();
            let viewport = *viewport.borrow();
            let selected = selected.borrow();
            cr.save().ok();
            cr.translate(viewport.pan_x, viewport.pan_y);
            cr.scale(viewport.zoom, viewport.zoom);

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

                if selected.as_deref() == Some(t.name.as_str()) {
                    cr.set_source_rgb(0.95, 0.75, 0.25);
                } else {
                    cr.set_source_rgb(bg.0, bg.1, bg.2);
                }
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
    let drag_start_viewport: Rc<RefCell<(f64, f64)>> = Rc::new(RefCell::new((0.0, 0.0)));
    let drag_moved = Rc::new(Cell::new(false));

    let gesture = gtk::GestureDrag::new();
    gesture.connect_drag_begin(clone!(
        #[strong]
        model,
        #[strong]
        viewport,
        #[strong]
        dragging,
        #[strong]
        drag_start_layout,
        #[strong]
        drag_start_viewport,
        #[strong]
        drag_moved,
        move |_, x, y| {
            let viewport_value = *viewport.borrow();
            let (world_x, world_y) = world_point(viewport_value, x, y);
            let hit = model.borrow().hit_test(world_x, world_y);
            *drag_start_viewport.borrow_mut() = (viewport_value.pan_x, viewport_value.pan_y);
            drag_moved.set(false);
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
        viewport,
        #[strong]
        dragging,
        #[strong]
        drag_start_layout,
        #[strong]
        drag_start_viewport,
        #[strong]
        drag_moved,
        #[weak]
        area,
        move |_, dx, dy| {
            if dx.abs() > 2.0 || dy.abs() > 2.0 {
                drag_moved.set(true);
            }
            if let Some(name) = dragging.borrow().clone() {
                let (sx, sy) = *drag_start_layout.borrow();
                if let Some(layout) = model.borrow_mut().positions.get_mut(&name) {
                    let zoom = viewport.borrow().zoom;
                    layout.x = sx + dx / zoom;
                    layout.y = sy + dy / zoom;
                }
            } else {
                let (sx, sy) = *drag_start_viewport.borrow();
                let mut viewport = viewport.borrow_mut();
                viewport.pan_x = sx + dx;
                viewport.pan_y = sy + dy;
            }
            area.queue_draw();
        }
    ));

    area.add_controller(gesture);

    let click = gtk::GestureClick::new();
    click.connect_released(clone!(
        #[strong]
        model,
        #[strong]
        viewport,
        #[strong]
        selected,
        #[strong]
        drag_moved,
        #[strong]
        on_open_table,
        #[strong]
        conn_id,
        #[strong]
        schema,
        #[weak]
        area,
        move |_, _, x, y| {
            if drag_moved.get() {
                return;
            }
            let (world_x, world_y) = world_point(*viewport.borrow(), x, y);
            let model = model.borrow();
            let target = model.hit_test(world_x, world_y).or_else(|| model.hit_relation(world_x, world_y));
            *selected.borrow_mut() = target.clone();
            area.queue_draw();
            if let Some(table) = target {
                (on_open_table)(conn_id.clone(), schema.clone(), table);
            }
        }
    ));
    area.add_controller(click);

    let fit_done = Rc::new(Cell::new(false));
    area.connect_resize(clone!(
        #[strong]
        model,
        #[strong]
        viewport,
        #[strong]
        fit_done,
        #[weak]
        area,
        move |_, _, _| {
            if !fit_done.get() {
                fit_view(&area, &model.borrow(), &mut viewport.borrow_mut());
                fit_done.set(true);
                area.queue_draw();
            }
        }
    ));

    let toolbar = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(6).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).build();
    toolbar.append(&gtk::Label::builder().label(format!("{} tables · {} relations", model.borrow().data.tables.len(), model.borrow().data.relations.len())).css_classes(["dim-label"]).hexpand(true).xalign(0.0).build());

    let zoom_label = gtk::Label::builder().label("100%").css_classes(["dim-label"]).build();
    let zoom_out = gtk::Button::builder().label("−").tooltip_text("Zoom out").css_classes(["flat"]).build();
    let zoom_in = gtk::Button::builder().label("+").tooltip_text("Zoom in").css_classes(["flat"]).build();
    let reset_btn = gtk::Button::builder().label("Reset").tooltip_text("Reset zoom and pan").css_classes(["flat"]).build();
    let fit_btn = gtk::Button::builder().label("Fit").tooltip_text("Fit diagram to canvas").css_classes(["flat"]).build();

    zoom_out.connect_clicked(clone!(
        #[strong]
        viewport,
        #[strong]
        zoom_label,
        #[weak]
        area,
        move |_| {
            let mut viewport = viewport.borrow_mut();
            viewport.zoom = (viewport.zoom * 0.8).clamp(0.35, 2.5);
            zoom_label.set_label(&format!("{}%", (viewport.zoom * 100.0).round() as i32));
            area.queue_draw();
        }
    ));
    zoom_in.connect_clicked(clone!(
        #[strong]
        viewport,
        #[strong]
        zoom_label,
        #[weak]
        area,
        move |_| {
            let mut viewport = viewport.borrow_mut();
            viewport.zoom = (viewport.zoom * 1.25).clamp(0.35, 2.5);
            zoom_label.set_label(&format!("{}%", (viewport.zoom * 100.0).round() as i32));
            area.queue_draw();
        }
    ));
    reset_btn.connect_clicked(clone!(
        #[strong]
        viewport,
        #[strong]
        zoom_label,
        #[weak]
        area,
        move |_| {
            *viewport.borrow_mut() = Viewport { pan_x: 0.0, pan_y: 0.0, zoom: 1.0 };
            zoom_label.set_label("100%");
            area.queue_draw();
        }
    ));
    fit_btn.connect_clicked(clone!(
        #[strong]
        model,
        #[strong]
        viewport,
        #[strong]
        zoom_label,
        #[weak]
        area,
        move |_| {
            fit_view(&area, &model.borrow(), &mut viewport.borrow_mut());
            zoom_label.set_label(&format!("{}%", (viewport.borrow().zoom * 100.0).round() as i32));
            area.queue_draw();
        }
    ));
    toolbar.append(&zoom_out);
    toolbar.append(&zoom_in);
    toolbar.append(&zoom_label);
    toolbar.append(&reset_btn);
    toolbar.append(&fit_btn);

    root.append(&toolbar);
    root.append(&area);
}
