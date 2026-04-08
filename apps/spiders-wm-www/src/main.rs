use clsx::clsx;
use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use wasm_bindgen::{JsCast, closure::Closure};

mod app_state;
mod bindings;
mod browser_ipc;
mod components;
mod editor_files;
mod editor_host;
mod layout_runtime;
mod perf;
mod session;
mod views;
mod workspace;

use app_state::AppState;
use layout_runtime::PreviewRenderRequest;
use perf::{log_timing, now_ms};
use views::cli::CliView;
use views::editor::EditorView;
use views::preview::PreviewView;
use views::system::SystemView;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    let app_state = AppState::new();
    browser_ipc::initialize(app_state.session);
    provide_context(app_state);

    install_keyboard_listener(app_state);
    install_config_loader(app_state);
    install_preview_renderer(app_state);

    view! {
        <Router>
            <AppShell />
        </Router>
    }
}

#[component]
fn AppShell() -> impl IntoView {
    let location = use_location();

    let tab_class = move |path: &'static str| {
        let current = location.pathname.get();
        let is_active = match path {
            "/" => current == "/" || current == "/preview",
            _ => current == path,
        };

        clsx!(
            "inline-flex items-center border border-b-0 px-3 py-1.5 text-sm transition duration-150",
            (is_active, "border-terminal-border-strong bg-terminal-bg text-terminal-fg-strong"),
            (
                !is_active,
                "border-terminal-border bg-terminal-bg-bar text-terminal-dim opacity-70 hover:text-terminal-fg hover:opacity-100"
            )
        )
    };

    view! {
        <main class="flex overflow-hidden flex-col h-screen bg-terminal-bg text-terminal-fg">
            <div class="overflow-hidden flex-1 min-h-0">
                <Routes fallback=NotFoundRoute>
                    <Route path=path!("/") view=PreviewRoute />
                    <Route path=path!("/preview") view=PreviewRoute />
                    <Route path=path!("/editor") view=EditorRoute />
                    <Route path=path!("/system") view=SystemRoute />
                    <Route path=path!("/cli") view=CliRoute />
                </Routes>
            </div>

            <div class="px-2 pb-1 border-t border-terminal-border bg-terminal-bg-subtle">
                <nav class="flex overflow-x-auto flex-wrap gap-1">
                    <A href="/" attr:class=move || tab_class("/")>
                        "1:preview"
                    </A>
                    <A href="/editor" attr:class=move || tab_class("/editor")>
                        "2:editor"
                    </A>
                    <A href="/system" attr:class=move || tab_class("/system")>
                        "3:system"
                    </A>
                    <A href="/cli" attr:class=move || tab_class("/cli")>
                        "4:cli"
                    </A>
                </nav>
            </div>
        </main>
    }
}

#[component]
fn PreviewRoute() -> impl IntoView {
    view! { <PreviewView /> }
}

#[component]
fn EditorRoute() -> impl IntoView {
    view! { <EditorView /> }
}

#[component]
fn SystemRoute() -> impl IntoView {
    view! { <SystemView /> }
}

#[component]
fn CliRoute() -> impl IntoView {
    view! { <CliView /> }
}

#[component]
fn NotFoundRoute() -> impl IntoView {
    view! {
        <section class="grid gap-2 p-3 border border-terminal-border bg-terminal-bg-subtle">
            <div class="text-xs uppercase text-terminal-dim tracking-[0.18em]">
                "route://missing"
            </div>
            <div class="text-lg text-terminal-fg-strong">"Not found"</div>
        </section>
    }
}

