use clsx::clsx;
use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use wasm_bindgen::{JsCast, closure::Closure};

mod app_state;
mod bindings;
mod components;
mod editor_files;
mod editor_host;
mod layout_runtime;
mod session;
mod views;
mod workspace;

use app_state::AppState;
use layout_runtime::PreviewRenderRequest;
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
            (
                is_active,
                "border-terminal-border-strong bg-terminal-bg text-terminal-fg-strong"
            ),
            (
                !is_active,
                "border-terminal-border bg-terminal-bg-bar text-terminal-dim opacity-70 hover:text-terminal-fg hover:opacity-100"
            )
        )
    };

    view! {
        <main class="flex overflow-hidden flex-col h-screen bg-terminal-bg text-terminal-fg">
            <div class="px-2 pt-1 border-b border-terminal-border bg-terminal-bg-subtle">
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
                </nav>
            </div>

            <div class="overflow-hidden flex-1 p-2 min-h-0">
                <Routes fallback=NotFoundRoute>
                    <Route path=path!("/") view=PreviewRoute />
                    <Route path=path!("/preview") view=PreviewRoute />
                    <Route path=path!("/editor") view=EditorRoute />
                    <Route path=path!("/system") view=SystemRoute />
                </Routes>
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
        let buffers = app_state.editor_buffers.get();
        let snapshot = app_state.session.get();
        let preview_request = PreviewRenderRequest::from_state(&buffers, &snapshot);
        let preview_request_key = format!("{preview_request:?}");

        if app_state.latest_preview_request_key.get_untracked() == preview_request_key {
            return;
        }

        app_state
            .latest_preview_request_key
            .set(preview_request_key.clone());

        wasm_bindgen_futures::spawn_local(async move {
            match layout_runtime::evaluate_layout_source(&preview_request).await {
                Ok(layout) => {
                    if app_state.latest_preview_request_key.get_untracked() != preview_request_key {
                        return;
                    }

                    app_state.apply_loaded_preview_layout(layout.clone());
                    app_state.session.update(|state| state.apply_layout_source(layout.layout, Some(&layout.config)));
                }
                Err(error) => {
                    if app_state.latest_preview_request_key.get_untracked() != preview_request_key {
                        return;
                    }

                    app_state.apply_preview_failure_state();
                    app_state
                        .session
                        .update(|state| state.apply_preview_failure("layout", error));
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
                app_state.session.update(|state| state.apply_command(command));
                app_state.refresh_preview_from_loaded_state();
            },
        ));

        let _ =
            window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
        });
}
