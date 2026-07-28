//! Connection dashboard tab: server/database info, arc gauges (cache hit, connection usage,
//! rollback rate) drawn with `cairo` (no chart library — same precedent as
//! `vega-gtk::ui::sparkline`), top tables, and a "Health" section (bloat, unused indexes,
//! sequential-scan hot spots) from `draco-core::postgres::queries::get_db_stats`.

use std::f64::consts::PI;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::error::CoreError;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, BloatRow, DashboardData, DbStats, SeqScanRow, TopTable, UnusedIndexRow};
use gtk::glib;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

pub struct DashboardView {
    root: gtk::Box,
}

impl DashboardView {
    pub fn new(conn_id: String, runtime: tokio::runtime::Handle, manager: SharedManager) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();

        let spinner = gtk::Spinner::builder().spinning(true).margin_top(24).margin_bottom(24).halign(gtk::Align::Center).build();
        root.append(&spinner);

        let task_id = conn_id.clone();
        let task_manager = manager.clone();
        let handle = runtime.spawn(async move {
            let mut mgr = task_manager.lock().await;
            crate::connection_runtime::ensure_connected(&mut mgr, &task_id).await?;
            let driver = mgr.get_driver(&task_id).ok_or(CoreError::NotConnected)?;
            let dashboard = queries::get_dashboard(driver).await?;
            let stats = queries::get_db_stats(driver).await?;
            Ok::<_, CoreError>((dashboard, stats))
        });

