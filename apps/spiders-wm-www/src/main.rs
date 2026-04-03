use clsx::clsx;
use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

mod app_state;
mod bindings;
mod components;
mod editor_host;
mod layout_runtime;
mod session;
mod views;
mod workspace;

use app_state::AppState;
#[cfg(target_arch = "wasm32")]
use bindings::parse_bindings_source;
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
    install_preview_renderer(app_state);

    view! {
        <Router>
            <AppShell/>
        </Router>
    }
}

#[component]
fn AppShell() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let location = use_location();

    let tab_class = move |path: &'static str| {
        let current = location.pathname.get();
        let is_active = match path {
            "/" => current == "/" || current == "/preview",
            _ => current == path,
        };

        clsx!(
            "inline-flex items-center rounded-t-[18px] border border-b-0 px-4 py-3 text-sm font-medium tracking-[0.04em] transition duration-150",
            (
                is_active,
                "border-sky-300/40 bg-white/[0.08] text-white shadow-[0_-1px_0_rgba(255,255,255,0.08)]"
            ),
            (
                !is_active,
                "border-white/10 bg-white/[0.03] text-slate-300 hover:border-sky-300/25 hover:bg-white/[0.06] hover:text-white"
            )
        )
    };

    view! {
        <main class="min-h-screen bg-[radial-gradient(circle_at_top,rgba(14,165,233,0.14),transparent_36%),linear-gradient(180deg,#04070b_0%,#09111a_52%,#05080c_100%)] px-4 py-6 text-slate-100 sm:px-6 lg:px-8">
            <div class="mx-auto flex w-full max-w-[96rem] flex-col gap-4">
                <nav class="flex flex-wrap gap-2 overflow-x-auto pb-1">
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

                <section class="grid gap-3 rounded-[28px] border border-white/10 bg-[linear-gradient(180deg,rgba(11,24,37,0.92),rgba(7,15,23,0.92))] p-5 shadow-[0_28px_80px_rgba(0,0,0,0.34)] backdrop-blur-[18px] sm:p-6">
                    <p class="text-[0.72rem] uppercase tracking-[0.18em] text-sky-200/70">
                        "Leptos CSR Prototype"
                    </p>
                    <h1 class="text-[clamp(1.75rem,2.5vw,2.7rem)] font-semibold tracking-[-0.04em] text-white">
                        "Rust-first playground shell"
                    </h1>
                    <p class="max-w-[68ch] text-sm leading-7 text-slate-300 sm:text-[0.98rem]">
                        "This app now follows the playground runtime model: source files live in app state, bindings are parsed from those buffers, and preview geometry is recomputed at runtime instead of from build.rs output."
                    </p>
                    <p class="rounded-[20px] border border-sky-300/15 bg-black/20 px-4 py-3 font-mono text-sm text-sky-100/90 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
                        {move || app_state.session.get().prompt().to_string()}
                    </p>
                </section>

                <Routes fallback=NotFoundRoute>
                    <Route path=path!("/") view=PreviewRoute/>
                    <Route path=path!("/preview") view=PreviewRoute/>
                    <Route path=path!("/editor") view=EditorRoute/>
                    <Route path=path!("/system") view=SystemRoute/>
                </Routes>
            </div>
        </main>
    }
}

#[component]
fn PreviewRoute() -> impl IntoView {
    view! { <PreviewView/> }
}

#[component]
fn EditorRoute() -> impl IntoView {
    view! { <EditorView/> }
}

#[component]
fn SystemRoute() -> impl IntoView {
    view! { <SystemView/> }
}

#[component]
fn NotFoundRoute() -> impl IntoView {
    view! {
        <section class="grid rounded-[28px] border border-white/10 bg-[linear-gradient(180deg,rgba(11,24,37,0.92),rgba(7,15,23,0.92))] p-4 shadow-[0_28px_80px_rgba(0,0,0,0.34)] backdrop-blur-[18px]">
            <div class="rounded-[22px] border border-white/10 bg-white/[0.03] p-4">
                <p class="text-[0.72rem] uppercase tracking-[0.18em] text-sky-200/70">"route://missing"</p>
                <h2 class="mt-2 text-xl font-semibold tracking-[-0.03em] text-white">"Not found"</h2>
            </div>
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
            match layout_runtime::evaluate_layout_renderable(&preview_request).await {
                Ok(layout_renderable) => {
                    if app_state.latest_preview_request_key.get_untracked() != preview_request_key {
                        return;
                    }

                    app_state
                        .session
                        .update(|state| state.apply_layout_renderable(layout_renderable));
                }
                Err(error) => {
                    if app_state.latest_preview_request_key.get_untracked() != preview_request_key {
                        return;
                    }

                    app_state
                        .session
                        .update(|state| state.apply_preview_failure("layout", error));
                }
            }
        });
    });
}

fn install_keyboard_listener(app_state: AppState) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = app_state;

    #[cfg(target_arch = "wasm32")]
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

                let buffers = app_state.editor_buffers.get_untracked();
                let bindings = parse_bindings_source(binding_source(&buffers));
                let Some(entry) = bindings.entries.iter().find(|entry| {
                    bindings::matches_web_keyboard_event(entry, &event, &bindings.mod_key)
                }) else {
                    return;
                };

                event.prevent_default();
                app_state
                    .session
                    .update(|state| state.apply_command(entry.command.clone()));
            },
        ));

        let _ =
            window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    });
}

#[cfg(target_arch = "wasm32")]
fn binding_source(buffers: &std::collections::BTreeMap<workspace::EditorFileId, String>) -> &str {
    buffers
        .get(&workspace::EditorFileId::ConfigBindings)
        .map(String::as_str)
        .unwrap_or_else(|| workspace::initial_content(workspace::EditorFileId::ConfigBindings))
}
