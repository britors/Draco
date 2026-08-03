//! Main window: `AdwNavigationSplitView` with the connection/schema Explorer sidebar and a
//! content pane (query editor and friends land here from M3 onward).

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use draco_core::connection::DbConnection;
use draco_core::manager::ConnectionManager;
use draco_core::store;
use gtk::gio;
use gtk::glib;
use gtk::glib::clone;
use tokio::sync::Mutex;

use crate::admin::AdminView;
use crate::ai_assistant::AiAssistantView;
use crate::backup_restore;
use crate::connection_dialog;
use crate::dashboard::DashboardView;
use crate::erd::ErdView;
use crate::explorer::Explorer;
use crate::function_editor::FunctionEditor;
use crate::query_editor::QueryEditor;
use crate::search;
use crate::settings;
use crate::object_creator;
use crate::table_creator;
use crate::table_detail::TableDetailView;

/// Registry of `(tab page, its QueryEditor's run action, its script action)` — see the
/// `query_run_registry` comment at its construction site in `build` for why this exists.
type QueryRunRegistry = Rc<RefCell<Vec<(adw::TabPage, Rc<dyn Fn(bool)>, Rc<dyn Fn()>)>>>;
type OpenTable = Rc<dyn Fn(String, String, String)>;
type OpenTableCell = Rc<RefCell<Option<OpenTable>>>;
/// Opens (or reuses) the connection's AI Assistant tab and sends it `(conn_id, message)` —
/// shared by the query editor's "Avaliar com IA" flow, the Explorer's "Open in New Query" AI
/// hook and the Admin view's per-row "Analisar com IA" button, so the tab-opening logic lives
/// in exactly one place.
type OpenAiMessage = Rc<dyn Fn(String, String)>;
/// Opens a new Query tab pre-filled with `(conn_id, sql)`, titled `title`. Shared by the
/// Explorer's "Open SELECT * in New Query" and the Admin view's per-row "Open in new Query tab"
/// button.
type OpenQuerySql = Rc<dyn Fn(String, String, String)>;