        let root_for_task = root.clone();
        glib::MainContext::default().spawn_local(async move {
            root_for_task.remove(&spinner);
            match handle.await {
                Ok(Ok((dashboard, stats))) => populate(&root_for_task, dashboard, stats),
                Ok(Err(err)) => {
                    root_for_task.append(
                        &adw::StatusPage::builder().icon_name("dialog-error-symbolic").title("Failed to load dashboard").description(err.to_string()).build(),
                    );
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

fn populate(root: &gtk::Box, dash: DashboardData, stats: DbStats) {
    let scroller = gtk::ScrolledWindow::builder().vexpand(true).build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    // ── Server / database info ──────────────────────────────────────────────
    let info_group = adw::PreferencesGroup::builder().title(&dash.db_name).description(format!("PostgreSQL {}", dash.pg_version)).build();
    info_group.add(&info_row("Host", &format!("{}:{}", dash.host, dash.port)));
    info_group.add(&info_row("Uptime", &dash.uptime));
    info_group.add(&info_row("Size", &dash.db_size));
    info_group.add(&info_row("Encoding / Collation", &format!("{} / {}", dash.encoding, dash.collation)));
    content.append(&info_group);

    // ── Gauges ───────────────────────────────────────────────────────────────
    let cache_hit: f64 = dash.cache_hit.parse().unwrap_or(0.0);
    let conn_usage = if dash.max_conn > 0 { dash.total_conn as f64 / dash.max_conn as f64 * 100.0 } else { 0.0 };
    let total_xact = dash.commits + dash.rollbacks;
    let rollback_rate = if total_xact > 0 { dash.rollbacks as f64 / total_xact as f64 * 100.0 } else { 0.0 };

    let gauges = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(24).halign(gtk::Align::Center).build();
    gauges.append(&arc_gauge(cache_hit, "Cache Hit", false));
    gauges.append(&arc_gauge(conn_usage, "Connections", false));
    gauges.append(&arc_gauge(rollback_rate, "Rollback Rate", true));
    content.append(&gauges);

    // ── KPIs ─────────────────────────────────────────────────────────────────
    let kpis = adw::PreferencesGroup::new();
    kpis.add(&info_row("Active / Idle / Idle-in-TX", &format!("{} / {} / {}", dash.active_conn, dash.idle_conn, dash.idle_in_tx_conn)));
    kpis.add(&info_row("Deadlocks", &dash.deadlocks.to_string()));
    kpis.add(&info_row("Temp Files", &dash.temp_files.to_string()));
    kpis.add(&info_row("Commits / Rollbacks", &format!("{} / {}", dash.commits, dash.rollbacks)));
    content.append(&kpis);

    // ── Top tables ───────────────────────────────────────────────────────────
    if !dash.top_tables.is_empty() {
        content.append(&section_label("Top 10 Largest Tables"));
        content.append(&top_tables_group(&dash.top_tables));
    }

    // ── Health ───────────────────────────────────────────────────────────────
    if !stats.bloat.is_empty() {
        content.append(&section_label("Table Bloat"));
        content.append(&bloat_group(&stats.bloat));
    }
    if !stats.unused_idx.is_empty() {
        content.append(&section_label("Unused Indexes"));
        content.append(&unused_idx_group(&stats.unused_idx));
    }
    if !stats.seq_scans.is_empty() {
        content.append(&section_label("Sequential Scan Hot Spots"));
        content.append(&seq_scans_group(&stats.seq_scans));
    }

    scroller.set_child(Some(&content));
    root.append(&scroller);
}

fn section_label(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).css_classes(["heading"]).margin_top(6).build()
}

fn info_row(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder().title(title).subtitle(value).build()
}

/// `higher_is_worse` flips the green/yellow/red thresholds — a high cache hit ratio is healthy,
/// a high rollback rate is not.
fn arc_gauge(value_pct: f64, label: &str, higher_is_worse: bool) -> gtk::Box {
    let value = value_pct.clamp(0.0, 100.0);
    let severity = if higher_is_worse { value } else { 100.0 - value };
    let (r, g, b) = if severity >= 70.0 {
        (0.86, 0.20, 0.20)
    } else if severity >= 40.0 {
        (0.90, 0.70, 0.15)
    } else {
        (0.20, 0.70, 0.35)
    };

    let area = gtk::DrawingArea::builder().content_width(120).content_height(70).build();
    area.set_draw_func(move |_, cr, w, h| {
        let cx = f64::from(w) / 2.0;
        let cy = f64::from(h) - 6.0;
        let radius = f64::from(w).min(f64::from(h) * 2.0) / 2.0 - 10.0;
        let start = PI;
        let end = 2.0 * PI;

        cr.set_line_width(10.0);
        cr.set_line_cap(gtk::cairo::LineCap::Round);

        cr.set_source_rgba(0.5, 0.5, 0.5, 0.25);
        cr.arc(cx, cy, radius, start, end);
        let _ = cr.stroke();

        let value_end = start + (end - start) * (value / 100.0);
        cr.set_source_rgb(r, g, b);
        cr.arc(cx, cy, radius, start, value_end);
        let _ = cr.stroke();
    });

    let value_label = gtk::Label::builder().label(format!("{value:.0}%")).css_classes(["title-3"]).halign(gtk::Align::Center).build();
    let name_label = gtk::Label::builder().label(label).css_classes(["dim-label"]).halign(gtk::Align::Center).build();

    let container = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).build();
    container.append(&area);
    container.append(&value_label);
    container.append(&name_label);
    container
}

fn size_bar(fraction: f64) -> gtk::Widget {
    let fraction = fraction.clamp(0.0, 1.0);
    let area = gtk::DrawingArea::builder().content_width(100).content_height(10).valign(gtk::Align::Center).build();
    area.set_draw_func(move |_, cr, w, h| {
        cr.set_source_rgba(0.5, 0.5, 0.5, 0.2);
        cr.rectangle(0.0, 0.0, f64::from(w), f64::from(h));
        let _ = cr.fill();
        cr.set_source_rgb(0.30, 0.55, 0.90);
        cr.rectangle(0.0, 0.0, f64::from(w) * fraction, f64::from(h));
        let _ = cr.fill();
    });
    area.upcast()
}

fn top_tables_group(tables: &[TopTable]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    let max_bytes = tables.iter().map(|t| t.raw_bytes).max().unwrap_or(1).max(1) as f64;
    for t in tables {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&format!("{}.{}", t.schema, t.table)))
            .subtitle(format!("{} rows (live)", t.n_live_tup.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string())))
            .build();
        row.add_suffix(&size_bar(t.raw_bytes as f64 / max_bytes));
        row.add_suffix(&gtk::Label::builder().label(&t.total_size).css_classes(["dim-label"]).width_chars(10).build());
        group.add(&row);
    }
    group
}

fn bloat_group(rows: &[BloatRow]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    for r in rows {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&format!("{}.{}", r.schema, r.table)))
            .subtitle(format!(
                "{}% dead tuples · {} live / {} dead · last autovacuum {}",
                r.bloat_pct,
                r.n_live_tup.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string()),
                r.n_dead_tup.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string()),
                r.last_autovacuum.as_deref().unwrap_or("never")
            ))
            .build();
        group.add(&row);
    }
    group
}

fn unused_idx_group(rows: &[UnusedIndexRow]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    for r in rows {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&r.index))
            .subtitle(format!("{}.{} · {}", r.schema, r.table, r.size))
            .build();
        group.add(&row);
    }
    group
}

fn seq_scans_group(rows: &[SeqScanRow]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    for r in rows {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&format!("{}.{}", r.schema, r.table)))
            .subtitle(format!(
                "{} sequential scans · {} rows · {}",
                r.seq_scan,
                r.n_live_tup.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string()),
                r.size
            ))
            .build();
        group.add(&row);
    }
    group
}
