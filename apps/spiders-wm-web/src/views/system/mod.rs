use dioxus::prelude::*;

use crate::bindings::ParsedBindingEntry;
use crate::components::{Panel, PanelBar};
use crate::session::PreviewSessionState;
use crate::workspace::{file_by_id, EditorFileId, EDITOR_FILES};

#[component]
pub fn SystemView(
    session: Signal<PreviewSessionState>,
    active_file_id: Signal<Option<EditorFileId>>,
    binding_entries: Vec<ParsedBindingEntry>,
) -> Element {
    let snapshot = session();
    let focused_window_label = snapshot
        .focused_window_id()
        .as_ref()
        .map(|window_id| snapshot.window_name(window_id))
        .unwrap_or_else(|| "none".to_string());
    let remembered_rows = snapshot.remembered_rows();
    let workspace_names = snapshot.workspace_names.clone();
    let active_workspace_name = snapshot.active_workspace_name.clone();
    let event_log = snapshot.event_log.clone();
    let diagnostics = snapshot.diagnostics.clone();
    let active_file_path = active_file_id()
        .map(file_by_id)
        .map(|file| file.path.to_string())
        .unwrap_or_else(|| "no file open".to_string());

    let state_rows = vec![
        (
            "layout".to_string(),
            snapshot.active_layout.title().to_string(),
        ),
        ("focused".to_string(), focused_window_label),
        ("workspace".to_string(), active_workspace_name),
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
        ("active file".to_string(), active_file_path),
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

    let workspace_state_rows = workspace_names.iter().cloned().map(|workspace_name| {
        let layout_name = snapshot.layout_name_for_workspace(&workspace_name);
        let window_count = snapshot
            .windows
            .iter()
            .filter(|window| window.workspace_name == workspace_name)
            .count();

        rsx! {
            div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm",
                div { class: "text-terminal-info text-xs", "{workspace_name}" }
                div { class: "pt-1 text-terminal-fg", "{window_count} windows · {layout_name}" }
            }
        }
    });

    rsx! {
        section { class: "grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2 xl:grid-cols-2",
            Panel {
                PanelBar {
                    div { "system://log" }
                }
                div { class: "grid min-h-0 flex-1 gap-1 overflow-auto p-2 text-sm",
                    if event_log.is_empty() {
                        div { class: "text-terminal-faint", "no events" }
                    } else {
                        for entry in event_log.iter().cloned() {
                            div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-terminal-fg",
                                "{entry}"
                            }
                        }
                    }
                }
            }

            Panel {
                PanelBar {
                    div { "system://state" }
                }
                div { class: "grid gap-1 p-2",
                    for (label , value) in state_rows.iter().cloned() {
                        div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm",
                            div { class: "text-terminal-info text-xs", "{label}" }
                            div { class: "pt-1 text-terminal-fg", "{value}" }
                        }
                    }
                }
            }

            Panel {
                PanelBar {
                    div { "system://runtime" }
                }
                div { class: "grid gap-1 p-2",
                    for (label , status , detail) in runtime_rows.iter().cloned() {
                        div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm",
                            div { class: "text-terminal-info text-xs", "{label} [{status}]" }
                            div { class: "pt-1 text-terminal-fg", "{detail}" }
                        }
                    }
                }
            }

            Panel {
                PanelBar {
                    div { "system://bindings" }
                }
                div { class: "grid min-h-0 flex-1 gap-1 overflow-auto p-2",
                    for entry in binding_entries.iter().cloned() {
                        div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm",
                            div { class: "text-terminal-info text-xs", "{entry.chord}" }
                            div { class: "pt-1 text-terminal-fg", "{entry.command_label}" }
                        }
                    }
                }
            }

            Panel {
                PanelBar {
                    div { "system://workspaces" }
                }
                div { class: "grid gap-1 p-2", {workspace_state_rows} }
            }

            Panel {
                PanelBar {
                    div { "system://files" }
                }
                div { class: "grid min-h-0 flex-1 gap-1 overflow-auto p-2",
                    for file in EDITOR_FILES {
                        div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm",
                            div { class: "text-terminal-info text-xs", "{file.label}" }
                            div { class: "pt-1 text-terminal-fg", "{file.path}" }
                        }
                    }
                }
            }

            Panel { class: Some("xl:col-span-2".to_string()),
                PanelBar {
                    div { "system://diagnostics" }
                }
                div { class: "grid min-h-0 flex-1 gap-1 overflow-auto p-2",
                    if diagnostics.is_empty() {
                        div { class: "text-terminal-faint text-sm", "no diagnostics" }
                    } else {
                        for diagnostic in diagnostics.iter().cloned() {
                            div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm",
                                div { class: "text-terminal-info text-xs",
                                    "{diagnostic.level} · {diagnostic.source}"
                                }
                                div { class: "pt-1 text-terminal-fg", "{diagnostic.message}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
