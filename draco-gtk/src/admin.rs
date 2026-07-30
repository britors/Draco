//! Administration tab: roles, pg_cron jobs, activity/locks, extensions, query stats and
//! sequences — one page per connection, same "fetch once, render sections" shape as
//! `dashboard.rs`, with per-row action buttons (drop/cancel/toggle/install) that refetch just
//! their own section afterward.

use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::manager::ConnectionManager;
use draco_core::postgres::queries::{self, ActivityRow, CronJob, ExtensionInfo, JobRun, LockRow, NewRole, QueryStatRow, RoleInfo, SequenceInfo};
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

use crate::confirm::confirm_destructive;

type SharedManager = Arc<Mutex<ConnectionManager>>;
type OpenQuerySql = Rc<dyn Fn(String, String, String)>;
type OpenAiMessage = Rc<dyn Fn(String, String)>;

/// Connects (if needed) then runs a query against the connection's driver, on the tokio runtime.
/// Pure text substitution (not generics) — keeps every call site's `Fn`/lifetime story identical
/// to the pattern already proven throughout `explorer.rs`/`dashboard.rs`/etc.
macro_rules! spawn_query {
    ($ctx:expr, |$driver:ident| $body:expr) => {{
        let conn_id = $ctx.conn_id.clone();
        let manager = $ctx.manager.clone();
        $ctx.runtime.spawn(async move {
            let mut mgr = manager.lock().await;
            crate::connection_runtime::ensure_connected(&mut mgr, &conn_id).await?;
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
    toasts: adw::ToastOverlay,
    open_query: OpenQuerySql,
    open_ai: OpenAiMessage,
}

pub struct AdminView {
    root: gtk::Box,
}

/// Wraps a single section's `PreferencesGroup` in its own scrollable `AdwViewStack` page, so each
/// section (Roles, Jobs, ...) scrolls independently instead of everything sharing one long page.
fn stack_page(section: &Section) -> gtk::ScrolledWindow {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(section.widget());
    gtk::ScrolledWindow::builder().child(&content).vexpand(true).build()
}

impl AdminView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        conn_id: String,
        runtime: tokio::runtime::Handle,
        manager: SharedManager,
        toasts: adw::ToastOverlay,
        open_query: OpenQuerySql,
        open_ai: OpenAiMessage,
    ) -> Self {
        let root = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();

        let roles_section = Section::new("Roles");
        let new_role_btn = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("New role").css_classes(["flat"]).valign(gtk::Align::Center).build();
        roles_section.widget().set_header_suffix(Some(&new_role_btn));
        let jobs_section = Section::new("Scheduled Jobs (pg_cron)");
        let new_job_btn = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("New scheduled job").css_classes(["flat"]).valign(gtk::Align::Center).build();
        jobs_section.widget().set_header_suffix(Some(&new_job_btn));
        let activity_section = Section::new("Activity");
        let locks_section = Section::new("Locks");
        let extensions_section = Section::new("Extensions");
        let query_stats_section = Section::new("Query Stats (pg_stat_statements)");
        let reset_stats_btn = gtk::Button::builder().icon_name("edit-clear-all-symbolic").tooltip_text("Reset stats").css_classes(["flat"]).valign(gtk::Align::Center).build();
        query_stats_section.widget().set_header_suffix(Some(&reset_stats_btn));
        let sequences_section = Section::new("Sequences (public)");

        let stack = adw::ViewStack::new();
        stack.add_titled_with_icon(&stack_page(&roles_section), Some("roles"), "Roles", "system-users-symbolic");
        stack.add_titled_with_icon(&stack_page(&jobs_section), Some("jobs"), "Jobs", "alarm-symbolic");
        stack.add_titled_with_icon(&stack_page(&activity_section), Some("activity"), "Activity", "system-run-symbolic");
        stack.add_titled_with_icon(&stack_page(&locks_section), Some("locks"), "Locks", "changes-prevent-symbolic");
        stack.add_titled_with_icon(&stack_page(&extensions_section), Some("extensions"), "Extensions", "application-x-addon-symbolic");
        stack.add_titled_with_icon(&stack_page(&query_stats_section), Some("query-stats"), "Query Stats", "view-sort-descending-symbolic");
        stack.add_titled_with_icon(&stack_page(&sequences_section), Some("sequences"), "Sequences", "view-list-ordered-symbolic");

        let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).halign(gtk::Align::Center).build();
        // Every section only (re)loads on tab open or right after one of its own mutating
        // actions — DDL run from outside Draco (another session, a DBA, `psql`) never shows up
        // until something triggers a refetch. This button re-runs every section's `refresh_*` at
        // once instead of requiring the whole Admin tab to be closed and reopened.
        let refresh_all_btn = gtk::Button::builder().icon_name("view-refresh-symbolic").tooltip_text("Refresh all").css_classes(["flat"]).valign(gtk::Align::Center).build();
        let switcher_bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .halign(gtk::Align::Center)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        switcher_bar.append(&switcher);
        switcher_bar.append(&refresh_all_btn);

        root.append(&switcher_bar);
        root.append(&stack);

        let ctx = Ctx { conn_id, runtime, manager, toasts, open_query, open_ai };

        reset_stats_btn.connect_clicked(clone!(
            #[strong]
            ctx,
            #[strong]
            query_stats_section,
            move |_| {
                let handle = spawn_query!(ctx, |d| queries::reset_query_stats(d).await);
                let ctx = ctx.clone();
                let section = query_stats_section.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = handle.await;
                    refresh_query_stats(ctx, section);
                });
            }
        ));

        new_role_btn.connect_clicked(clone!(
            #[strong]
            ctx,
            #[strong]
            roles_section,
            move |btn| {
                let Some(parent) = btn.root() else { return };
                open_new_role_dialog(&parent, ctx.clone(), roles_section.clone());
            }
        ));
        new_job_btn.connect_clicked(clone!(
            #[strong]
            ctx,
            #[strong]
            jobs_section,
            move |btn| {
                let Some(parent) = btn.root() else { return };
                open_new_job_dialog(&parent, ctx.clone(), jobs_section.clone());
            }
        ));

        refresh_all_btn.connect_clicked(clone!(
            #[strong]
            ctx,
            #[strong]
            roles_section,
            #[strong]
            jobs_section,
            #[strong]
            activity_section,
            #[strong]
            locks_section,
            #[strong]
            extensions_section,
            #[strong]
            query_stats_section,
            #[strong]
            sequences_section,
            move |_| {
                refresh_roles(ctx.clone(), roles_section.clone());
                refresh_jobs(ctx.clone(), jobs_section.clone());
                refresh_activity(ctx.clone(), activity_section.clone());
                refresh_locks(ctx.clone(), locks_section.clone());
                refresh_extensions(ctx.clone(), extensions_section.clone());
                refresh_query_stats(ctx.clone(), query_stats_section.clone());
                refresh_sequences(ctx.clone(), sequences_section.clone());
            }
        ));

        refresh_roles(ctx.clone(), roles_section);
        refresh_jobs(ctx.clone(), jobs_section);
        refresh_activity(ctx.clone(), activity_section);
        refresh_locks(ctx.clone(), locks_section);
        refresh_extensions(ctx.clone(), extensions_section);
        refresh_query_stats(ctx.clone(), query_stats_section);
        refresh_sequences(ctx.clone(), sequences_section);

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

/// Icon reflecting `state` (`pg_stat_activity.state`): actively running vs. idle/idle-in-transaction/
/// waiting-on-a-lock, so the list reads at a glance instead of requiring the subtitle text.
fn activity_state_icon(state: Option<&str>) -> &'static str {
    match state {
        Some("active") => "media-playback-start-symbolic",
        Some(s) if s.starts_with("idle in transaction") => "dialog-warning-symbolic",
        Some("idle") => "media-playback-pause-symbolic",
        _ => "system-run-symbolic",
    }
}

