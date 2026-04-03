use std::collections::BTreeMap;

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast};

mod bindings;
mod components;
mod editor_host;
mod layout_runtime;
mod session;
mod views;
mod workspace;

use bindings::parse_bindings_source;
use layout_runtime::PreviewRenderRequest;
use session::{PreviewEnvironment, PreviewLayoutId, PreviewSessionState};
use views::editor::EditorView;
use views::preview::PreviewView;
use views::system::SystemView;
use workspace::{
    initial_content, initial_editor_buffers, initial_open_directories, initial_open_editor_files,
    parse_workspace_names, EditorFileId,
};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");

fn main() {
    dioxus::launch(App);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaygroundTab {
    Preview,
    Editor,
    System,
}

impl PlaygroundTab {
    const ALL: [Self; 3] = [Self::Preview, Self::Editor, Self::System];

    fn label(self) -> &'static str {
        match self {
            Self::Preview => "1:preview",
            Self::Editor => "2:editor",
            Self::System => "3:system",
        }
    }
}

fn build_preview_environment(buffers: &BTreeMap<EditorFileId, String>) -> PreviewEnvironment {
    let root_css = buffers
        .get(&EditorFileId::RootCss)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::RootCss));
    let master_css = buffers
        .get(&EditorFileId::LayoutCss)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::LayoutCss));
    let focus_css = buffers
        .get(&EditorFileId::FocusReproLayoutCss)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::FocusReproLayoutCss));
    let config_source = buffers
        .get(&EditorFileId::Config)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::Config));

    PreviewEnvironment {
        workspace_names: parse_workspace_names(config_source),
        stylesheets: BTreeMap::from([
            (
                PreviewLayoutId::MasterStack,
                format!("{root_css}\n\n{master_css}"),
            ),
            (
                PreviewLayoutId::FocusRepro,
                format!("{root_css}\n\n{focus_css}"),
            ),
        ]),
    }
}

fn binding_source(buffers: &BTreeMap<EditorFileId, String>) -> &str {
    buffers
        .get(&EditorFileId::ConfigBindings)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::ConfigBindings))
}

#[component]
fn App() -> Element {
    let mut active_tab = use_signal(|| PlaygroundTab::Preview);
    let mut editor_buffers = use_signal(initial_editor_buffers);
    let active_file_id = use_signal(|| Some(EditorFileId::LayoutTsx));
    let open_file_ids = use_signal(initial_open_editor_files);
    let directory_open_state = use_signal(initial_open_directories);
    let latest_preview_request_key = use_signal(String::new);
    let initial_environment = build_preview_environment(&initial_editor_buffers());
    let mut session =
        use_signal(|| PreviewSessionState::new(PreviewLayoutId::MasterStack, initial_environment));

    let buffers_snapshot = editor_buffers();
    let parsed_bindings = parse_bindings_source(binding_source(&buffers_snapshot));

    {
        let environment = build_preview_environment(&buffers_snapshot);
        let mut session_for_environment = session;

        use_effect(move || {
            session_for_environment.with_mut(|state| state.sync_environment(environment.clone()));
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        let session_for_keys = session;
        let active_tab_for_keys = active_tab;
        let buffers_for_keys = editor_buffers;
        let mut keyboard_listener_installed = use_signal(|| false);

        use_effect(move || {
            if keyboard_listener_installed() {
                return;
            }

            let Some(window) = web_sys::window() else {
                return;
            };
            keyboard_listener_installed.set(true);

            let mut keyboard_session = session_for_keys;

            let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::wrap(Box::new(
                move |event: web_sys::KeyboardEvent| {
                    if active_tab_for_keys() != PlaygroundTab::Preview {
                        return;
                    }

                    let buffers = buffers_for_keys();
                    let bindings = parse_bindings_source(binding_source(&buffers));
                    let Some(entry) = bindings.entries.iter().find(|entry| {
                        bindings::matches_web_keyboard_event(entry, &event, &bindings.mod_key)
                    }) else {
                        return;
                    };

                    event.prevent_default();
                    keyboard_session.with_mut(|state| state.apply_command(entry.command.clone()));
                },
            ));

            let _ = window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
            closure.forget();
        });
    }

    let snapshot = session();
    let preview_request = PreviewRenderRequest::from_state(&buffers_snapshot, &snapshot);
    let preview_request_key = format!("{preview_request:?}");

    {
        let preview_request = preview_request.clone();
        let preview_request_key = preview_request_key.clone();
        let latest_preview_request_key = latest_preview_request_key;
        let session_for_preview = session;

        use_effect(move || {
            let mut latest_preview_request_key = latest_preview_request_key;
            if latest_preview_request_key() == preview_request_key {
                return;
            }

            latest_preview_request_key.set(preview_request_key.clone());

            spawn({
                let preview_request = preview_request.clone();
                let preview_request_key = preview_request_key.clone();
                let latest_preview_request_key = latest_preview_request_key;
                let mut session_for_preview = session_for_preview;

                async move {
                    match layout_runtime::evaluate_layout_renderable(&preview_request).await {
                        Ok(layout_renderable) => {
                            if latest_preview_request_key() != preview_request_key {
                                return;
                            }

                            session_for_preview
                                .with_mut(|state| state.apply_layout_renderable(layout_renderable));
                        }
                        Err(error) => {
                            if latest_preview_request_key() != preview_request_key {
                                return;
                            }

                            session_for_preview
                                .with_mut(|state| state.apply_preview_failure("layout", error));
                        }
                    }
                }
            });
        });
    }

    rsx! {
        document::Title { "spiders-wm-web" }
        document::Link { rel: "icon", href: FAVICON }
        document::Stylesheet { href: TAILWIND_CSS }
        document::Stylesheet { href: MAIN_CSS }

        main { class: "shell",

            div { class: "tab-strip",
                for tab in PlaygroundTab::ALL {
                    button {
                        class: if tab == active_tab() { "tab-button is-active" } else { "tab-button" },
                        onclick: move |_| active_tab.set(tab),
                        "{tab.label()}"
                    }
                }
            }

            section { class: "hero-card",

                p { class: "eyebrow", "Dioxus Prototype" }
                h1 { "Rust-first playground shell" }
                p { class: "lede",
                    "This app now follows the playground runtime model: source files live in app state, bindings are parsed from those buffers, and preview geometry is recomputed at runtime instead of from build.rs output."
                }
                p { class: "prompt", "{snapshot.prompt()}" }
            }

            if active_tab() == PlaygroundTab::Preview {
                PreviewView { session }
            }

            if active_tab() == PlaygroundTab::Editor {
                EditorView {
                    session,
                    editor_buffers,
                    active_file_id,
                    open_file_ids,
                    directory_open_state,
                    binding_entries: parsed_bindings.entries.clone(),
                    on_update_buffer: move |(file_id, next_value)| {
                        editor_buffers
                            .with_mut(|buffers| {
                                buffers.insert(file_id, next_value);
                                let next_environment = build_preview_environment(buffers);
                                session.with_mut(|state| state.sync_environment(next_environment));
                            });
                    },
                }
            }

            if active_tab() == PlaygroundTab::System {
                SystemView {
                    session,
                    active_file_id,
                    binding_entries: parsed_bindings.entries.clone(),
                }
            }
        }
    }
}
