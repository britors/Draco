//! Administration tab: roles, pg_cron jobs, activity/locks, extensions and sequences — one page
//! per connection, same "fetch once, render sections" shape as `dashboard.rs`, with per-row
//! action buttons (drop/cancel/toggle/install) that refetch just their own section afterward.

use std::sync::Arc;

use adw::prelude::*;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, ActivityRow, CronJob, ExtensionInfo, LockRow, RoleInfo, SequenceInfo};
use draco_core::secrets;
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

/// Connects (if needed) then runs a query against the connection's driver, on the tokio runtime.
/// Pure text substitution (not generics) — keeps every call site's `Fn`/lifetime story identical
/// to the pattern already proven throughout `explorer.rs`/`dashboard.rs`/etc.
macro_rules! spawn_query {
    ($ctx:expr, |$driver:ident| $body:expr) => {{
        let conn_id = $ctx.conn_id.clone();
        let manager = $ctx.manager.clone();
        $ctx.runtime.spawn(async move {
            let mut mgr = manager.lock().await;
            if mgr.get_driver(&conn_id).is_none() {
                let password = secrets::get_password(&conn_id).await.unwrap_or_default();
                mgr.connect(&conn_id, &password, 30_000, None, None).await?;
            }
            let $driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
            $body
        })
    }};
}

#[derive(Clone)]
struct Ctx {
    conn_id: String,
    runtime: tokio::runtime::Handle,
    manager: SharedManager,
}

pub struct AdminView {
    root: gtk::Box,
}

impl AdminView {
    pub fn new(conn_id: String, runtime: tokio::runtime::Handle, manager: SharedManager) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();

        let scroller = gtk::ScrolledWindow::builder().vexpand(true).build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        scroller.set_child(Some(&content));

        let roles_section = Section::new("Roles");
        let jobs_section = Section::new("Scheduled Jobs (pg_cron)");
        let activity_section = Section::new("Activity");
        let locks_section = Section::new("Locks");
        let extensions_section = Section::new("Extensions");
        let sequences_section = Section::new("Sequences (public)");

        content.append(roles_section.widget());
        content.append(jobs_section.widget());
        content.append(activity_section.widget());
        content.append(locks_section.widget());
        content.append(extensions_section.widget());
        content.append(sequences_section.widget());

        let ctx = Ctx { conn_id, runtime, manager };

        refresh_roles(ctx.clone(), roles_section);
        refresh_jobs(ctx.clone(), jobs_section);
        refresh_activity(ctx.clone(), activity_section);
        refresh_locks(ctx.clone(), locks_section);
        refresh_extensions(ctx.clone(), extensions_section);
        refresh_sequences(ctx.clone(), sequences_section);

        root.append(&scroller);
        Self { root }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

/// A titled `PreferencesGroup` whose row list gets replaced wholesale on every refresh.
#[derive(Clone)]
struct Section {
    group: adw::PreferencesGroup,
    rows: std::rc::Rc<std::cell::RefCell<Vec<adw::ActionRow>>>,
}

impl Section {
    fn new(title: &str) -> Self {
        Self { group: adw::PreferencesGroup::builder().title(title).build(), rows: Default::default() }
    }

    fn widget(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn set_rows(&self, rows: Vec<adw::ActionRow>) {
        for old in self.rows.borrow_mut().drain(..) {
            self.group.remove(&old);
        }
        let rows = if rows.is_empty() { vec![adw::ActionRow::builder().title("—").build()] } else { rows };
        for row in &rows {
            self.group.add(row);
        }
        *self.rows.borrow_mut() = rows;
    }
}

fn refresh_activity(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_activity(d).await);
    glib::MainContext::default().spawn_local(async move {
        if let Ok(Ok(rows)) = handle.await {
            section.set_rows(rows.iter().map(|r| activity_row(r, &ctx, &section)).collect());
        }
    });
}

fn activity_row(r: &ActivityRow, ctx: &Ctx, section: &Section) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(format!("pid {} · {}", r.pid, r.usename.as_deref().unwrap_or("?")))
        .subtitle(format!("{} · {}s · {}", r.state.as_deref().unwrap_or("?"), r.duration.as_deref().unwrap_or("0"), r.query.as_deref().unwrap_or("")))
        .build();
    let cancel_btn = gtk::Button::builder().icon_name("process-stop-symbolic").tooltip_text("Cancel").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&cancel_btn);
    let pid = r.pid;
    cancel_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        move |_| {
            let handle = spawn_query!(ctx, |d| queries::cancel_activity(d, pid).await);
            let ctx = ctx.clone();
            let section = section.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = handle.await;
                refresh_activity(ctx, section);
            });
        }
    ));
    row
}

fn refresh_locks(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_locks(d).await);
    glib::MainContext::default().spawn_local(async move {
        if let Ok(Ok(rows)) = handle.await {
            section.set_rows(rows.iter().map(lock_row).collect());
        }
    });
}

fn lock_row(r: &LockRow) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(format!("pid {} blocked by pid {}", r.blocked_pid, r.blocking_pid))
        .subtitle(format!(
            "{} · waiting {}s · blocked: {} · blocking: {}",
            r.locktype.as_deref().unwrap_or("?"),
            r.wait_sec.as_deref().unwrap_or("0"),
            r.blocked_query.as_deref().unwrap_or(""),
            r.blocking_query.as_deref().unwrap_or("")
        ))
        .build()
}