fn activity_row(r: &ActivityRow, ctx: &Ctx, section: &Section) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(format!("pid {} · {}", r.pid, r.usename.as_deref().unwrap_or("?")))
        .subtitle(format!("{} · {}s · {}", r.state.as_deref().unwrap_or("?"), r.duration.as_deref().unwrap_or("0"), r.query.as_deref().unwrap_or("")))
        .build();
    row.add_prefix(&gtk::Image::from_icon_name(activity_state_icon(r.state.as_deref())));
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

fn refresh_query_stats(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_query_stats(d).await);
    glib::MainContext::default().spawn_local(async move {
        match handle.await {
            Ok(Ok(stats)) if stats.installed => {
                section.set_rows(stats.queries.iter().map(|r| query_stat_row(r, &ctx)).collect());
            }
            Ok(Ok(_)) => {
                let row = adw::ActionRow::builder().title("pg_stat_statements is not installed on this database").build();
                let enable_btn = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("Enable extension").valign(gtk::Align::Center).css_classes(["flat"]).build();
                row.add_suffix(&enable_btn);
                enable_btn.connect_clicked(clone!(
                    #[strong]
                    ctx,
                    #[strong]
                    section,
                    move |_| {
                        let handle = spawn_query!(ctx, |d| queries::ext_install(d, "pg_stat_statements").await);
                        let ctx = ctx.clone();
                        let section = section.clone();
                        glib::MainContext::default().spawn_local(async move {
                            let _ = handle.await;
                            refresh_query_stats(ctx, section);
                        });
                    }
                ));
                section.set_rows(vec![row]);
            }
            Ok(Err(err)) => section.set_rows(vec![adw::ActionRow::builder().title("Unable to load query stats").subtitle(err.to_string()).build()]),
            Err(_) => section.set_rows(vec![adw::ActionRow::builder().title("Query stats loading task failed").build()]),
        }
    });
}