pub fn build(app: &adw::Application, runtime: tokio::runtime::Handle) {
    let manager: Arc<Mutex<ConnectionManager>> = Arc::new(Mutex::new(ConnectionManager::new()));

    // Wraps the whole window content so any view can surface a transient `AdwToast` — see the
    // clones threaded into AdminView::new/QueryEditor::new/connection_dialog::open below.
    let toast_overlay = adw::ToastOverlay::new();

    let tab_view = adw::TabView::new();

    let open_table_cell: OpenTableCell = Rc::new(RefCell::new(None));
    let open_table: OpenTable = Rc::new(clone!(
        #[weak]
        tab_view,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        open_table_cell,
        move |conn_id, schema, table| {
            let navigation = open_table_cell.borrow().clone().unwrap_or_else(|| Rc::new(|_, _, _| {}));
            let view = TableDetailView::new_with_navigation(
                conn_id,
                schema.clone(),
                table.clone(),
                runtime.clone(),
                manager.clone(),
                navigation,
            );
            let page = tab_view.append(view.widget());
            page.set_title(&format!("{schema}.{table}"));
            tab_view.set_selected_page(&page);
        }
    ));
    *open_table_cell.borrow_mut() = Some(open_table.clone());

    // F8 (run) / F10 (explain) need to reach whichever query tab is currently selected — a
    // widget-local key controller on the query editor's own root turned out not to reliably see
    // those keys while the GtkSourceView had focus, so instead each QueryEditor registers its
    // `run_action` here (keyed by its `AdwTabPage`, cleaned up on `page-detached`) and the two
    // window-level actions below (mirroring `win.search`/`win.new-query`) look up whichever entry
    // matches `tab_view.selected_page()` at the moment the accelerator fires.
    let query_run_registry: QueryRunRegistry = Rc::new(RefCell::new(Vec::new()));
    tab_view.connect_page_detached(clone!(
        #[strong]
        query_run_registry,
        move |_, page, _| {
            query_run_registry.borrow_mut().retain(|(p, _, _)| p != page);
        }
    ));

    let open_ai_with_message: OpenAiMessage = Rc::new(clone!(
        #[strong]
        runtime,
        #[strong]
        manager,
        #[weak]
        tab_view,
        move |conn_id: String, message: String| {
            let view = AiAssistantView::new(conn_id, runtime.clone(), manager.clone());
            let page = tab_view.append(view.widget());
            page.set_title("Assistente de IA");
            tab_view.set_selected_page(&page);
            view.send_message(&message);
        }
    ));

    let open_query_with_sql: OpenQuerySql = Rc::new(clone!(
        #[strong]
        runtime,
        #[strong]
        manager,
        #[weak]
        tab_view,
        #[strong]
        query_run_registry,
        #[strong]
        toast_overlay,
        #[strong]
        open_ai_with_message,
        move |conn_id: String, sql: String, title: String| {
            let connections = store::list_connections();
            let editor = QueryEditor::new(
                connections,
                runtime.clone(),
                manager.clone(),
                toast_overlay.clone(),
                Some((conn_id, sql)),
                clone!(
                    #[strong]
                    open_ai_with_message,
                    move |conn_id, message| open_ai_with_message(conn_id, message)
                ),
            );
            let page = tab_view.append(editor.widget());
            page.set_title(&title);
            editor.bind_tab_page(page.clone());
            query_run_registry.borrow_mut().push((page.clone(), editor.run_action(), editor.script_action()));
            tab_view.set_selected_page(&page);
        }
    ));

    let app_for_new_table = app.clone();
    // Filled in right after `Explorer::new` returns, below — lets callbacks constructed here
    // (which only ever *run* later, after a user click) refresh the Explorer once it exists.
    let explorer_cell: Rc<RefCell<Option<Rc<Explorer>>>> = Rc::new(RefCell::new(None));
    let explorer = Rc::new(Explorer::new(
        clone!(
            #[strong]
            open_table,
            move |conn_id, schema, table| {
                (open_table)(conn_id, schema, table);
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[weak]
            tab_view,
            move |conn_id| {
                let view = DashboardView::new(conn_id, runtime.clone(), manager.clone());
                let page = tab_view.append(view.widget());
                page.set_title("Dashboard");
                tab_view.set_selected_page(&page);
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[weak]
            tab_view,
            move |conn_id, schema, func_name: Option<String>| {
                let title = func_name
                    .clone()
                    .unwrap_or_else(|| "New Function".to_string());
                let editor = FunctionEditor::new(
                    conn_id,
                    schema,
                    func_name,
                    runtime.clone(),
                    manager.clone(),
                );
                let page = tab_view.append(editor.widget());
                page.set_title(&title);
                tab_view.set_selected_page(&page);
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            app_for_new_table,
            #[strong]
            explorer_cell,
            move |conn_id: String, schema: String| {
                let Some(parent) = app_for_new_table.active_window() else {
                    return;
                };
                table_creator::open(
                    &parent,
                    conn_id,
                    vec![schema],
                    runtime.clone(),
                    manager.clone(),
                    clone!(
                        #[strong]
                        runtime,
                        #[strong]
                        manager,
                        #[strong]
                        explorer_cell,
                        move || {
                            // Reloads every connection's tree from scratch (simplest correct way
                            // to surface the new table) — this does collapse any expanded rows.
                            if let Some(explorer) = explorer_cell.borrow().clone() {
                                refresh_connections(&explorer, &manager, &runtime);
                            }
                        }
                    ),
                );
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            app_for_new_table,
            #[strong]
            explorer_cell,
            move |conn_id: String, schema: String| {
                let Some(parent) = app_for_new_table.active_window() else {
                    return;
                };
                object_creator::open_sequence(
                    &parent,
                    conn_id,
                    schema,
                    runtime.clone(),
                    manager.clone(),
                    clone!(
                        #[strong]
                        runtime,
                        #[strong]
                        manager,
                        #[strong]
                        explorer_cell,
                        move || {
                            if let Some(explorer) = explorer_cell.borrow().clone() {
                                refresh_connections(&explorer, &manager, &runtime);
                            }
                        }
                    ),
                );
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            app_for_new_table,
            #[strong]
            explorer_cell,
            move |conn_id: String, schema: String| {
                let Some(parent) = app_for_new_table.active_window() else {
                    return;
                };
                object_creator::open_trigger(
                    &parent,
                    conn_id,
                    schema,
                    runtime.clone(),
                    manager.clone(),
                    clone!(
                        #[strong]
                        runtime,
                        #[strong]
                        manager,
                        #[strong]
                        explorer_cell,
                        move || {
                            if let Some(explorer) = explorer_cell.borrow().clone() {
                                refresh_connections(&explorer, &manager, &runtime);
                            }
                        }
                    ),
                );
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[weak]
            tab_view,
            #[strong]
            toast_overlay,
            #[strong]
            open_query_with_sql,
            #[strong]
            open_ai_with_message,
            move |conn_id| {
                let view = AdminView::new(
                    conn_id,
                    runtime.clone(),
                    manager.clone(),
                    toast_overlay.clone(),
                    open_query_with_sql.clone(),
                    open_ai_with_message.clone(),
                );
                let page = tab_view.append(view.widget());
                page.set_title("Admin");
                tab_view.set_selected_page(&page);
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[weak]
            tab_view,
            move |conn_id| {
                let view = AiAssistantView::new(conn_id, runtime.clone(), manager.clone());
                let page = tab_view.append(view.widget());
                page.set_title("Assistente de IA");
                tab_view.set_selected_page(&page);
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[weak]
            tab_view,
            #[strong]
            open_table,
            move |conn_id, schema: String| {
                let view = ErdView::new(conn_id, schema.clone(), runtime.clone(), manager.clone(), open_table.clone());
                let page = tab_view.append(view.widget());
                page.set_title(&format!("ERD: {schema}"));
                tab_view.set_selected_page(&page);
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            app_for_new_table,
            #[strong]
            explorer_cell,
            #[strong]
            toast_overlay,
            move |conn: DbConnection| {
                let Some(parent) = app_for_new_table.active_window() else {
                    return;
                };
                connection_dialog::open(
                    &parent,
                    runtime.clone(),
                    Some(conn),
                    toast_overlay.clone(),
                    clone!(
                        #[strong]
                        runtime,
                        #[strong]
                        manager,
                        #[strong]
                        explorer_cell,
                        move |_saved| {
                            if let Some(explorer) = explorer_cell.borrow().clone() {
                                refresh_connections(&explorer, &manager, &runtime);
                            }
                        }
                    ),
                );
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            app_for_new_table,
            move |conn: DbConnection| {
                let Some(parent) = app_for_new_table.active_window() else {
                    return;
                };
                backup_restore::open(&parent, conn, runtime.clone(), manager.clone());
            }
        ),
        clone!(
            #[strong]
            runtime,
            #[strong]
            manager,
            #[strong]
            explorer_cell,
            move || {
                if let Some(explorer) = explorer_cell.borrow().clone() {
                    refresh_connections(&explorer, &manager, &runtime);
                }
            }
        ),
        clone!(
            #[strong]
            open_query_with_sql,
            move |conn_id: String, sql: String, title: String| open_query_with_sql(conn_id, sql, title)
        ),
        toast_overlay.clone(),
    ));
    *explorer_cell.borrow_mut() = Some(explorer.clone());

    refresh_connections(&explorer, &manager, &runtime);

    let add_btn = gtk::Button::from_icon_name("list-add-symbolic");
    add_btn.set_tooltip_text(Some("Add Connection"));

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_title_widget(Some(&adw::WindowTitle::new("Conexões", "")));
    sidebar_header.pack_end(&add_btn);

    let sidebar_scroller = gtk::ScrolledWindow::builder()
        .child(explorer.widget())
        .vexpand(true)
        .build();
    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.add_top_bar(&sidebar_header);
    sidebar_toolbar.set_content(Some(&sidebar_scroller));
    let sidebar_page = adw::NavigationPage::builder()
        .title("Explorer")
        .child(&sidebar_toolbar)
        .build();

    // Without this, AdwTabBar hides itself (and its close button) whenever only one tab is
    // open, so a lone tab can't be closed except via keyboard shortcut.
    let tab_bar = adw::TabBar::builder()
        .view(&tab_view)
        .autohide(false)
        .build();

    let new_query_btn = gtk::Button::from_icon_name("tab-new-symbolic");
    new_query_btn.set_tooltip_text(Some("New Query"));

    let search_btn = gtk::Button::from_icon_name("system-search-symbolic");
    search_btn.set_tooltip_text(Some("Search (Ctrl+P)"));

    let content_header = adw::HeaderBar::new();
    content_header.pack_start(&new_query_btn);
    content_header.pack_start(&search_btn);

    let menu = build_main_menu();
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Menu principal")
        .menu_model(&menu)
        .build();
    content_header.pack_end(&menu_button);

    // Reflects the currently selected tab's title — every `page.set_title(...)` call above
    // happens before the page is selected, so listening for selection changes alone is enough.
    let content_title = adw::WindowTitle::new("Draco", "");
    content_header.set_title_widget(Some(&content_title));
    tab_view.connect_selected_page_notify(clone!(
        #[strong]
        content_title,
        move |view| {
            let title = view.selected_page().map(|page| page.title().to_string()).unwrap_or_else(|| "Draco".to_string());
            content_title.set_title(&title);
        }
    ));

    let tabs_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    tabs_box.append(&tab_bar);
    tabs_box.append(&tab_view);

    // Shown instead of the (empty) tab area until the first tab is opened. Loads the logo
    // straight from its exact gresource path rather than through icon-theme name lookup — the
    // themed lookup (`add_resource_path` + `icon_name(APP_ID)`) rendered a broken-image glyph
    // here, and this sidesteps that resolution step entirely.
    let logo_image = gtk::Image::from_resource(
        "/org/lyraos/Draco/icons/hicolor/1024x1024/apps/org.lyraos.Draco.png",
    );
    logo_image.set_pixel_size(128);
    let empty_state = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    empty_state.append(&logo_image);
    empty_state.append(
        &gtk::Label::builder()
            .label("Draco")
            .css_classes(["title-1"])
            .build(),
    );
    empty_state.append(
        &gtk::Label::builder()
            .label("Open a query, or explore a table from the sidebar")
            .css_classes(["dim-label"])
            .build(),
    );

    let content_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    content_stack.add_named(&empty_state, Some("empty"));
    content_stack.add_named(&tabs_box, Some("tabs"));
    content_stack.set_visible_child_name("empty");

    tab_view.connect_notify_local(
        Some("n-pages"),
        clone!(
            #[strong]
            content_stack,
            move |view, _| {
                content_stack.set_visible_child_name(if view.n_pages() > 0 {
                    "tabs"
                } else {
                    "empty"
                });
            }
        ),
    );

    let content_toolbar = adw::ToolbarView::new();
    content_toolbar.add_top_bar(&content_header);
    content_toolbar.set_content(Some(&content_stack));
    let content_page = adw::NavigationPage::builder()
        .title("Draco")
        .child(&content_toolbar)
        .build();

    // Every call opens an independent tab (its own buffer, connection selection and results
    // grid) in the same outer `AdwTabView` as table/dashboard/admin tabs — numbered so several
    // open at once stay distinguishable in the tab bar instead of all reading "Query".
    let query_tab_count: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let open_new_query = clone!(
        #[strong]
        runtime,
        #[strong]
        manager,
        #[weak]
        tab_view,
        #[strong]
        query_tab_count,
        #[strong]
        query_run_registry,
        #[strong]
        toast_overlay,
        #[strong]
        open_ai_with_message,
        move || {
            let connections = store::list_connections();
            let editor = QueryEditor::new(
                connections,
                runtime.clone(),
                manager.clone(),
                toast_overlay.clone(),
                None,
                clone!(
                    #[strong]
                    open_ai_with_message,
                    move |conn_id, message| open_ai_with_message(conn_id, message)
                ),
            );
            let page = tab_view.append(editor.widget());
            let n = query_tab_count.get() + 1;
            query_tab_count.set(n);
            page.set_title(&format!("Query {n}"));
            editor.bind_tab_page(page.clone());
            query_run_registry
                .borrow_mut()
                .push((page.clone(), editor.run_action(), editor.script_action()));
            tab_view.set_selected_page(&page);
        }
    );
    new_query_btn.connect_clicked(clone!(
        #[strong]
        open_new_query,
        move |_| open_new_query()
    ));

    let split_view = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .min_sidebar_width(280.0)
        .max_sidebar_width(420.0)
        .build();

    toast_overlay.set_child(Some(&split_view));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Draco")
        .default_width(1280)
        .default_height(820)
        .content(&toast_overlay)
        .build();

    install_window_actions(&window, runtime.clone());

    add_btn.connect_clicked(clone!(
        #[weak]
        window,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        explorer,
        #[strong]
        toast_overlay,
        move |_| {
            connection_dialog::open(
                &window,
                runtime.clone(),
                None,
                toast_overlay.clone(),
                clone!(
                    #[strong]
                    runtime,
                    #[strong]
                    manager,
                    #[strong]
                    explorer,
                    move |_saved| {
                        refresh_connections(&explorer, &manager, &runtime);
                    }
                ),
            );
        }
    ));

    let open_search = clone!(
        #[weak]
        window,
        #[strong]
        runtime,
        #[strong]
        manager,
        #[strong]
        open_table,
        move || {
            let connections = store::list_connections();
            search::open(
                &window,
                connections,
                runtime.clone(),
                manager.clone(),
                clone!(
                    #[strong]
                    open_table,
                    move |conn_id, schema, table| (open_table)(conn_id, schema, table)
                ),
            );
        }
    );
    search_btn.connect_clicked(clone!(
        #[strong]
        open_search,
        move |_| open_search()
    ));

    let search_action = gio::SimpleAction::new("search", None);
    search_action.connect_activate(clone!(
        #[strong]
        open_search,
        move |_, _| open_search()
    ));
    window.add_action(&search_action);
    app.set_accels_for_action("win.search", &["<Primary>p"]);

    let new_query_action = gio::SimpleAction::new("new-query", None);
    new_query_action.connect_activate(clone!(
        #[strong]
        open_new_query,
        move |_, _| open_new_query()
    ));
    window.add_action(&new_query_action);
    app.set_accels_for_action("win.new-query", &["<Primary>t"]);

    let run_query_action = gio::SimpleAction::new("run-query", None);
    run_query_action.connect_activate(clone!(
        #[strong]
        tab_view,
        #[strong]
        query_run_registry,
        move |_, _| {
            if let Some(page) = tab_view.selected_page() {
                if let Some((_, run, _)) = query_run_registry.borrow().iter().find(|(p, _, _)| *p == page)
                {
                    run(false);
                }
            }
        }
    ));
    window.add_action(&run_query_action);
    app.set_accels_for_action("win.run-query", &["F8", "<Primary>Return"]);

    let explain_query_action = gio::SimpleAction::new("explain-query", None);
    explain_query_action.connect_activate(clone!(
        #[strong]
        tab_view,
        #[strong]
        query_run_registry,
        move |_, _| {
            if let Some(page) = tab_view.selected_page() {
                if let Some((_, run, _)) = query_run_registry.borrow().iter().find(|(p, _, _)| *p == page)
                {
                    run(true);
                }
            }
        }
    ));
    window.add_action(&explain_query_action);
    app.set_accels_for_action("win.explain-query", &["F10"]);

    let run_script_action = gio::SimpleAction::new("run-script", None);
    run_script_action.connect_activate(clone!(
        #[strong]
        tab_view,
        #[strong]
        query_run_registry,
        move |_, _| {
            if let Some(page) = tab_view.selected_page() {
                if let Some((_, _, run_script)) = query_run_registry.borrow().iter().find(|(p, _, _)| *p == page)
                {
                    run_script();
                }
            }
        }
    ));
    window.add_action(&run_script_action);
    app.set_accels_for_action("win.run-script", &["F5"]);

    window.present();
}

/// Builds the application menu shown by the main content header bar.
///
/// The menu points at window actions so the same operations can later be exposed through
/// shortcuts without duplicating their implementation.
fn build_main_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let settings = gio::Menu::new();
    settings.append(Some("Configurações"), Some("win.settings"));
    menu.append_section(None, &settings);

    let application = gio::Menu::new();
    application.append(Some("Sobre o Draco"), Some("win.about"));
    menu.append_section(None, &application);

    menu
}

fn install_window_actions(window: &adw::ApplicationWindow, runtime: tokio::runtime::Handle) {
    let settings_action = gio::SimpleAction::new("settings", None);
    settings_action.connect_activate(clone!(
        #[weak]
        window,
        #[strong]
        runtime,
        move |_, _| settings::show(&window, runtime.clone())
    ));
    window.add_action(&settings_action);

    let about_action = gio::SimpleAction::new("about", None);
    about_action.connect_activate(clone!(
        #[weak]
        window,
        move |_, _| {
            let dialog = adw::AboutDialog::builder()
                .application_name("Draco")
                // Reuse the canonical application icon. Keeping a second icon name here lets
                // the desktop/theme cache retain a stale About-dialog image independently.
                .application_icon("org.lyraos.Draco")
                .developer_name("Lyra OS")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://github.com/britors/Draco")
                .issue_url("https://github.com/britors/Draco/issues")
                .license_type(gtk::License::Gpl30)
                .build();
            dialog.set_developers(&["Rodrigo Brito"]);
            dialog.present(Some(&window));
        }
    ));
    window.add_action(&about_action);
}

fn refresh_connections(
    explorer: &Rc<Explorer>,
    manager: &Arc<Mutex<ConnectionManager>>,
    runtime: &tokio::runtime::Handle,
) {
    let connections = store::list_connections();
    if let Ok(mut mgr) = manager.try_lock() {
        mgr.sync_connections(connections.clone());
    }
    explorer.set_connections(connections, runtime.clone(), manager.clone());
}
