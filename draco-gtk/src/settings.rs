//! Application settings exposed through the main application menu.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use draco_core::assistant::{self, Provider};
use draco_core::store;
use gtk::glib;
use gtk::glib::clone;

/// Opens the preferences dialog and persists each value as it changes.
pub fn show(parent: &adw::ApplicationWindow, runtime: tokio::runtime::Handle) {
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

    page.add(&build_ai_group(parent, runtime));
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

fn persist_ai(update: impl FnOnce(&mut store::AiSettings)) {
    let mut settings = store::get_ai_settings();
    update(&mut settings);
    if let Err(error) = store::save_ai_settings(&settings) {
        tracing::warn!(%error, "failed to save AI assistant settings");
    }
}

/// The window's own content *is* the `AdwToastOverlay` set up in `window_main::build` — reusing it
/// here avoids adding a second overlay just for this dialog's async feedback (save/remove key,
/// refresh models).
fn toast(parent: &adw::ApplicationWindow, message: &str) {
    if let Some(overlay) = parent.content().and_downcast::<adw::ToastOverlay>() {
        overlay.add_toast(adw::Toast::new(message));
    }
}

fn build_ai_group(parent: &adw::ApplicationWindow, runtime: tokio::runtime::Handle) -> adw::PreferencesGroup {
    let ai = store::get_ai_settings();

    let group = adw::PreferencesGroup::builder()
        .title("Assistente de IA")
        .description(
            "A chave de API fica somente no Secret Service (GNOME Keyring), nunca em arquivo de \
             configuração. Ao usar o Assistente numa aba de conexão, o texto enviado ao provedor \
             pode incluir schema, planos de execução (EXPLAIN) e amostras de dados do banco.",
        )
        .build();

    let provider_row = adw::ComboRow::builder()
        .title("Provedor")
        .model(&gtk::StringList::new(&Provider::ALL.map(|p| p.label())))
        .selected(ai.provider.index())
        .build();
    group.add(&provider_row);

    // Holds the model names behind `model_row`'s displayed labels — `ComboRow` only exposes the
    // selected *index*, so this is what turns that index back into a model name to persist.
    let models: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(vec![ai.model().to_string()]));
    let model_row = adw::ComboRow::builder()
        .title("Modelo")
        .subtitle("Salve uma chave válida e atualize para ver os modelos disponíveis")
        .model(&gtk::StringList::new(&[ai.model()]))
        .build();
    let refresh_models_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .tooltip_text("Atualizar modelos disponíveis")
        .build();
    model_row.add_suffix(&refresh_models_btn);
    group.add(&model_row);

    let api_key_row = adw::PasswordEntryRow::builder().title("Chave de API").build();
    let save_key_btn = gtk::Button::builder()
        .icon_name("document-save-symbolic")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .tooltip_text("Salvar chave no keyring")
        .build();
    let remove_key_btn = gtk::Button::builder()
        .icon_name("edit-delete-symbolic")
        .valign(gtk::Align::Center)
        .css_classes(["flat", "destructive-action"])
        .tooltip_text("Remover chave do keyring")
        .build();
    api_key_row.add_suffix(&save_key_btn);
    api_key_row.add_suffix(&remove_key_btn);
    group.add(&api_key_row);

    let daily_limit = adw::SpinRow::builder()
        .title("Limite diário de mensagens")
        .subtitle("Máximo de mensagens enviadas ao provedor por dia")
        .adjustment(&gtk::Adjustment::new(f64::from(ai.max_messages_per_day), 1.0, 5000.0, 1.0, 10.0, 0.0))
        .build();
    group.add(&daily_limit);

    let max_rounds = adw::SpinRow::builder()
        .title("Máximo de etapas por mensagem")
        .subtitle("Quantas vezes o Assistente pode chamar ferramentas antes de responder")
        .adjustment(&gtk::Adjustment::new(f64::from(ai.max_rounds_per_message), 1.0, 20.0, 1.0, 1.0, 0.0))
        .build();
    group.add(&max_rounds);

    provider_row.connect_selected_notify(clone!(
        #[strong]
        model_row,
        #[strong]
        models,
        move |row| {
            let provider = Provider::from_index(row.selected());
            persist_ai(move |settings| settings.provider = provider);
            let label = store::get_ai_settings().model().to_string();
            model_row.set_model(Some(&gtk::StringList::new(&[label.as_str()])));
            model_row.set_selected(0);
            *models.borrow_mut() = vec![label];
        }
    ));

    model_row.connect_selected_notify(clone!(
        #[strong]
        models,
        move |row| {
            if let Some(model) = models.borrow().get(row.selected() as usize).cloned() {
                persist_ai(move |settings| settings.set_model(model));
            }
        }
    ));

    daily_limit.connect_value_notify(|row| {
        let value = row.value().round() as u32;
        persist_ai(move |settings| settings.max_messages_per_day = value);
    });
    max_rounds.connect_value_notify(|row| {
        let value = row.value().round() as u32;
        persist_ai(move |settings| settings.max_rounds_per_message = value);
    });

    save_key_btn.connect_clicked(clone!(
        #[weak]
        parent,
        #[weak]
        provider_row,
        #[weak]
        api_key_row,
        #[strong]
        runtime,
        move |_| {
            let provider = Provider::from_index(provider_row.selected());
            let key = api_key_row.text().to_string();
            glib::MainContext::default().spawn_local(clone!(
                #[weak]
                parent,
                #[weak]
                api_key_row,
                #[strong]
                runtime,
                async move {
                    let handle = runtime.spawn(async move { assistant::save_key(provider, &key).await });
                    match handle.await {
                        Ok(Ok(())) => {
                            api_key_row.set_text("");
                            toast(&parent, "Chave salva com segurança no keyring");
                        }
                        Ok(Err(error)) => toast(&parent, &error.to_string()),
                        Err(_) => toast(&parent, "Falha interna ao acessar o keyring"),
                    }
                }
            ));
        }
    ));

    remove_key_btn.connect_clicked(clone!(
        #[weak]
        parent,
        #[weak]
        provider_row,
        #[strong]
        runtime,
        move |_| {
            let provider = Provider::from_index(provider_row.selected());
            glib::MainContext::default().spawn_local(clone!(
                #[weak]
                parent,
                #[strong]
                runtime,
                async move {
                    let handle = runtime.spawn(async move { assistant::clear_key(provider).await });
                    match handle.await {
                        Ok(Ok(())) => toast(&parent, "Chave removida do keyring"),
                        Ok(Err(error)) => toast(&parent, &error.to_string()),
                        Err(_) => toast(&parent, "Falha interna ao acessar o keyring"),
                    }
                }
            ));
        }
    ));

    refresh_models_btn.connect_clicked(clone!(
        #[weak]
        parent,
        #[weak]
        provider_row,
        #[weak]
        model_row,
        #[strong]
        models,
        #[strong]
        runtime,
        move |_| {
            let provider = Provider::from_index(provider_row.selected());
            let current_model = store::get_ai_settings().model().to_string();
            glib::MainContext::default().spawn_local(clone!(
                #[weak]
                parent,
                #[weak]
                model_row,
                #[strong]
                models,
                #[strong]
                runtime,
                async move {
                    let handle = runtime.spawn(async move { assistant::list_models(provider).await });
                    match handle.await {
                        Ok(Ok(mut list)) => {
                            if !list.contains(&current_model) {
                                list.insert(0, current_model.clone());
                            }
                            let labels = list.iter().map(String::as_str).collect::<Vec<_>>();
                            model_row.set_model(Some(&gtk::StringList::new(&labels)));
                            let index = list.iter().position(|m| *m == current_model).unwrap_or(0);
                            model_row.set_selected(index as u32);
                            *models.borrow_mut() = list;
                            toast(&parent, "Lista de modelos atualizada");
                        }
                        Ok(Err(error)) => toast(&parent, &error.to_string()),
                        Err(_) => toast(&parent, "Falha interna ao consultar modelos"),
                    }
                }
            ));
        }
    ));

    group
}