/// First line of `query`, truncated to a tab-title-friendly length.
fn short_title(query: &str) -> String {
    let first_line = query.lines().next().unwrap_or("").trim();
    if first_line.chars().count() > 40 {
        format!("{}…", first_line.chars().take(40).collect::<String>())
    } else if first_line.is_empty() {
        "Query".to_string()
    } else {
        first_line.to_string()
    }
}

fn query_stat_row(r: &QueryStatRow, ctx: &Ctx) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&r.query))
        .subtitle(format!("{} calls · {:.1}ms avg · {:.0}ms total · {} rows", r.calls, r.mean_exec_ms, r.total_exec_ms, r.rows))
        .build();

    let open_query_btn = gtk::Button::builder().icon_name("media-playback-start-symbolic").tooltip_text("Open in new Query tab").valign(gtk::Align::Center).css_classes(["flat"]).build();
    let conn_id = ctx.conn_id.clone();
    let query = r.query.clone();
    let open_query = ctx.open_query.clone();
    open_query_btn.connect_clicked(move |_| {
        open_query(conn_id.clone(), query.clone(), short_title(&query));
    });
    row.add_suffix(&open_query_btn);

    let analyze_btn = gtk::Button::builder().icon_name("chat-message-new-symbolic").tooltip_text("Analisar com IA").valign(gtk::Align::Center).css_classes(["flat"]).build();
    let conn_id = ctx.conn_id.clone();
    let message = format!(
        "Analise esta query quanto a performance. Estatísticas do pg_stat_statements: {} execuções, {:.1}ms em média, {:.0}ms no total, {} linhas retornadas no total.\n\n```sql\n{}\n```",
        r.calls, r.mean_exec_ms, r.total_exec_ms, r.rows, r.query
    );
    let open_ai = ctx.open_ai.clone();
    analyze_btn.connect_clicked(move |_| {
        open_ai(conn_id.clone(), message.clone());
    });
    row.add_suffix(&analyze_btn);

    row
}