fn install_preview_renderer(app_state: AppState) {
    Effect::new(move |_| {
        let effect_started = now_ms();
        let _request_id = app_state.preview_eval_request.get();
        let buffers = app_state.editor_buffers.get();
        let snapshot = app_state.session.get_untracked();
        let preview_request = PreviewRenderRequest::from_state(&buffers, &snapshot);
        let preview_request_key = format!("{preview_request:?}");

        if app_state.latest_preview_request_key.get_untracked() == preview_request_key {
            return;
        }

        app_state.latest_preview_request_key.set(preview_request_key.clone());

        log_timing(
            "preview-renderer.request-key",
            effect_started,
            format!(
                "layout={} windows={}",
                preview_request.active_layout.as_str(),
                preview_request.runtime_state.windows.len()
            ),
        );

        wasm_bindgen_futures::spawn_local(async move {
            let async_started = now_ms();
            match layout_runtime::evaluate_layout_source(&preview_request).await {
                Ok(layout) => {
                    if app_state.latest_preview_request_key.get_untracked() != preview_request_key {
                        return;
                    }

                    let loaded_preview_layout = app_state.loaded_preview_layout.get_untracked();
                    let layout_unchanged = loaded_preview_layout.as_ref().is_some_and(|loaded| {
                        loaded.layout == layout.layout && loaded.config == layout.config
                    });

                    let apply_started = now_ms();
                    app_state.apply_loaded_preview_layout(layout.clone());
                    if !layout_unchanged {
                        let mut scene_cache = app_state.preview_scene_cache.get_untracked();
                        app_state.session.update(|state| {
                            state.apply_layout_source(
                                layout.layout,
                                Some(&layout.config),
                                Some(&mut scene_cache),
                            )
                        });
                        app_state.preview_scene_cache.set(scene_cache);
                    }
                    log_timing(
                        "preview-renderer.apply-layout",
                        apply_started,
                        format!(
                            "layout={} visible_windows={} unchanged={}",
                            preview_request.active_layout.as_str(),
                            preview_request
                                .runtime_state
                                .windows
                                .iter()
                                .filter(|window| {
                                    window.workspace_name
                                        == preview_request.runtime_state.active_workspace_name
                                })
                                .count(),
                            layout_unchanged
                        ),
                    );
                    log_timing(
                        "preview-renderer.total",
                        async_started,
                        format!("layout={}", preview_request.active_layout.as_str()),
                    );
                }
                Err(error) => {
                    if app_state.latest_preview_request_key.get_untracked() != preview_request_key {
                        return;
                    }

                    app_state.apply_preview_failure_state();
                    app_state.session.update(|state| state.apply_preview_failure("layout", error));
                    log_timing(
                        "preview-renderer.total",
                        async_started,
                        format!("layout={} error", preview_request.active_layout.as_str()),
                    );
                }
            }
        });
    });
}

fn install_config_loader(app_state: AppState) {
    Effect::new(move |_| {
        let buffers = app_state.editor_buffers.get();
        let request_key = format!("{buffers:?}");

        if app_state.latest_config_request_key.get_untracked() == request_key {
            return;
        }

        app_state.latest_config_request_key.set(request_key.clone());

        wasm_bindgen_futures::spawn_local(async move {
            match app_state::load_config_from_buffers(&buffers).await {
                Ok(config) => {
                    if app_state.latest_config_request_key.get_untracked() != request_key {
                        return;
                    }
                    app_state.apply_loaded_config(config);
                }
                Err(_) => {
                    if app_state.latest_config_request_key.get_untracked() != request_key {
                        return;
                    }
                    app_state.apply_config_error();
                }
            }
        });
    });
}

fn install_keyboard_listener(app_state: AppState) {
    Effect::new(move |_| {
        let Some(window) = web_sys::window() else {
            return;
        };

        let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::wrap(Box::new(
            move |event: web_sys::KeyboardEvent| {
                let keydown_started = now_ms();
                let is_preview_route = web_sys::window()
                    .and_then(|window| window.location().pathname().ok())
                    .map(|pathname| pathname == "/" || pathname == "/preview")
                    .unwrap_or(false);

                if !is_preview_route {
                    return;
                }

                let bindings = app_state.parsed_bindings();
                let Some(entry) = bindings.entries.iter().find(|entry| {
                    bindings::matches_web_keyboard_event(entry, &event, &bindings.mod_key)
                }) else {
                    return;
                };
                let Some(command) = entry.command.clone() else {
                    return;
                };

                event.prevent_default();
                let command_label = format!("{:?}", command);
                let render_action = app_state.mutate_session(|state| state.apply_command(command));
                log_timing("preview-input.apply-command", keydown_started, command_label.clone());

                let refresh_started = now_ms();
                app_state.apply_preview_render_action(render_action);
                log_timing(
                    "preview-input.schedule-refresh",
                    refresh_started,
                    format!("{command_label} action={render_action:?}"),
                );
            },
        ));

        let _ =
            window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    });
}
