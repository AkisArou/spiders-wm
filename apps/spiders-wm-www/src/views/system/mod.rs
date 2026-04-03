use leptos::prelude::*;

use crate::app_state::AppState;
use crate::components::{Panel, PanelBar};
use crate::workspace::{EDITOR_FILES, file_by_id};

fn active_file_path(app_state: AppState) -> String {
    app_state
        .active_file_id
        .get()
        .map(file_by_id)
        .map(|file| file.path.to_string())
        .unwrap_or_else(|| "no file open".to_string())
}

#[component]
pub fn SystemView() -> impl IntoView {
    let app_state = expect_context::<AppState>();
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

    view! {
        <section class="grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2 xl:grid-cols-2">
            <Panel>
                <PanelBar>
                    <div>"system://log"</div>
                </PanelBar>
                <div class="grid min-h-0 flex-1 gap-1 overflow-auto p-2 text-sm">
                    <Show
                        when=move || !app_state.session.get().event_log.is_empty()
                        fallback=move || view! { <div class="text-terminal-faint">"no events"</div> }
                    >
                        <>
                            {move || {
                                app_state
                                    .session
                                    .get()
                                    .event_log
                                    .into_iter()
                                    .map(|entry| {
                                        view! {
                                            <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-terminal-fg">
                                                {entry}
                                            </div>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </>
                    </Show>
                </div>
            </Panel>

            <Panel>
                <PanelBar>
                    <div>"system://state"</div>
                </PanelBar>
                <div class="grid gap-1 p-2">
                    {move || {
                        let snapshot = app_state.session.get();
                        let focused_window_label = snapshot
                            .focused_window_id()
                            .as_ref()
                            .map(|window_id| snapshot.window_name(window_id))
                            .unwrap_or_else(|| "none".to_string());
                        let rows = vec![
                            (
                                "layout".to_string(),
                                snapshot.active_layout.title().to_string(),
                            ),
                            ("focused".to_string(), focused_window_label),
                            ("workspace".to_string(), snapshot.active_workspace_name.clone()),
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
                                snapshot.remembered_rows().len().to_string(),
                            ),
                            ("active file".to_string(), active_file_path(app_state)),
                            ("last action".to_string(), snapshot.last_action.clone()),
                        ];

                        rows.into_iter()
                            .map(|(label, value)| {
                                view! {
                                    <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm">
                                        <div class="text-terminal-info text-xs">{label}</div>
                                        <div class="pt-1 text-terminal-fg">{value}</div>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                </div>
            </Panel>

            <Panel>
                <PanelBar>
                    <div>"system://runtime"</div>
                </PanelBar>
                <div class="grid gap-1 p-2">
                    {runtime_rows
                        .into_iter()
                        .map(|(label, status, detail)| {
                            view! {
                                <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm">
                                    <div class="text-terminal-info text-xs">{format!("{label} [{status}]")}</div>
                                    <div class="pt-1 text-terminal-fg">{detail}</div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </Panel>

            <Panel>
                <PanelBar>
                    <div>"system://bindings"</div>
                </PanelBar>
                <div class="grid min-h-0 flex-1 gap-1 overflow-auto p-2">
                    {move || {
                        app_state
                            .binding_entries()
                            .into_iter()
                            .map(|entry| {
                                view! {
                                    <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm">
                                        <div class="text-terminal-info text-xs">{entry.chord}</div>
                                        <div class="pt-1 text-terminal-fg">{entry.command_label}</div>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                </div>
            </Panel>

            <Panel>
                <PanelBar>
                    <div>"system://workspaces"</div>
                </PanelBar>
                <div class="grid gap-1 p-2">
                    {move || {
                        let snapshot = app_state.session.get();
                        snapshot
                            .workspace_names
                            .iter()
                            .cloned()
                            .map(|workspace_name| {
                                let layout_name = snapshot.layout_name_for_workspace(&workspace_name);
                                let window_count = snapshot
                                    .windows
                                    .iter()
                                    .filter(|window| window.workspace_name == workspace_name)
                                    .count();

                                view! {
                                    <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm">
                                        <div class="text-terminal-info text-xs">{workspace_name}</div>
                                        <div class="pt-1 text-terminal-fg">
                                            {format!("{window_count} windows · {layout_name}")}
                                        </div>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                </div>
            </Panel>

            <Panel>
                <PanelBar>
                    <div>"system://files"</div>
                </PanelBar>
                <div class="grid min-h-0 flex-1 gap-1 overflow-auto p-2">
                    {EDITOR_FILES
                        .iter()
                        .copied()
                        .map(|file| {
                            view! {
                                <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm">
                                    <div class="text-terminal-info text-xs">{file.label}</div>
                                    <div class="pt-1 text-terminal-fg">{file.path}</div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </Panel>

            <Panel class="xl:col-span-2">
                <PanelBar>
                    <div>"system://diagnostics"</div>
                </PanelBar>
                <div class="grid min-h-0 flex-1 gap-1 overflow-auto p-2">
                    <Show
                        when=move || !app_state.session.get().diagnostics.is_empty()
                        fallback=move || {
                            view! {
                                <div class="text-terminal-faint text-sm">"no diagnostics"</div>
                            }
                        }
                    >
                        <>
                            {move || {
                                app_state
                                    .session
                                    .get()
                                    .diagnostics
                                    .into_iter()
                                    .map(|diagnostic| {
                                        view! {
                                            <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-sm">
                                                <div class="text-terminal-info text-xs">
                                                    {format!("{} · {}", diagnostic.level, diagnostic.source)}
                                                </div>
                                                <div class="pt-1 text-terminal-fg">{diagnostic.message}</div>
                                            </div>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </>
                    </Show>
                </div>
            </Panel>
        </section>
    }
}