fn open_new_role_dialog(parent: &impl IsA<gtk::Widget>, ctx: Ctx, section: Section) {
    let name_row = adw::EntryRow::builder().title("Role name").build();
    let password_row = adw::PasswordEntryRow::builder().title("Password (optional)").build();
    let login_row = adw::SwitchRow::builder().title("Can login").subtitle("LOGIN").active(true).build();
    let createdb_row = adw::SwitchRow::builder().title("Create databases").subtitle("CREATEDB").build();
    let createrole_row = adw::SwitchRow::builder().title("Create roles").subtitle("CREATEROLE").build();
    let superuser_row = adw::SwitchRow::builder().title("Superuser").subtitle("SUPERUSER").build();
    let conn_limit_row = adw::EntryRow::builder().title("Connection limit (-1 = unlimited)").text("-1").build();
    let valid_until_row = adw::EntryRow::builder().title("Valid until (YYYY-MM-DD, empty = infinity)").build();

    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    group.add(&password_row);
    group.add(&login_row);
    group.add(&createdb_row);
    group.add(&createrole_row);
    group.add(&superuser_row);
    group.add(&conn_limit_row);
    group.add(&valid_until_row);

    let error_label = gtk::Label::builder().css_classes(["error"]).wrap(true).visible(false).build();

    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    content.append(&error_label);
    content.append(&group);

    let scroller = gtk::ScrolledWindow::builder().child(&content).propagate_natural_height(true).min_content_width(420).build();

    let dialog = adw::Dialog::builder().title("New Role").content_width(460).content_height(520).build();
    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let create_btn = gtk::Button::builder().label("Create").css_classes(["suggested-action"]).build();
    let cancel_btn = gtk::Button::builder().label("Cancel").build();
    header.pack_start(&cancel_btn);
    header.pack_end(&create_btn);
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));
    dialog.set_child(Some(&toolbar_view));

    cancel_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));

    let dialog_for_create = dialog.clone();
    create_btn.connect_clicked(move |btn| {
        let name = name_row.text().to_string();
        if name.trim().is_empty() {
            error_label.set_label("Role name is required");
            error_label.set_visible(true);
            return;
        }
        let password = password_row.text().to_string();
        let conn_limit = match conn_limit_row.text().parse::<i32>() {
            Ok(value) if value >= -1 => value,
            _ => {
                error_label.set_label("Connection limit must be an integer >= -1");
                error_label.set_visible(true);
                return;
            }
        };
        let valid_until = valid_until_row.text().trim().to_string();
        let new_role = NewRole {
            name: name.trim().to_string(),
            password: if password.is_empty() { None } else { Some(password) },
            login: login_row.is_active(),
            createdb: createdb_row.is_active(),
            createrole: createrole_row.is_active(),
            superuser: superuser_row.is_active(),
            conn_limit,
            valid_until: (!valid_until.is_empty()).then_some(valid_until),
        };

        btn.set_sensitive(false);
        let handle = spawn_query!(ctx, |d| queries::create_role(d, &new_role).await);

        let dialog = dialog_for_create.clone();
        let error_label = error_label.clone();
        let btn = btn.clone();
        let ctx_for_refresh = ctx.clone();
        let section = section.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(())) => {
                    dialog.close();
                    let toasts = ctx_for_refresh.toasts.clone();
                    refresh_roles(ctx_for_refresh, section);
                    toasts.add_toast(adw::Toast::new("Role created"));
                }
                Ok(Err(err)) => {
                    error_label.set_label(&format!("Failed to create role: {err}"));
                    error_label.set_visible(true);
                }
                Err(_) => {}
            }
            btn.set_sensitive(true);
        });
    });

    dialog.present(Some(parent));
}

