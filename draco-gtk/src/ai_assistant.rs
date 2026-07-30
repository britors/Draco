//! AI Assistant tab: one per connection, same "own buffer, own connection" shape as the SQL
//! Editor. A chat that can inspect the connected database (schema, `EXPLAIN` plans, sample data,
//! health stats) to advise on query best practices, performance, security and indexing — see
//! `draco_core::assistant` for the tool-calling round-trip and the read-only guarantee.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::assistant::{self, AiMessage, AiRole, ToolCall};
use draco_core::manager::ConnectionManager;
use draco_core::store;
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

type SharedManager = Arc<Mutex<ConnectionManager>>;

pub struct AiAssistantView {
    root: gtk::Widget,
    prompt: gtk::TextView,
    send: gtk::Button,
}

impl AiAssistantView {
    pub fn new(conn_id: String, runtime: tokio::runtime::Handle, manager: SharedManager) -> Self {
        let history: Rc<RefCell<Vec<AiMessage>>> = Rc::new(RefCell::new(store::get_ai_history(&conn_id)));

        let status = gtk::Label::builder().label("Pronto").xalign(0.0).wrap(true).css_classes(["dim-label"]).build();

        let transcript = gtk::TextView::builder().editable(false).cursor_visible(false).wrap_mode(gtk::WrapMode::WordChar).left_margin(16).right_margin(16).top_margin(16).bottom_margin(16).vexpand(true).build();
        let transcript_scroll = gtk::ScrolledWindow::builder().child(&transcript).vexpand(true).build();
        transcript_scroll.add_css_class("card");
        render_history(&transcript, &history.borrow());

        // TextView (not Entry) since a prompt may span several lines (a pasted query, a plan);
        // Enter alone sends, Shift+Enter breaks the line — same composer as `vega-gtk::ui::assistant`.
        let prompt = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).accepts_tab(false).top_margin(6).bottom_margin(6).left_margin(8).right_margin(8).hexpand(true).build();
        let prompt_scroll = gtk::ScrolledWindow::builder().child(&prompt).hscrollbar_policy(gtk::PolicyType::Never).min_content_height(36).max_content_height(160).propagate_natural_height(true).hexpand(true).build();
        prompt_scroll.add_css_class("card");

        let send = gtk::Button::builder().label("Enviar").css_classes(["suggested-action"]).valign(gtk::Align::End).build();
        let clear = gtk::Button::builder().label("Limpar conversa").valign(gtk::Align::End).build();

        let send_on_enter = send.clone();
        let enter_controller = gtk::EventControllerKey::new();
        enter_controller.connect_key_pressed(move |_, key, _, modifiers| {
            if matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter) && !modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                send_on_enter.emit_clicked();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        prompt.add_controller(enter_controller);

        let composer = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(8).build();
        composer.append(&prompt_scroll);
        composer.append(&send);
        composer.append(&clear);

        let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
        content.add_css_class("content-page");
        content.append(&gtk::Label::builder().label("Assistente de IA").xalign(0.0).css_classes(["title-1"]).build());
        content.append(
            &gtk::Label::builder()
                .label("Acesso somente leitura ao banco desta conexão — analisa queries, planos (EXPLAIN) e schema para sugerir índices e outras otimizações. O texto enviado (schema, planos, amostras de dados) sai para o provedor de IA configurado em Configurações.")
                .xalign(0.0)
                .wrap(true)
                .css_classes(["dim-label"])
                .build(),
        );
        content.append(&status);
        content.append(&transcript_scroll);
        content.append(&composer);

        clear.connect_clicked(clone!(
            #[strong]
            conn_id,
            #[strong]
            history,
            #[weak]
            transcript,
            #[weak]
            status,
            move |_| {
                history.borrow_mut().clear();
                render_history(&transcript, &history.borrow());
                match store::clear_ai_history(&conn_id) {
                    Ok(()) => status.set_label("Conversa limpa"),
                    Err(error) => status.set_label(&error.to_string()),
                }
            }
        ));

        send.connect_clicked(clone!(
            #[strong]
            conn_id,
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            history,
            #[weak]
            prompt,
            #[weak]
            transcript,
            #[weak]
            status,
            #[weak]
            send,
            move |_| {
                let buffer = prompt.buffer();
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).trim().to_string();
                if text.is_empty() {
                    return;
                }
                buffer.set_text("");

                history.borrow_mut().push(AiMessage { role: AiRole::User, content: text, tool_label: None });
                render_history(&transcript, &history.borrow());
                let _ = store::save_ai_history(&conn_id, &history.borrow());

                set_busy(&prompt, &send, true);
                status.set_label("Consultando o provedor…");

                glib::MainContext::default().spawn_local(clone!(
                    #[strong]
                    conn_id,
                    #[strong]
                    runtime,
                    #[strong]
                    manager,
                    #[strong]
                    history,
                    #[weak]
                    transcript,
                    #[weak]
                    status,
                    #[weak]
                    prompt,
                    #[weak]
                    send,
                    async move {
                        run_conversation(&conn_id, &runtime, &manager, &history, &transcript, &status).await;
                        set_busy(&prompt, &send, false);
                    }
                ));
            }
        ));

        Self { root: content.upcast(), prompt, send }
    }

    pub fn widget(&self) -> &gtk::Widget {
        &self.root
    }

    /// Pre-fills the composer with `text` and submits it immediately — used by the Query
    /// Editor's "Avaliar com IA" action to hand off a freshly-opened tab with the review request
    /// already sent, instead of making the user paste it in and click Send themselves.
    pub fn send_message(&self, text: &str) {
        self.prompt.buffer().set_text(text);
        self.send.emit_clicked();
    }
}

