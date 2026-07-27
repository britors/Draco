//! Create/edit dialog for a `DbConnection`. Saving is async (persisting the password goes
//! through the Secret Service via `draco-core::secrets`), so — unlike a purely synchronous
//! dialog — the Save button spawns work on the tokio runtime and only closes the dialog once
//! that work has actually finished.

use adw::prelude::*;
use draco_core::connection::{validate_connection, DbConnection, DbConnectionDraft};
use draco_core::{secrets, store};
use gtk::glib;
use gtk::glib::clone;

pub fn open(
    parent: &impl IsA<gtk::Widget>,
    runtime: tokio::runtime::Handle,
    initial: Option<DbConnection>,
    on_saved: impl Fn(DbConnection) + 'static,
) {
    let on_saved = std::rc::Rc::new(on_saved);
    let is_new = initial.is_none();
    let base = initial.unwrap_or_default();

    let label_row = adw::EntryRow::builder().title("Label").text(&base.label).build();
    let host_row = adw::EntryRow::builder().title("Host").text(&base.host).build();
    let port_row = adw::SpinRow::builder()
        .title("Port")
        .adjustment(&gtk::Adjustment::new(f64::from(base.port), 1.0, 65535.0, 1.0, 10.0, 0.0))
        .build();
    let database_row = adw::EntryRow::builder().title("Database").text(&base.database).build();
    let user_row = adw::EntryRow::builder().title("User").text(&base.user).build();
    let password_row = adw::PasswordEntryRow::builder().title("Password").build();
    let ssl_row = adw::SwitchRow::builder().title("SSL").active(base.ssl).build();

    let conn_group = adw::PreferencesGroup::builder().title("Connection").build();
    for row in [&label_row, &host_row] {
        conn_group.add(row);
    }
    conn_group.add(&port_row);
    conn_group.add(&database_row);
    conn_group.add(&user_row);
    conn_group.add(&password_row);
    conn_group.add(&ssl_row);

    let ssh_enabled_row = adw::SwitchRow::builder().title("Enabled").active(base.ssh_enabled).build();
    let ssh_host_row = adw::EntryRow::builder().title("SSH Host").text(base.ssh_host.clone().unwrap_or_default()).build();
    let ssh_port_row = adw::SpinRow::builder()
        .title("SSH Port")
        .adjustment(&gtk::Adjustment::new(f64::from(base.ssh_port.unwrap_or(22)), 1.0, 65535.0, 1.0, 10.0, 0.0))
        .build();
    let ssh_user_row = adw::EntryRow::builder().title("SSH User").text(base.ssh_user.clone().unwrap_or_default()).build();
    let ssh_key_row =
        adw::EntryRow::builder().title("Private key path (optional)").text(base.ssh_key_path.clone().unwrap_or_default()).build();
    let ssh_password_row = adw::PasswordEntryRow::builder().title("SSH password / key passphrase").build();

    let ssh_group = adw::PreferencesGroup::builder().title("SSH Tunnel").build();
    ssh_group.add(&ssh_enabled_row);
    ssh_group.add(&ssh_host_row);
    ssh_group.add(&ssh_port_row);
    ssh_group.add(&ssh_user_row);
    ssh_group.add(&ssh_key_row);
    ssh_group.add(&ssh_password_row);

    let jump_host_row =
        adw::EntryRow::builder().title("Jump Host").text(base.ssh_jump_host.clone().unwrap_or_default()).build();
    let jump_port_row = adw::SpinRow::builder()
        .title("Jump Port")
        .adjustment(&gtk::Adjustment::new(f64::from(base.ssh_jump_port.unwrap_or(22)), 1.0, 65535.0, 1.0, 10.0, 0.0))
        .build();
    let jump_user_row = adw::EntryRow::builder().title("Jump User").text(base.ssh_jump_user.clone().unwrap_or_default()).build();
    let jump_key_row =
        adw::EntryRow::builder().title("Jump key path (optional)").text(base.ssh_jump_key_path.clone().unwrap_or_default()).build();
    let jump_password_row = adw::PasswordEntryRow::builder().title("Jump password / key passphrase").build();

    let jump_expander =
        adw::ExpanderRow::builder().title("Jump Host (ProxyJump)").subtitle("Optional — reach the SSH host through a bastion").build();
    jump_expander.add_row(&jump_host_row);
    jump_expander.add_row(&jump_port_row);
    jump_expander.add_row(&jump_user_row);
    jump_expander.add_row(&jump_key_row);
    jump_expander.add_row(&jump_password_row);
    ssh_group.add(&jump_expander);

    let error_label = gtk::Label::builder().css_classes(["error"]).wrap(true).visible(false).build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&error_label);
    content.append(&conn_group);
    content.append(&ssh_group);

    let scroller =
        gtk::ScrolledWindow::builder().child(&content).propagate_natural_height(true).min_content_width(480).build();

    let dialog = adw::Dialog::builder()
        .title(if is_new { "New Connection" } else { "Edit Connection" })
        .content_width(520)
        .content_height(640)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let save_btn = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
    let cancel_btn = gtk::Button::builder().label("Cancel").build();
    header.pack_start(&cancel_btn);
    header.pack_end(&save_btn);
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

    save_btn.connect_clicked(clone!(
        #[weak]
        dialog,
        #[strong]
        on_saved,
        move |btn| {
        let draft = DbConnectionDraft {
            label: label_row.text().to_string(),
            host: host_row.text().to_string(),
            port: port_row.value() as u32,
            database: database_row.text().to_string(),
            user: user_row.text().to_string(),
            ssl: ssl_row.is_active(),
        };
        let errors = validate_connection(&draft);
        if !errors.is_empty() {
            error_label.set_label(&errors.join(", "));
            error_label.set_visible(true);
            return;
        }

        let non_empty = |s: glib::GString| -> Option<String> {
            let s = s.to_string();
            (!s.is_empty()).then_some(s)
        };

        let conn = DbConnection {
            id: base.id.clone(),
            label: draft.label,
            host: draft.host,
            port: draft.port as u16,
            database: draft.database,
            user: draft.user,
            ssl: draft.ssl,
            ssh_enabled: ssh_enabled_row.is_active(),
            ssh_host: non_empty(ssh_host_row.text()),
            ssh_port: Some(ssh_port_row.value() as u16),
            ssh_user: non_empty(ssh_user_row.text()),
            ssh_key_path: non_empty(ssh_key_row.text()),
            ssh_jump_host: non_empty(jump_host_row.text()),
            ssh_jump_port: Some(jump_port_row.value() as u16),
            ssh_jump_user: non_empty(jump_user_row.text()),
            ssh_jump_key_path: non_empty(jump_key_row.text()),
            favorite: base.favorite,
        };
        let password = password_row.text().to_string();
        let ssh_password = ssh_password_row.text().to_string();
        let jump_password = jump_password_row.text().to_string();

        let saved = match store::save_connection(conn) {
            Ok(saved) => saved,
            Err(err) => {
                error_label.set_label(&format!("Failed to save connection: {err}"));
                error_label.set_visible(true);
                return;
            }
        };

        btn.set_sensitive(false);
        let id = saved.id.clone();
        let saved_for_task = saved.clone();
        let save_btn = btn.clone();
        let dialog_for_task = dialog.clone();
        let on_saved_for_task = on_saved.clone();
        let handle = runtime.spawn(async move {
            let _ = secrets::store_password(&id, &password).await;
            if !ssh_password.is_empty() {
                let _ = secrets::store_ssh_password(&id, &ssh_password).await;
            }
            if !jump_password.is_empty() {
                let _ = secrets::store_jump_ssh_password(&id, &jump_password).await;
            }
        });
        glib::MainContext::default().spawn_local(async move {
            let _ = handle.await;
            save_btn.set_sensitive(true);
            dialog_for_task.close();
            on_saved_for_task(saved_for_task);
        });
        }
    ));

    dialog.present(Some(parent));
}