fn refresh_roles(ctx: Ctx, section: Section) {
    let handle = spawn_query!(ctx, |d| queries::get_roles(d).await);
    glib::MainContext::default().spawn_local(async move {
        match handle.await {
            Ok(Ok(rows)) => section.set_rows(rows.iter().map(|r| role_row(r, &ctx, &section)).collect()),
            Ok(Err(err)) => section.set_rows(vec![adw::ActionRow::builder().title("Unable to load roles").subtitle(err.to_string()).build()]),
            Err(_) => section.set_rows(vec![adw::ActionRow::builder().title("Role loading task failed").build()]),
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
    let mut subtitle = badges.join(", ");
    if !subtitle.is_empty() {
        subtitle.push_str(" · ");
    }
    subtitle.push_str(&format!("connections: {} · valid until: {}", r.rolconnlimit, r.rolvaliduntil.as_deref().unwrap_or("infinity")));
    let row = adw::ActionRow::builder().title(glib::markup_escape_text(&r.rolname)).subtitle(subtitle).build();
    let edit_btn = gtk::Button::builder().icon_name("document-edit-symbolic").tooltip_text("Edit role").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&edit_btn);
    let role = r.clone();
    edit_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        move |btn| {
            let Some(parent) = btn.root() else { return };
            open_edit_role_dialog(&parent, ctx.clone(), section.clone(), role.clone());
        }
    ));
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
        move |btn| {
            let Some(parent) = btn.root() else { return };
            let ctx = ctx.clone();
            let section = section.clone();
            let name = name.clone();
            confirm_destructive(
                &parent,
                "Drop role?",
                &format!("This permanently removes the role \u{201c}{name}\u{201d}. This cannot be undone."),
                "Drop Role",
                move || {
                    let handle = spawn_query!(ctx, |d| queries::drop_role(d, &name).await);
                    glib::MainContext::default().spawn_local(async move {
                        match handle.await {
                            Ok(Ok(())) => {
                                let toasts = ctx.toasts.clone();
                                refresh_roles(ctx, section);
                                toasts.add_toast(adw::Toast::new("Role dropped"));
                            }
                            Ok(Err(err)) => section.set_rows(vec![adw::ActionRow::builder().title("Failed to drop role").subtitle(err.to_string()).build()]),
                            Err(_) => section.set_rows(vec![adw::ActionRow::builder().title("Role operation task failed").build()]),
                        }
                    });
                },
            );
        }
    ));
    row
}

