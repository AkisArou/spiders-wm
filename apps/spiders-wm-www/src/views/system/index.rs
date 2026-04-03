use std::collections::BTreeMap;

use crate::app_state::AppState;
use crate::bindings::ParsedBindingEntry;
use crate::bindings::format_binding_token;
use crate::components::{Panel, PanelBar};
use crate::editor_files::{EditorFileId, file_by_id, initial_content};
use leptos::prelude::*;
fn active_file_path(app_state: AppState) -> String {
    app_state
        .active_file_id
        .get()
        .map(file_by_id)
        .map(|file| file.path.to_string())
        .unwrap_or_else(|| "no file open".to_string())
}
fn dirty_file_count(buffers: &BTreeMap<EditorFileId, String>) -> usize {
    buffers.iter().filter(|(file_id, value)| value.as_str() != initial_content(**file_id)).count()
}

#[component]
pub fn SystemView() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    view! {
        <section class="grid grid-cols-1 gap-2 w-full min-w-0 h-full min-h-0 xl:grid-cols-[minmax(0,1.4fr)_20rem]">
            <Panel>
                <PanelBar>
                    <div>"system://log"</div>
                </PanelBar>
                <div class="overflow-auto flex-1 p-2 min-h-0 text-sm leading-5 text-terminal-muted">
                    <div class="grid gap-1">
                        {move || {
                            let snapshot = app_state.session.get();
                            let buffers = app_state.editor_buffers.get();
                            let dirty_count = dirty_file_count(&buffers);
                            let preview_level = if snapshot.snapshot_root.is_some() {
                                "info"
                            } else if snapshot.diagnostics.is_empty() {
                                "wait"
                            } else {
                                "error"
                            };
                            let log_lines = vec![
                                (
                                    preview_level.to_string(),
                                    "bindings".to_string(),
                                    if snapshot.snapshot_root.is_some() {
                                        "spiders-wm-runtime returned a preview tree".to_string()
                                    } else if snapshot.diagnostics.is_empty() {
                                        "waiting for wasm bindings".to_string()
                                    } else {
                                        snapshot
                                            .diagnostics
                                            .first()
                                            .map(|diagnostic| diagnostic.message.clone())
                                            .unwrap_or_else(|| "preview runtime degraded".to_string())
                                    },
                                ),
                                (
                                    if dirty_count > 0 { "warn" } else { "info" }.to_string(),
                                    "editor".to_string(),
                                    if dirty_count > 0 {
                                        format!("{dirty_count} modified buffer(s) not persisted")
                                    } else {
                                        "buffer contents match imported fixtures".to_string()
                                    },
                                ),
                                (
                                    "info".to_string(),
                                    "editor".to_string(),
                                    format!("active buffer {}", active_file_path(app_state)),
                                ),
                                (
                                    if snapshot.diagnostics.is_empty() { "info" } else { "warn" }
                                        .to_string(),
                                    "scene".to_string(),
                                    format!("{} diagnostic(s) reported", snapshot.diagnostics.len()),
                                ),
                            ];
                            log_lines
                                .into_iter()
                                .map(|(level, scope, message)| {
                                    let level_class = match level.as_str() {
                                        "error" => "text-terminal-error",
                                        "warn" => "text-terminal-warn",
                                        "wait" => "text-terminal-wait",
                                        _ => "text-terminal-info",
                                    };

                                    view! {
                                        <div class="flex gap-3 py-1 px-2 border border-terminal-border bg-terminal-bg-panel">
                                            <span class=move || {
                                                format!("w-12 shrink-0 {level_class}")
                                            }>{level}</span>
                                            <span class="w-16 text-terminal-dim shrink-0">{scope}</span>
                                            <span>{message}</span>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </div>
            </Panel>

            <div class="grid gap-2 min-h-0 xl:grid-rows-[auto_minmax(14rem,1fr)_minmax(0,1fr)]">
                <Panel>
                    <PanelBar>
                        <div>"system://state"</div>
                    </PanelBar>
                    <div class="grid gap-1 p-2 text-sm text-terminal-muted">
                        {move || {
                            let snapshot = app_state.session.get();
                            let focused_window = snapshot
                                .focused_window_id()
                                .as_ref()
                                .map(|window_id| snapshot.window_name(window_id))
                                .unwrap_or_else(|| "none".to_string());
                            let buffers = app_state.editor_buffers.get();
                            let dirty_count = dirty_file_count(&buffers);
                            let preview_state = if snapshot.snapshot_root.is_some() {
                                "ready"
                            } else if snapshot.diagnostics.is_empty() {
                                "booting"
                            } else {
                                "degraded"
                            };
                            vec![
                                (
                                    "workspace".to_string(),
                                    snapshot.active_workspace_name().to_string(),
                                ),
                                ("layout".to_string(), snapshot.active_layout.as_str().to_string()),
                                ("focused".to_string(), focused_window),
                                ("dirty".to_string(), dirty_count.to_string()),
                                ("preview".to_string(), preview_state.to_string()),
                                ("active file".to_string(), active_file_path(app_state)),
                            ]
                                .into_iter()
                                .map(|(label, value)| {

                                    view! {
                                        <div class="flex gap-3 justify-between py-1 px-2 border border-terminal-border bg-terminal-bg-panel">
                                            <span>{label}</span>
                                            <span class="text-right text-terminal-fg-strong truncate">
                                                {value}
                                            </span>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </Panel>

                <Panel>
                    <PanelBar>
                        <div>"system://bindings"</div>
                    </PanelBar>
                    <div class="overflow-auto p-2 min-h-0 text-sm text-terminal-muted">
                        {move || {
                            let binding_state = app_state.parsed_bindings();
                            let mod_key = format_binding_token("mod", &binding_state.mod_key);
                            let has_entries = !binding_state.entries.is_empty();

                            view! {
                                <>
                                    <div class="flex justify-between items-center py-1 px-2 mb-2 border border-terminal-border bg-terminal-bg-panel">
                                        <span>"mod"</span>
                                        <span class="text-terminal-fg-strong">{mod_key}</span>
                                    </div>
                                    {if has_entries {
                                        view! {
                                            <BindingEntries entries=binding_state.entries.clone() />
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <div class="text-terminal-faint">"no bindings parsed"</div>
                                        }
                                            .into_any()
                                    }}
                                </>
                            }
                        }}
                    </div>
                </Panel>

                <Panel>
                    <PanelBar>
                        <div>"system://diagnostics"</div>
                    </PanelBar>
                    <div class="overflow-auto p-2 min-h-0 text-sm text-terminal-muted">
                        <Show
                            when=move || !app_state.session.get().diagnostics.is_empty()
                            fallback=move || {
                                view! { <div class="text-terminal-faint">"no diagnostics"</div> }
                            }
                        >
                            <div class="grid gap-1">
                                {move || {
                                    app_state
                                        .session
                                        .get()
                                        .diagnostics
                                        .into_iter()
                                        .map(|diagnostic| {
                                            let level_class = if diagnostic.level == "error" {
                                                "text-terminal-error"
                                            } else {
                                                "text-terminal-warn"
                                            };

                                            view! {
                                                <div class="py-1 px-2 border border-terminal-border bg-terminal-bg-panel">
                                                    <div class="flex gap-2 items-center text-xs">
                                                        <span class=level_class>{diagnostic.level}</span>
                                                        <span class="text-terminal-dim">{diagnostic.source}</span>
                                                    </div>
                                                    <div class="mt-1">{diagnostic.message}</div>
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </div>
                        </Show>
                    </div>
                </Panel>
            </div>
        </section>
    }
}

#[component]
fn BindingEntries(entries: Vec<ParsedBindingEntry>) -> impl IntoView {
    view! {
        <div class="grid gap-1">
            {entries
                .into_iter()
                .map(|entry| {
                    view! {
                        <div class="grid gap-1 py-1 px-2 border border-terminal-border bg-terminal-bg-panel">
                            <div class="text-terminal-fg-strong">{entry.chord}</div>
                            <div class="text-terminal-dim">{entry.command_label}</div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}
