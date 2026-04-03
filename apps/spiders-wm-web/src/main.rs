use std::collections::BTreeMap;

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

mod bindings;
mod layout_runtime;
mod session;
mod workspace;

use bindings::parse_bindings_source;
use layout_runtime::PreviewRenderRequest;
use session::{PreviewEnvironment, PreviewLayoutId, PreviewSessionState, PreviewSessionWindow};
use spiders_core::navigation::NavigationDirection;
use workspace::{
    EDITOR_FILES, EditorFileId, WORKSPACE_ROOT, file_by_id, initial_content,
    initial_editor_buffers, parse_workspace_names,
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

fn pane_style(window: &PreviewSessionWindow, canvas_width: i32, canvas_height: i32) -> String {
    let left = window.geometry.x as f32 / canvas_width as f32 * 100.0;
    let top = window.geometry.y as f32 / canvas_height as f32 * 100.0;
    let width = window.geometry.width as f32 / canvas_width as f32 * 100.0;
    let height = window.geometry.height as f32 / canvas_height as f32 * 100.0;

    format!(
        "left: {left:.3}%; top: {top:.3}%; width: calc({width:.3}% - 0.6rem); height: calc({height:.3}% - 0.6rem); --accent: {};",
        window.accent,
    )
}

#[component]
fn App() -> Element {
    let mut active_tab = use_signal(|| PlaygroundTab::Preview);
    let mut editor_buffers = use_signal(initial_editor_buffers);
    let mut active_file_id = use_signal(|| EditorFileId::LayoutTsx);
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

            let _ = window.add_event_listener_with_callback(
                "keydown",
                closure.as_ref().unchecked_ref(),
            );
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

    let focused_window_id = snapshot.focused_window_id();
    let focused_window_label = focused_window_id
        .as_ref()
        .map(|window_id| snapshot.window_name(window_id))
        .unwrap_or_else(|| "none".to_string());
    let current_scope_path = snapshot.current_scope_path();
    let remembered_rows = snapshot.remembered_rows();
    let workspace_names = snapshot.workspace_names.clone();
    let active_workspace_name = snapshot.active_workspace_name.clone();
    let visible_windows = snapshot.visible_windows();
    let claimed_windows = snapshot.claimed_visible_windows();
    let unclaimed_windows = snapshot.unclaimed_visible_windows();
    let canvas_width = snapshot.canvas_width();
    let canvas_height = snapshot.canvas_height();
    let event_log = snapshot.event_log.clone();
    let diagnostics = snapshot.diagnostics.clone();
    let selected_layout_name = snapshot.selected_layout_name();
    let binding_entries = parsed_bindings.entries.clone();
    let active_file = file_by_id(active_file_id());
    let active_buffer = buffers_snapshot
        .get(&active_file.id)
        .cloned()
        .unwrap_or_else(|| initial_content(active_file.id).to_string());

    let state_rows = vec![
        ("layout".to_string(), snapshot.active_layout.title().to_string()),
        ("focused".to_string(), focused_window_label.clone()),
        ("workspace".to_string(), active_workspace_name.clone()),
        ("windows".to_string(), snapshot.windows.len().to_string()),
        (
            "visible windows".to_string(),
            snapshot.visible_window_count().to_string(),
        ),
        (
            "claimed windows".to_string(),
            snapshot.claimed_visible_window_count().to_string(),
        ),
        (
            "remembered scopes".to_string(),
            remembered_rows.len().to_string(),
        ),
        (
            "active file".to_string(),
            active_file.path.to_string(),
        ),
        ("last action".to_string(), snapshot.last_action.clone()),
    ];
    let runtime_rows = vec![
        (
            "source bundle".to_string(),
            "done".to_string(),
            "The app now ships the same playground workspace files as runtime buffers instead of generating preview cache JSON at build time.".to_string(),
        ),
        (
            "preview runtime".to_string(),
            "done".to_string(),
            "Preview geometry now comes from browser-evaluated authored layout renderables plus live authored CSS buffers.".to_string(),
        ),
        (
            "bindings runtime".to_string(),
            "done".to_string(),
            "Keyboard dispatch now parses the live bindings buffer instead of a generated profile artifact.".to_string(),
        ),
        (
            "tsx runtime".to_string(),
            "done".to_string(),
            "Active layout TSX now compiles from the runtime source bundle in Rust, executes in the browser module graph, and flows back into wasm preview compute.".to_string(),
        ),
    ];

    let workspace_chips = workspace_names.iter().cloned().map(|workspace_name| {
        let target_workspace = workspace_name.clone();
        let is_active = workspace_name == active_workspace_name;

        rsx! {
            button {
                class: if is_active { "workspace-chip is-active" } else { "workspace-chip" },
                onclick: move |_| session.with_mut(|state| state.select_workspace(target_workspace.clone())),
                "{workspace_name}"
            }
        }
    });

    let visible_window_rows = visible_windows.iter().cloned().map(|window| {
        let title = window.display_title().to_string();
        let badge = window.badge.clone();
        let subtitle = window.subtitle.clone();
        let geometry = format!("{} × {}", window.geometry.width, window.geometry.height);

        rsx! {
            li {
                span { class: "memory-scope", "{badge} · {subtitle}" }
                span { class: "memory-window", "{title} · {geometry}" }
            }
        }
    });

    let unclaimed_window_rows = unclaimed_windows.iter().cloned().map(|window| {
        let title = window.display_title().to_string();
        let badge = window.badge.clone();
        let subtitle = window.subtitle.clone();

        rsx! {
            li {
                span { class: "memory-scope", "{badge} · {subtitle}" }
                span { class: "memory-window", "{title}" }
            }
        }
    });

    let remembered_scope_rows = remembered_rows.iter().cloned().map(|(scope, window_name)| {
        rsx! {
            li {
                span { class: "memory-scope", "{scope}" }
                span { class: "memory-window", "{window_name}" }
            }
        }
    });

    let state_row_nodes = state_rows.iter().cloned().map(|(label, value)| {
        rsx! {
            li {
                span { class: "memory-scope", "{label}" }
                span { class: "memory-window", "{value}" }
            }
        }
    });

    let runtime_row_nodes = runtime_rows.iter().cloned().map(|(label, status, detail)| {
        rsx! {
            li {
                span { class: "memory-scope", "{label} [{status}]" }
                span { class: "memory-window", "{detail}" }
            }
        }
    });

    let file_tabs = EDITOR_FILES.iter().copied().map(|file| {
        let file_id = file.id;
        let is_active = file_id == active_file_id();

        rsx! {
            button {
                class: if is_active { "workspace-chip is-active" } else { "workspace-chip" },
                onclick: move |_| active_file_id.set(file_id),
                "{file.label}"
            }
        }
    });

    let workspace_state_rows = workspace_names.iter().cloned().map(|workspace_name| {
        let layout_name = snapshot.layout_name_for_workspace(&workspace_name);
        let window_count = snapshot
            .windows
            .iter()
            .filter(|window| window.workspace_name == workspace_name)
            .count();

        rsx! {
            li {
                span { class: "memory-scope", "{workspace_name}" }
                span { class: "memory-window", "{window_count} windows · {layout_name}" }
            }
        }
    });

    let window_cards = claimed_windows.iter().cloned().map(|window| {
        let style = pane_style(&window, canvas_width, canvas_height);
        let is_focused = focused_window_id.as_ref() == Some(&window.id);
        let target_id = window.id.clone();
        let badge = window.badge.clone();
        let subtitle = window.subtitle.clone();
        let title = window.display_title().to_string();
        let dimensions = format!("{} × {}", window.geometry.width, window.geometry.height);
        let accent = window.accent.clone();

        rsx! {
            button {
                class: if is_focused { "wm-window is-focused" } else { "wm-window" },
                style: "{style}",
                onclick: move |_| session.with_mut(|state| state.set_focus(target_id.clone())),

                div { class: "wm-window-chrome",
                    span {
                        class: "wm-window-badge",
                        style: "background: color-mix(in srgb, {accent} 86%, white);",
                        "{badge}"
                    }
                    span { class: "wm-window-subtitle", "{subtitle}" }
                }
                h3 { "{title}" }
                p { class: "wm-window-meta", "{dimensions}" }
            }
        }
    });

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

            section { class: "control-strip",

                div { class: "scenario-switcher",
                    for layout in PreviewLayoutId::ALL {
                        button {
                            class: if layout == snapshot.active_layout { "scenario-pill is-active" } else { "scenario-pill" },
                            onclick: move |_| session.with_mut(|state| state.switch_layout(layout)),
                            span { class: "scenario-pill-title", "{layout.display_title()}" }
                            span { class: "scenario-pill-copy", "{layout.eyebrow()}" }
                        }
                    }
                }

                div { class: "dpad",

                    button {
                        class: "dpad-button dpad-up",
                        onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Up)),
                        "Up"
                    }
                    button {
                        class: "dpad-button dpad-left",
                        onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Left)),
                        "Left"
                    }
                    button {
                        class: "dpad-button dpad-right",
                        onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Right)),
                        "Right"
                    }
                    button {
                        class: "dpad-button dpad-down",
                        onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Down)),
                        "Down"
                    }
                    button {
                        class: "reset-button",
                        onclick: move |_| session.with_mut(PreviewSessionState::reset),
                        "Reset"
                    }
                }
            }

            if active_tab() == PlaygroundTab::Preview {
                section { class: "studio-grid",

                    article { class: "canvas-card",

                        div { class: "canvas-header",
                            div {
                                p { class: "eyebrow", "{snapshot.eyebrow()}" }
                                h2 { "{snapshot.display_title()}" }
                            }
                            p { class: "scenario-summary", "{snapshot.summary()}" }
                        }

                        div { class: "workspace-strip",
                            {workspace_chips}
                            span { class: "workspace-hint",
                                "{snapshot.visible_window_count()} visible windows"
                            }
                        }

                        div { class: "canvas-stage", {window_cards} }
                    }

                    article { class: "inspector-card",

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Focused" }
                            h3 { "{focused_window_label}" }
                            ul { class: "chip-list",
                                for scope in current_scope_path {
                                    li { class: "chip", "{scope}" }
                                }
                            }
                        }

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Visible windows" }
                            ul { class: "memory-list", {visible_window_rows} }
                        }

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Unclaimed" }
                            if unclaimed_windows.is_empty() {
                                p { class: "memory-window", "all visible windows are claimed" }
                            } else {
                                ul { class: "memory-list", {unclaimed_window_rows} }
                            }
                        }

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Remembered scopes" }
                            ul { class: "memory-list", {remembered_scope_rows} }
                        }

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Diagnostics" }
                            if diagnostics.is_empty() {
                                p { class: "memory-window", "no diagnostics" }
                            } else {
                                ul { class: "memory-list",
                                    for diagnostic in diagnostics.iter().cloned() {
                                        li {
                                            span { class: "memory-scope",
                                                "{diagnostic.level} · {diagnostic.source}"
                                            }
                                            span { class: "memory-window", "{diagnostic.message}" }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Event log" }
                            ul { class: "event-log",
                                for entry in event_log.iter().cloned() {
                                    li { "{entry}" }
                                }
                            }
                        }
                    }
                }
            }

            if active_tab() == PlaygroundTab::Editor {
                section { class: "studio-grid",

                    article { class: "canvas-card",

                        div { class: "canvas-header",
                            div {
                                p { class: "eyebrow", "Workspace source" }
                                h2 { "{active_file.path}" }
                            }
                            p { class: "scenario-summary",
                                "These buffers mirror the playground workspace bundle. Config, bindings, and CSS edits apply at runtime; TSX buffers are preserved in state for the Monaco/runtime-eval step."
                            }
                        }

                        div { class: "workspace-strip", {file_tabs} }

                        textarea {
                            class: "editor-textarea",
                            value: active_buffer,
                            spellcheck: false,
                            oninput: move |event| {
                                let next_value = event.value();
                                let current_file_id = active_file_id();

                                editor_buffers
                                    .with_mut(|buffers| {
                                        buffers.insert(current_file_id, next_value);
                                        let next_environment = build_preview_environment(buffers);
                                        session.with_mut(|state| state.sync_environment(next_environment));
                                    });
                            },
                        }
                    }

                    article { class: "inspector-card",

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "File metadata" }
                            ul { class: "memory-list",
                                li {
                                    span { class: "memory-scope", "language" }
                                    span { class: "memory-window", "{active_file.language}" }
                                }
                                li {
                                    span { class: "memory-scope", "workspace root" }
                                    span { class: "memory-window", "{WORKSPACE_ROOT}" }
                                }
                                li {
                                    span { class: "memory-scope", "preview layout" }
                                    span { class: "memory-window", "{selected_layout_name}" }
                                }
                            }
                        }

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Bindings" }
                            ul { class: "memory-list",
                                for entry in binding_entries.iter().cloned() {
                                    li {
                                        span { class: "memory-scope", "{entry.chord}" }
                                        span { class: "memory-window", "{entry.command_label}" }
                                    }
                                }
                            }
                        }

                        div { class: "inspector-panel",
                            p { class: "eyebrow", "Runtime notes" }
                            ul { class: "memory-list",
                                li {
                                    span { class: "memory-scope", "Applied live" }
                                    span { class: "memory-window",
                                        "config.ts, bindings.ts, root css, layout css, active layout tsx"
                                    }
                                }
                                li {
                                    span { class: "memory-scope", "Buffered next" }
                                    span { class: "memory-window",
                                        "additional imported source files are the next gap once the workspace bundle expands beyond the current playground set"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if active_tab() == PlaygroundTab::System {
                section { class: "system-grid",

                    article { class: "inspector-card",
                        div { class: "inspector-panel",
                            p { class: "eyebrow", "system://log" }
                            ul { class: "event-log",
                                for entry in event_log.iter().cloned() {
                                    li { "{entry}" }
                                }
                            }
                        }
                    }

                    article { class: "inspector-card",
                        div { class: "inspector-panel",
                            p { class: "eyebrow", "system://state" }
                            ul { class: "memory-list", {state_row_nodes} }
                        }
                    }

                    article { class: "inspector-card",
                        div { class: "inspector-panel",
                            p { class: "eyebrow", "system://runtime" }
                            ul { class: "memory-list", {runtime_row_nodes} }
                        }
                    }

                    article { class: "inspector-card",
                        div { class: "inspector-panel",
                            p { class: "eyebrow", "system://bindings" }
                            ul { class: "memory-list",
                                for entry in binding_entries.iter().cloned() {
                                    li {
                                        span { class: "memory-scope", "{entry.chord}" }
                                        span { class: "memory-window", "{entry.command_label}" }
                                    }
                                }
                            }
                        }
                    }

                    article { class: "inspector-card",
                        div { class: "inspector-panel",
                            p { class: "eyebrow", "system://workspaces" }
                            ul { class: "memory-list", {workspace_state_rows} }
                        }
                    }

                    article { class: "inspector-card",
                        div { class: "inspector-panel",
                            p { class: "eyebrow", "system://files" }
                            ul { class: "memory-list",
                                for file in EDITOR_FILES {
                                    li {
                                        span { class: "memory-scope", "{file.label}" }
                                        span { class: "memory-window", "{file.path}" }
                                    }
                                }
                            }
                        }
                    }

                    article { class: "inspector-card",
                        div { class: "inspector-panel",
                            p { class: "eyebrow", "system://diagnostics" }
                            if diagnostics.is_empty() {
                                p { class: "memory-window", "no diagnostics" }
                            } else {
                                ul { class: "memory-list",
                                    for diagnostic in diagnostics.iter().cloned() {
                                        li {
                                            span { class: "memory-scope",
                                                "{diagnostic.level} · {diagnostic.source}"
                                            }
                                            span { class: "memory-window", "{diagnostic.message}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}