fn open_edit_role_dialog(parent: &impl IsA<gtk::Widget>, ctx: Ctx, section: Section, role: RoleInfo) {
    let password_row = adw::PasswordEntryRow::builder().title("New password (optional)").build();
    let login_row = adw::SwitchRow::builder().title("Can login").active(role.rolcanlogin).build();
    let createdb_row = adw::SwitchRow::builder().title("Create databases").active(role.rolcreatedb).build();
    let createrole_row = adw::SwitchRow::builder().title("Create roles").active(role.rolcreaterole).build();
    let superuser_row = adw::SwitchRow::builder().title("Superuser").active(role.rolsuper).build();
    let conn_limit_row = adw::EntryRow::builder().title("Connection limit (-1 = unlimited)").text(role.rolconnlimit.to_string()).build();
    let valid_until_row = adw::EntryRow::builder().title("Valid until (YYYY-MM-DD, empty = infinity)").text(role.rolvaliduntil.as_deref().unwrap_or("")).build();
    let group = adw::PreferencesGroup::new();
    group.add(&password_row);
    group.add(&login_row);
    group.add(&createdb_row);
    group.add(&createrole_row);
    group.add(&superuser_row);
    group.add(&conn_limit_row);
    group.add(&valid_until_row);
    let error_label = gtk::Label::builder().css_classes(["error"]).wrap(true).visible(false).build();
    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    content.append(&error_label);
    content.append(&group);
    let scroller = gtk::ScrolledWindow::builder().child(&content).propagate_natural_height(true).min_content_width(420).build();
    let dialog = adw::Dialog::builder().title(format!("Edit Role: {}", role.rolname)).content_width(460).content_height(520).build();
    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let save_btn = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
    let cancel_btn = gtk::Button::builder().label("Cancel").build();
    header.pack_start(&cancel_btn);
    header.pack_end(&save_btn);
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));
    dialog.set_child(Some(&toolbar_view));
    cancel_btn.connect_clicked(clone!(#[weak] dialog, move |_| {
        dialog.close();
    }));
    let dialog_for_save = dialog.clone();
    save_btn.connect_clicked(move |btn| {
        let conn_limit = match conn_limit_row.text().parse::<i32>() {
            Ok(value) if value >= -1 => value,
            _ => {
                error_label.set_label("Connection limit must be an integer >= -1");
                error_label.set_visible(true);
                return;
            }
        };
        let valid_until = valid_until_row.text().trim().to_string();
        let password = password_row.text().to_string();
        btn.set_sensitive(false);
        let role_name = role.rolname.clone();
        let login = login_row.is_active();
        let createdb = createdb_row.is_active();
        let createrole = createrole_row.is_active();
        let superuser = superuser_row.is_active();
        let valid_until = (!valid_until.is_empty()).then_some(valid_until);
        let password = (!password.is_empty()).then_some(password);
        let handle = spawn_query!(ctx, |d| queries::update_role(
            d,
            &role_name,
            login,
            createdb,
            createrole,
            superuser,
            conn_limit,
            valid_until.as_deref(),
            password.as_deref(),
        ).await);
        let dialog = dialog_for_save.clone();
        let error_label = error_label.clone();
        let btn = btn.clone();
        let ctx = ctx.clone();
        let section = section.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(())) => {
                    dialog.close();
                    let toasts = ctx.toasts.clone();
                    refresh_roles(ctx, section);
                    toasts.add_toast(adw::Toast::new("Role updated"));
                }
                Ok(Err(err)) => {
                    error_label.set_label(&format!("Failed to update role: {err}"));
                    error_label.set_visible(true);
                }
                Err(_) => error_label.set_label("Role update task failed"),
            }
            btn.set_sensitive(true);
        });
    });
    dialog.present(Some(parent));
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
        move |btn| {
            let run = {
                let ctx = ctx.clone();
                let section = section.clone();
                let name = name.clone();
                move || {
                    let handle = if installed { spawn_query!(ctx, |d| queries::ext_drop(d, &name).await) } else { spawn_query!(ctx, |d| queries::ext_install(d, &name).await) };
                    glib::MainContext::default().spawn_local(async move {
                        let _ = handle.await;
                        refresh_extensions(ctx, section);
                    });
                }
            };
            if installed {
                let Some(parent) = btn.root() else { return };
                confirm_destructive(
                    &parent,
                    "Drop extension?",
                    &format!("This permanently removes the extension \u{201c}{name}\u{201d} and everything it provides from this database. This cannot be undone."),
                    "Drop Extension",
                    run,
                );
            } else {
                run();
            }
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
            Ok(Err(err)) => section.set_rows(vec![adw::ActionRow::builder().title("Unable to load pg_cron jobs").subtitle(err.to_string()).build()]),
            Err(_) => section.set_rows(vec![adw::ActionRow::builder().title("Job loading task failed").build()]),
        }
    });
}