fn refresh_roles(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_roles(d).await);
    glib::MainContext::default().spawn_local(async move {
        if let Ok(Ok(rows)) = handle.await {
            section.set_rows(rows.iter().map(|r| role_row(r, &ctx, &section)).collect());
        }
    });
}

fn role_row(r: &RoleInfo, ctx: &Ctx, section: &Section) -> adw::ActionRow {
    let mut badges = Vec::new();
    if r.rolsuper {
        badges.push("SUPERUSER");
    }
    if r.rolcreatedb {
        badges.push("CREATEDB");
    }
    if r.rolcanlogin {
        badges.push("LOGIN");
    }
    let row = adw::ActionRow::builder().title(glib::markup_escape_text(&r.rolname)).subtitle(badges.join(", ")).build();
    let drop_btn = gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Drop role").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&drop_btn);
    let name = r.rolname.clone();
    drop_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        #[strong]
        name,
        move |_| {
            let name = name.clone();
            let handle = spawn_query!(ctx, |d| queries::drop_role(d, &name).await);
            let ctx = ctx.clone();
            let section = section.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = handle.await;
                refresh_roles(ctx, section);
            });
        }
    ));
    row
}

fn refresh_extensions(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_extensions(d).await);
    glib::MainContext::default().spawn_local(async move {
        if let Ok(Ok(exts)) = handle.await {
            let installed_names: std::collections::HashSet<&str> = exts.installed.iter().map(|e| e.name.as_str()).collect();
            let mut rows: Vec<adw::ActionRow> = exts.installed.iter().map(|e| extension_row(e, true, &ctx, &section)).collect();
            rows.extend(exts.available.iter().filter(|e| !installed_names.contains(e.name.as_str())).take(30).map(|e| extension_row(e, false, &ctx, &section)));
            section.set_rows(rows);
        }
    });
}

fn extension_row(e: &ExtensionInfo, installed: bool, ctx: &Ctx, section: &Section) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&e.name))
        .subtitle(if installed { format!("installed {}", e.installed_version.as_deref().unwrap_or("")) } else { "available".to_string() })
        .build();
    let btn = gtk::Button::builder()
        .icon_name(if installed { "user-trash-symbolic" } else { "list-add-symbolic" })
        .tooltip_text(if installed { "Drop extension" } else { "Install extension" })
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    row.add_suffix(&btn);
    let name = e.name.clone();
    btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        #[strong]
        name,
        move |_| {
            let name = name.clone();
            let handle = if installed { spawn_query!(ctx, |d| queries::ext_drop(d, &name).await) } else { spawn_query!(ctx, |d| queries::ext_install(d, &name).await) };
            let ctx = ctx.clone();
            let section = section.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = handle.await;
                refresh_extensions(ctx, section);
            });
        }
    ));
    row
}

fn refresh_sequences(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_sequences(d, "public").await);
    glib::MainContext::default().spawn_local(async move {
        if let Ok(Ok(rows)) = handle.await {
            section.set_rows(rows.iter().map(|s| sequence_row(s, &ctx)).collect());
        }
    });
}

fn sequence_row(s: &SequenceInfo, ctx: &Ctx) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(glib::markup_escape_text(&s.name)).build();
    let value_label = gtk::Label::builder().css_classes(["dim-label"]).build();
    row.add_suffix(&value_label);
    let next_btn = gtk::Button::builder().label("Next Value").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&next_btn);
    let name = s.name.clone();
    next_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        name,
        #[strong]
        value_label,
        move |_| {
            let name = name.clone();
            let handle = spawn_query!(ctx, |d| queries::seq_next_val(d, "public", &name).await);
            let value_label = value_label.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(Ok(value)) = handle.await {
                    value_label.set_label(&value);
                }
            });
        }
    ));
    row
}

fn refresh_jobs(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_jobs(d).await);
    glib::MainContext::default().spawn_local(async move {
        match handle.await {
            Ok(Ok(jobs)) if jobs.installed => {
                section.set_rows(jobs.jobs.iter().map(|j| job_row(j, &ctx, &section)).collect());
            }
            Ok(Ok(_)) => {
                section.set_rows(vec![adw::ActionRow::builder().title("pg_cron is not installed on this database").build()]);
            }
            _ => {}
        }
    });
}

fn job_row(j: &CronJob, ctx: &Ctx, section: &Section) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(j.jobname.as_deref().unwrap_or(&j.jobid.to_string())))
        .subtitle(format!("{} · {} · last: {}", j.schedule, j.command, j.last_status.as_deref().unwrap_or("never run")))
        .build();

    let toggle_btn = gtk::Switch::builder().active(j.active).valign(gtk::Align::Center).build();
    row.add_suffix(&toggle_btn);
    let job_id = j.jobid;
    toggle_btn.connect_state_set(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        move |_, active| {
            let handle = spawn_query!(ctx, |d| queries::toggle_job(d, job_id, active).await);
            let ctx = ctx.clone();
            let section = section.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = handle.await;
                refresh_jobs(ctx, section);
            });
            glib::Propagation::Proceed
        }
    ));

    let delete_btn = gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Delete job").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&delete_btn);
    delete_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        move |_| {
            let handle = spawn_query!(ctx, |d| queries::delete_job(d, job_id).await);
            let ctx = ctx.clone();
            let section = section.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = handle.await;
                refresh_jobs(ctx, section);
            });
        }
    ));

    row
}