fn set_busy(prompt: &gtk::TextView, send: &gtk::Button, busy: bool) {
    prompt.set_sensitive(!busy);
    send.set_sensitive(!busy);
    send.set_label(if busy { "Pensando…" } else { "Enviar" });
}

fn render_history(transcript: &gtk::TextView, history: &[AiMessage]) {
    let text = if history.is_empty() {
        "Pergunte sobre performance, segurança ou boas práticas de uma query. Descreva o problema \
         ou cole o SQL — o Assistente pode inspecionar o schema, rodar EXPLAIN e sugerir índices."
            .to_string()
    } else {
        history
            .iter()
            .map(|message| {
                let who = match (&message.role, &message.tool_label) {
                    (_, Some(tool)) => format!("Ferramenta · {tool}"),
                    (AiRole::User, None) => "Você".to_string(),
                    (AiRole::Assistant, None) => "Draco".to_string(),
                };
                format!("{who}\n{}", message.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let buffer = transcript.buffer();
    buffer.set_text(&text);

    // `scroll_to_iter` needs a fresh size allocation for the new text to compute the right
    // offset, which hasn't happened yet at this point in the call stack — deferring to an idle
    // callback lets GTK finish laying out the buffer first, so the scroll actually lands at the
    // bottom instead of wherever the view was before the update.
    glib::idle_add_local_once(clone!(
        #[weak]
        transcript,
        move || {
            let buffer = transcript.buffer();
            let mut end = buffer.end_iter();
            transcript.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
        }
    ));
}

async fn run_conversation(
    conn_id: &str,
    runtime: &tokio::runtime::Handle,
    manager: &SharedManager,
    history: &Rc<RefCell<Vec<AiMessage>>>,
    transcript: &gtk::TextView,
    status: &gtk::Label,
) {
    let settings = store::get_ai_settings();
    let max_rounds = settings.max_rounds_per_message.clamp(1, 20);
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut estimated_cost = 0.0f64;
    let mut has_cost = false;

    for round in 0..max_rounds {
        status.set_label(&format!("Pensando… etapa {} de {max_rounds}", round + 1));

        let round_settings = settings.clone();
        let round_history = history.borrow().clone();
        let handle = runtime.spawn(async move {
            if round == 0 {
                assistant::send(&round_settings, &round_history).await
            } else {
                assistant::continue_after_tool(&round_settings, &round_history).await
            }
        });
        let reply = match handle.await {
            Ok(Ok(reply)) => reply,
            Ok(Err(error)) => {
                status.set_label(&error.to_string());
                return;
            }
            Err(_) => {
                status.set_label("Falha interna ao consultar o provedor");
                return;
            }
        };

        input_tokens += reply.input_tokens;
        output_tokens += reply.output_tokens;
        if let Some(cost) = reply.estimated_cost_usd {
            estimated_cost += cost;
            has_cost = true;
        }

        if !reply.text.is_empty() {
            history.borrow_mut().push(AiMessage { role: AiRole::Assistant, content: reply.text, tool_label: None });
            render_history(transcript, &history.borrow());
        }

        let has_tools = !reply.tool_calls.is_empty();
        for call in reply.tool_calls {
            run_tool_call(conn_id, runtime, manager, history, transcript, call).await;
        }

        let _ = store::save_ai_history(conn_id, &history.borrow());

        let cost = if has_cost { format!(" • estimativa US$ {estimated_cost:.6}") } else { String::new() };
        status.set_label(&format!("{input_tokens} tokens de entrada • {output_tokens} de saída{cost}"));

        if !has_tools {
            return;
        }
    }
}

/// Resolves the live driver (connecting first if needed, same as every other view) and dispatches
/// the tool call — a plain fn so its `Result<String, AssistantError>` return type is explicit,
/// letting the `?` conversions from `CoreError` below resolve unambiguously.
async fn ensure_and_run_tool(manager: SharedManager, conn_id: String, call: ToolCall) -> Result<String, assistant::AssistantError> {
    let mut mgr = manager.lock().await;
    crate::connection_runtime::ensure_connected(&mut mgr, &conn_id).await?;
    let driver = mgr.get_driver(&conn_id).ok_or(draco_core::error::CoreError::NotConnected)?;
    assistant::run_tool(driver, &call).await
}

async fn run_tool_call(
    conn_id: &str,
    runtime: &tokio::runtime::Handle,
    manager: &SharedManager,
    history: &Rc<RefCell<Vec<AiMessage>>>,
    transcript: &gtk::TextView,
    call: ToolCall,
) {
    let handle = runtime.spawn(ensure_and_run_tool(manager.clone(), conn_id.to_string(), call.clone()));

    let outcome = match handle.await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => format!("ERRO: {error}"),
        Err(_) => "ERRO: falha interna ao rodar a ferramenta".to_string(),
    };
    let content = format!(
        "<dado_nao_confiavel origem=\"tool:{}\">\n{outcome}\n</dado_nao_confiavel>\nContinue a resposta usando este resultado (ou corrija a chamada, se for um erro).",
        call.name
    );
    history.borrow_mut().push(AiMessage { role: AiRole::User, content, tool_label: Some(call.name) });
    render_history(transcript, &history.borrow());
}