fn job_row(j: &CronJob, ctx: &Ctx, section: &Section) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(j.jobname.as_deref().unwrap_or(&j.jobid.to_string())))
        .subtitle(format!("{} · {} · last: {}", j.schedule, j.command, j.last_status.as_deref().unwrap_or("never run")))
        .build();

    let history_btn = gtk::Button::builder().icon_name("view-list-symbolic").tooltip_text("Execution history").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&history_btn);
    let job_id = j.jobid;
    let job_title = j.jobname.clone().unwrap_or_else(|| job_id.to_string());
    history_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        job_title,
        move |btn| {
            let Some(parent) = btn.root() else { return };
            open_job_history(&parent, ctx.clone(), job_id, job_title.clone());
        }
    ));

    let edit_btn = gtk::Button::builder().icon_name("document-edit-symbolic").tooltip_text("Edit scheduled job").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&edit_btn);
    let job = j.clone();
    edit_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        move |btn| {
            let Some(parent) = btn.root() else { return };
            open_edit_job_dialog(&parent, ctx.clone(), section.clone(), job.clone());
        }
    ));

    let toggle_btn = gtk::Switch::builder().active(j.active).valign(gtk::Align::Center).build();
    row.add_suffix(&toggle_btn);
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
                match handle.await {
                    Ok(Ok(())) => refresh_jobs(ctx, section),
                    Ok(Err(err)) => section.set_rows(vec![adw::ActionRow::builder().title("Failed to change job state").subtitle(err.to_string()).build()]),
                    Err(_) => section.set_rows(vec![adw::ActionRow::builder().title("Job operation task failed").build()]),
                }
            });
            glib::Propagation::Proceed
        }
    ));

    let delete_btn = gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text("Delete job").valign(gtk::Align::Center).css_classes(["flat"]).build();
    row.add_suffix(&delete_btn);
    let job_name = j.jobname.clone().unwrap_or_else(|| job_id.to_string());
    delete_btn.connect_clicked(clone!(
        #[strong]
        ctx,
        #[strong]
        section,
        #[strong]
        job_name,
        move |btn| {
            let Some(parent) = btn.root() else { return };
            let ctx = ctx.clone();
            let section = section.clone();
            confirm_destructive(
                &parent,
                "Delete scheduled job?",
                &format!("This permanently removes the job \u{201c}{job_name}\u{201d}. This cannot be undone."),
                "Delete Job",
                move || {
                    let handle = spawn_query!(ctx, |d| queries::delete_job(d, job_id).await);
                    glib::MainContext::default().spawn_local(async move {
                        match handle.await {
                            Ok(Ok(())) => {
                                let toasts = ctx.toasts.clone();
                                refresh_jobs(ctx, section);
                                toasts.add_toast(adw::Toast::new("Scheduled job deleted"));
                            }
                            Ok(Err(err)) => section.set_rows(vec![adw::ActionRow::builder().title("Failed to delete job").subtitle(err.to_string()).build()]),
                            Err(_) => section.set_rows(vec![adw::ActionRow::builder().title("Job operation task failed").build()]),
                        }
                    });
                },
            );
        }
    ));

    row
}

fn open_new_job_dialog(parent: &impl IsA<gtk::Widget>, ctx: Ctx, section: Section) {
    open_job_dialog(parent, ctx, section, None);
}

fn open_edit_job_dialog(parent: &impl IsA<gtk::Widget>, ctx: Ctx, section: Section, job: CronJob) {
    open_job_dialog(parent, ctx, section, Some(job));
}

