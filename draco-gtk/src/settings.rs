//! Application settings exposed through the main application menu.

use adw::prelude::*;
use draco_core::store;

/// Opens the preferences dialog and persists each value as it changes.
pub fn show(parent: &adw::ApplicationWindow) {
    let current = store::get_settings();
    let dialog = adw::PreferencesDialog::builder()
        .title("Configurações")
        .build();
    let page = adw::PreferencesPage::new();

    let queries = adw::PreferencesGroup::builder().title("Consultas").build();
    let query_timeout = adw::SpinRow::builder()
        .title("Tempo limite")
        .subtitle("Tempo máximo de execução da consulta, em milissegundos")
        .adjustment(&gtk::Adjustment::new(
            current.query_timeout as f64,
            1_000.0,
            600_000.0,
            1_000.0,
            10_000.0,
            0.0,
        ))
        .build();
    let preview_row_limit = adw::SpinRow::builder()
        .title("Limite de linhas")
        .subtitle("Quantidade máxima de linhas mostradas nas prévias")
        .adjustment(&gtk::Adjustment::new(
            current.preview_row_limit as f64,
            1.0,
            100_000.0,
            1.0,
            100.0,
            0.0,
        ))
        .build();
    let show_row_count = adw::SwitchRow::builder()
        .title("Mostrar contagem de linhas")
        .subtitle("Exibe a quantidade de linhas no resultado da consulta")
        .active(current.show_row_count)
        .build();
    queries.add(&query_timeout);
    queries.add(&preview_row_limit);
    queries.add(&show_row_count);
    page.add(&queries);
    dialog.add(&page);

    query_timeout.connect_value_notify(|row| {
        persist(|settings| settings.query_timeout = row.value().round() as u32);
    });
    preview_row_limit.connect_value_notify(|row| {
        persist(|settings| settings.preview_row_limit = row.value().round() as u32);
    });
    show_row_count.connect_active_notify(|row| {
        persist(|settings| settings.show_row_count = row.is_active());
    });

    dialog.present(Some(parent));
}

fn persist(update: impl FnOnce(&mut store::AppSettings)) {
    if let Err(error) = store::patch_settings(update) {
        tracing::warn!(%error, "failed to save application settings");
    }
}
