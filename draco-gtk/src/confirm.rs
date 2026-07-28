//! Shared "are you sure?" dialog for irreversible actions (DROP, DELETE, destructive ALTER,
//! VACUUM FULL) — the `adw::AlertDialog` confirmation pattern used across `admin.rs`,
//! `table_editor.rs` and `table_detail.rs`.

use adw::prelude::*;
use gtk::gio;

pub fn confirm_destructive(parent: &impl IsA<gtk::Widget>, heading: &str, body: &str, confirm_label: &str, on_confirm: impl FnOnce() + 'static) {
    let dialog = adw::AlertDialog::builder().heading(heading).body(body).close_response("cancel").default_response("cancel").build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("confirm", confirm_label);
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    dialog.choose(Some(parent), None::<&gio::Cancellable>, move |response| {
        if response == "confirm" {
            on_confirm();
        }
    });
}