fn open_job_dialog(parent: &impl IsA<gtk::Widget>, ctx: Ctx, section: Section, existing: Option<CronJob>) {
    let editing = existing.is_some();
    let job_id = existing.as_ref().map(|job| job.jobid);
    let name_row = adw::EntryRow::builder().title("Job name (optional)").build();
    let schedule_row = adw::EntryRow::builder().title("Schedule (cron expression)").build();
    let command_row = adw::EntryRow::builder().title("SQL command").build();
    if let Some(job) = &existing {
        name_row.set_text(job.jobname.as_deref().unwrap_or(""));
        schedule_row.set_text(&job.schedule);
        command_row.set_text(&job.command);
    }
    let error_label = gtk::Label::builder().css_classes(["error"]).wrap(true).visible(false).build();
    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    group.add(&schedule_row);
    group.add(&command_row);
    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    content.append(&error_label);
    content.append(&group);
    let dialog = adw::Dialog::builder().title(if editing { "Edit Scheduled Job" } else { "New Scheduled Job" }).content_width(520).content_height(340).build();
    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let save_btn = gtk::Button::builder().label(if editing { "Save" } else { "Create" }).css_classes(["suggested-action"]).build();
    let cancel_btn = gtk::Button::builder().label("Cancel").build();
    header.pack_start(&cancel_btn);
    header.pack_end(&save_btn);
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));
    cancel_btn.connect_clicked(clone!(#[weak] dialog, move |_| {
        dialog.close();
    }));
    let dialog_for_save = dialog.clone();
    save_btn.connect_clicked(move |btn| {
        let schedule = schedule_row.text().trim().to_string();
        let command = command_row.text().trim().to_string();
        if schedule.is_empty() || command.is_empty() {
            error_label.set_label("Schedule and SQL command are required");
            error_label.set_visible(true);
            return;
        }
        let name = name_row.text().trim().to_string();
        btn.set_sensitive(false);
        let handle = if let Some(job_id) = job_id {
            spawn_query!(ctx, |d| queries::update_job(d, job_id, &schedule, &command).await)
        } else {
            spawn_query!(ctx, |d| queries::create_job(d, (!name.is_empty()).then_some(name.as_str()), &schedule, &command).await)
        };
        let dialog = dialog_for_save.clone();
        let error_label = error_label.clone();
        let btn = btn.clone();
        let ctx = ctx.clone();
        let section = section.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.await {
                Ok(Ok(())) => {
                    dialog.close();
                    refresh_jobs(ctx, section);
                }
                Ok(Err(err)) => {
                    error_label.set_label(&format!("Failed to save job: {err}"));
                    error_label.set_visible(true);
                }
                Err(_) => {
                    error_label.set_label("Job operation task failed");
                    error_label.set_visible(true);
                }
            }
            btn.set_sensitive(true);
        });
    });
    dialog.present(Some(parent));
}

fn open_job_history(parent: &impl IsA<gtk::Widget>, ctx: Ctx, job_id: i64, job_name: String) {
    let spinner = gtk::Spinner::builder().spinning(true).halign(gtk::Align::Center).margin_top(18).margin_bottom(18).build();
    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    content.append(&spinner);
    let dialog = adw::Dialog::builder().title(format!("Execution history: {job_name}")).content_width(700).content_height(520).build();
    dialog.set_child(Some(&content));
    let handle = spawn_query!(ctx, |d| queries::get_job_runs(d, job_id).await);
    let content_for_task = content.clone();
    glib::MainContext::default().spawn_local(async move {
        content_for_task.remove(&spinner);
        match handle.await {
            Ok(Ok(runs)) if runs.is_empty() => content_for_task.append(&adw::StatusPage::builder().title("No executions yet").build()),
            Ok(Ok(runs)) => {
                let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
                for run in runs {
                    list.append(&job_run_row(&run));
                }
                content_for_task.append(&gtk::ScrolledWindow::builder().child(&list).vexpand(true).build());
            }
            Ok(Err(err)) => content_for_task.append(&adw::StatusPage::builder().icon_name("dialog-error-symbolic").title("Failed to load execution history").description(err.to_string()).build()),
            Err(_) => content_for_task.append(&adw::StatusPage::builder().icon_name("dialog-error-symbolic").title("Execution history task failed").build()),
        }
    });
    dialog.present(Some(parent));
}

fn job_run_row(run: &JobRun) -> adw::ActionRow {
    let status = run.status.as_deref().unwrap_or("unknown");
    let timing = match (&run.start_time, &run.duration_sec) {
        (Some(start), Some(duration)) => format!("{start} · {duration}s"),
        (Some(start), None) => start.clone(),
        _ => "no timestamp".to_string(),
    };
    adw::ActionRow::builder().title(format!("run {} · {status}", run.runid)).subtitle(format!("{timing} · {}", run.return_message.as_deref().unwrap_or(""))).build()
}
