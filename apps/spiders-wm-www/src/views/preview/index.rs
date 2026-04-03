use leptos::prelude::*;

use crate::app_state::AppState;
use crate::components::{Panel, PanelBar, TerminalSelect, TerminalSelectOption};
use crate::session::{
    PreviewDiagnostic, PreviewLayoutId, PreviewSessionState, PreviewSessionWindow,
    PreviewSnapshotNode,
};

fn pane_style(window: &PreviewSessionWindow, canvas_width: i32, canvas_height: i32) -> String {
    let left = window.geometry.x as f32 / canvas_width as f32 * 100.0;
    let top = window.geometry.y as f32 / canvas_height as f32 * 100.0;
    let width = window.geometry.width as f32 / canvas_width as f32 * 100.0;
    let height = window.geometry.height as f32 / canvas_height as f32 * 100.0;

    format!(
        "left: {left:.3}%; top: {top:.3}%; width: calc({width:.3}% - 0.4rem); height: calc({height:.3}% - 0.4rem); --accent: {};",
        window.accent,
    )
}

fn focused_window_label(snapshot: &PreviewSessionState) -> String {
    snapshot
        .focused_window_id()
        .as_ref()
        .map(|window_id| snapshot.window_name(window_id))
        .unwrap_or_else(|| "none".to_string())
}

const TOGGLE_BUTTON_BASE: &str = "border px-2 py-0.5 transition-colors duration-150";

fn descendant_window_ids(node: &PreviewSnapshotNode, ids: &mut Vec<String>) {
    if let Some(window_id) = node.window_id.as_ref() {
        ids.push(window_id.as_str().to_string());
    }

    for child in &node.children {
        descendant_window_ids(child, ids);
    }
}

fn descendant_window_count(node: &PreviewSnapshotNode) -> usize {
    let mut ids = Vec::new();
    descendant_window_ids(node, &mut ids);
    ids.len()
}

fn descendant_window_titles(
    node: &PreviewSnapshotNode,
    windows: &[PreviewSessionWindow],
) -> String {
    let mut ids = Vec::new();
    descendant_window_ids(node, &mut ids);

    ids.into_iter()
        .map(|window_id| {
            windows
                .iter()
                .find(|window| window.id.as_str() == window_id)
                .map(|window| window.display_title().to_string())
                .unwrap_or(window_id)
        })
        .collect::<Vec<_>>()
        .join("  |  ")
}

#[component]
pub fn PreviewView() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let show_sidebar = RwSignal::new(false);
    let layout_options = PreviewLayoutId::ALL
        .iter()
        .map(|layout| TerminalSelectOption {
            value: layout.title().to_string(),
            label: layout.display_title().to_string(),
        })
        .collect::<Vec<_>>();
    let layout_value =
        Signal::derive(move || app_state.session.get().active_layout.title().to_string());

    view! {
        <section class=move || {
            if show_sidebar.get() {
                "grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2 xl:grid-cols-[minmax(0,1.55fr)_22rem]"
            } else {
                "grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2"
            }
        }>
            <div class="grid gap-2 min-h-0">
                <Panel>
                    <PanelBar class="grid gap-2 grid-cols-[auto_minmax(0,1fr)_auto]">
                        <div class="flex overflow-x-auto gap-1 items-center min-w-0">
                            {move || {
                                let snapshot = app_state.session.get();
                                snapshot
                                    .workspace_names
                                    .iter()
                                    .cloned()
                                    .map(|workspace_name| {
                                        let target_workspace = workspace_name.clone();
                                        let class_workspace = workspace_name.clone();

                                        view! {
                                            <button
                                                class=move || {
                                                    if app_state.session.get().active_workspace_name
                                                        == class_workspace
                                                    {
                                                        format!(
                                                            "{TOGGLE_BUTTON_BASE} border-terminal-info bg-terminal-info/10 text-terminal-info",
                                                        )
                                                    } else {
                                                        format!(
                                                            "{TOGGLE_BUTTON_BASE} border-terminal-border bg-terminal-bg-subtle text-terminal-dim hover:text-terminal-fg",
                                                        )
                                                    }
                                                }
                                                on:click=move |_| {
                                                    app_state
                                                        .session
                                                        .update(|state| {
                                                            state.select_workspace(target_workspace.clone())
                                                        });
                                                }
                                            >
                                                {workspace_name}
                                            </button>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>

                        <div class="px-2 min-w-0 text-center text-terminal-fg-strong truncate">
                            {move || focused_window_label(&app_state.session.get())}
                        </div>

                        <div class="flex gap-2 justify-self-end items-center">
                            <TerminalSelect
                                value=layout_value
                                aria_label="Select preview layout"
                                options=layout_options
                                onchange=Callback::new(move |layout_name| {
                                    if let Some(layout) = PreviewLayoutId::ALL
                                        .iter()
                                        .copied()
                                        .find(|candidate| candidate.title() == layout_name)
                                    {
                                        app_state
                                            .session
                                            .update(|state| state.switch_layout(layout));
                                    }
                                })
                            />

                            <span>
                                {move || {
                                    format!(
                                        "{} windows",
                                        app_state.session.get().visible_window_count(),
                                    )
                                }}
                            </span>

                            <button
                                class=move || {
                                    if show_sidebar.get() {
                                        format!(
                                            "{TOGGLE_BUTTON_BASE} border-terminal-info bg-terminal-info/10 text-terminal-info",
                                        )
                                    } else {
                                        format!(
                                            "{TOGGLE_BUTTON_BASE} border-terminal-border bg-terminal-bg-subtle text-terminal-dim hover:text-terminal-fg",
                                        )
                                    }
                                }
                                on:click=move |_| show_sidebar.update(|value| *value = !*value)
                            >
                                {move || if show_sidebar.get() { "Hide info" } else { "Show info" }}
                            </button>
                        </div>
                    </PanelBar>

                    <div class="overflow-hidden flex-1 min-h-0">
                        <Show
                            when=move || app_state.session.get().snapshot_root.is_some()
                            fallback=move || {
                                view! {
                                    <div class="flex justify-center items-center p-3 h-full text-sm text-terminal-faint min-h-72">
                                        "loading wasm preview..."
                                    </div>
                                }
                            }
                        >
                            <div
                                class="overflow-hidden relative w-full h-full bg-terminal-bg-subtle min-h-72"
                                style="background-image: linear-gradient(color-mix(in srgb, var(--color-terminal-bg) 72%, transparent), color-mix(in srgb, var(--color-terminal-bg-subtle) 58%, transparent));"
                            >
                                <div class="flex absolute inset-0 z-30 justify-center items-center pointer-events-none">
                                    <img
                                        class="h-auto w-[min(26rem,52%)] opacity-[0.14]"
                                        src="/spiders-wm-logo.png"
                                        alt=""
                                    />
                                </div>

                                {move || {
                                    app_state
                                        .session
                                        .get()
                                        .claimed_visible_windows()
                                        .into_iter()
                                        .map(|window| {
                                            let style_window = window.clone();
                                            let surface_window = window.clone();
                                            let focus_target = window.id.clone();
                                            let focused_id = window.id.clone();
                                            let title = window.display_title().to_string();
                                            let dimensions = format!(
                                                "{}x{}",
                                                window.geometry.width,
                                                window.geometry.height,
                                            );
                                            let is_foot = window.app_id.as_deref() == Some("foot");

                                            view! {
                                                <button
                                                    class=move || {
                                                        if app_state.session.get().focused_window_id().as_ref()
                                                            == Some(&focused_id)
                                                        {
                                                            "text-terminal-fg absolute z-20 overflow-hidden border border-terminal-info bg-terminal-bg-active text-left text-xs"
                                                        } else {
                                                            "text-terminal-fg absolute z-20 overflow-hidden border border-terminal-border-strong bg-terminal-bg-panel text-left text-xs"
                                                        }
                                                    }
                                                    style=move || {
                                                        let snapshot = app_state.session.get();
                                                        pane_style(
                                                            &style_window,
                                                            snapshot.canvas_width(),
                                                            snapshot.canvas_height(),
                                                        )
                                                    }
                                                    on:click=move |_| {
                                                        app_state
                                                            .session
                                                            .update(|state| state.set_focus(focus_target.clone()));
                                                    }
                                                >
                                                    <div class="flex justify-between items-center py-0.5 px-1 text-xs border-b bg-terminal-bg-subtle/80 text-terminal-dim border-current/20">
                                                        <span class="truncate">{title}</span>
                                                        <span>{dimensions}</span>
                                                    </div>

                                                    {if is_foot {
                                                        view! {
                                                            <FootTerminal focused=Signal::derive(move || {
                                                                app_state.session.get().focused_window_id().as_ref()
                                                                    == Some(&window.id)
                                                            }) />
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! { <WindowSurface window=surface_window /> }.into_any()
                                                    }}
                                                </button>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </div>
                        </Show>
                    </div>
                </Panel>
            </div>

            <Show when=move || show_sidebar.get()>
                <div class="grid gap-2 min-h-0 xl:grid-rows-[auto_auto_auto_minmax(10rem,0.8fr)_minmax(12rem,1fr)]">
                    <InspectorPanel title="session://windows">
                        <WindowList
                            windows=Signal::derive(move || {
                                app_state.session.get().visible_windows()
                            })
                            empty_label="no windows"
                        />
                    </InspectorPanel>

                    <InspectorPanel title="session://unclaimed">
                        <WindowList
                            windows=Signal::derive(move || {
                                app_state.session.get().unclaimed_visible_windows()
                            })
                            empty_label="all claimed"
                        />
                    </InspectorPanel>

                    <InspectorPanel title="layout://order">
                        <div class="grid gap-3 p-2 text-sm">
                            <WindowOrderSummary
                                label="input"
                                windows=Signal::derive(move || {
                                    app_state.session.get().visible_windows()
                                })
                            />
                            <WindowOrderSummary
                                label="claimed"
                                windows=Signal::derive(move || {
                                    app_state.session.get().claimed_visible_windows()
                                })
                            />
                        </div>
                    </InspectorPanel>

                    <InspectorPanel title="scene://diagnostics">
                        <div class="overflow-auto p-2 min-h-0 text-sm">
                            <DiagnosticsList diagnostics=Signal::derive(move || {
                                app_state.session.get().diagnostics.clone()
                            }) />
                        </div>
                    </InspectorPanel>

                    <InspectorPanel title="scene://tree">
                        <div class="overflow-auto p-2 min-h-0">
                            {move || {
                                let snapshot = app_state.session.get();
                                if let Some(root) = snapshot.snapshot_root.clone() {
                                    view! {
                                        <LayoutTreeNode
                                            node=root
                                            windows=snapshot.visible_windows()
                                        />
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="text-sm text-terminal-faint">
                                            "no resolved tree"
                                        </div>
                                    }
                                        .into_any()
                                }
                            }}
                        </div>
                    </InspectorPanel>
                </div>
            </Show>
        </section>
    }
}

#[component]
fn InspectorPanel(#[prop(into)] title: Oco<'static, str>, children: Children) -> impl IntoView {
    view! {
        <Panel>
            <PanelBar>
                <div>{title}</div>
            </PanelBar>
            <div class="overflow-auto flex-1 min-h-0">{children()}</div>
        </Panel>
    }
}

#[component]
fn WindowList(
    #[prop(into)] windows: Signal<Vec<PreviewSessionWindow>>,
    #[prop(into)] empty_label: Oco<'static, str>,
) -> impl IntoView {
    let fallback_label = empty_label.clone();

    view! {
        <Show
            when=move || !windows.get().is_empty()
            fallback=move || {
                view! {
                    <div class="p-2 text-sm text-terminal-faint">{fallback_label.clone()}</div>
                }
            }
        >
            <div class="grid gap-1 p-2 text-sm">
                {move || {
                    windows
                        .get()
                        .into_iter()
                        .map(|window| {
                            let title = window.display_title().to_string();
                            let app_id = window.app_id.unwrap_or_else(|| "unknown".to_string());

                            view! {
                                <div class=if window.focused {
                                    "flex items-center gap-2 border border-terminal-info bg-terminal-bg-active px-2 py-1"
                                } else {
                                    "flex items-center gap-2 border border-terminal-border bg-terminal-bg-panel px-2 py-1"
                                }>
                                    <span class="text-terminal-fg-strong">{title}</span>
                                    <Show when=move || window.floating>
                                        <span class="text-terminal-warn">"float"</span>
                                    </Show>
                                    <span class="ml-auto text-terminal-faint">{app_id}</span>
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
        </Show>
    }
}

#[component]
fn WindowSurface(window: PreviewSessionWindow) -> impl IntoView {
    view! {
        <div class="flex flex-col p-2 text-sm text-terminal-muted h-[calc(100%-1.5rem)]">
            <div>
                <div class="text-terminal-fg-strong">
                    {window.app_id.unwrap_or_else(|| "window".to_string())}
                </div>
                <div class="mt-1 text-terminal-dim">
                    {window.title.unwrap_or_else(|| "unbound node".to_string())}
                </div>
            </div>
        </div>
    }
}

#[component]
fn FootTerminal(focused: Signal<bool>) -> impl IntoView {
    view! {
        <div class="flex items-start py-2 px-2 text-sm bg-terminal-bg text-terminal-fg h-[calc(100%-1.5rem)]">
            <div>
                <span class=move || {
                    if focused.get() { "text-terminal-info" } else { "text-terminal-faint" }
                }>"akisarou@spiders"</span>
                <span class="text-terminal-dim">":$ "</span>
                <Show when=move || focused.get()>
                    <span class="inline-block w-2 h-4 foot-cursor bg-terminal-fg-strong align-[-0.125rem]" />
                </Show>
            </div>
        </div>
    }
}

#[component]
fn WindowOrderSummary(
    #[prop(into)] label: Oco<'static, str>,
    #[prop(into)] windows: Signal<Vec<PreviewSessionWindow>>,
) -> impl IntoView {
    view! {
        <div class="grid gap-1">
            <div class="text-xs text-terminal-dim">{label}</div>
            <div class="text-sm text-terminal-muted">
                {move || {
                    let rows = windows.get();
                    if rows.is_empty() {
                        "none".to_string()
                    } else {
                        rows.into_iter()
                            .enumerate()
                            .map(|(index, window)| {
                                format!("{}:{}", index + 1, window.display_title())
                            })
                            .collect::<Vec<_>>()
                            .join("  ->  ")
                    }
                }}
            </div>
        </div>
    }
}

#[component]
fn DiagnosticsList(#[prop(into)] diagnostics: Signal<Vec<PreviewDiagnostic>>) -> impl IntoView {
    view! {
        <Show
            when=move || !diagnostics.get().is_empty()
            fallback=move || view! { <div class="text-terminal-faint">"no diagnostics"</div> }
        >
            <div class="grid gap-1">
                {move || {
                    diagnostics
                        .get()
                        .into_iter()
                        .map(|diagnostic| {
                            let level_class = if diagnostic.level == "error" {
                                "text-terminal-error"
                            } else {
                                "text-terminal-warn"
                            };

                            view! {
                                <div class="py-1 px-2 border border-terminal-border bg-terminal-bg-panel text-terminal-muted">
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
    }
}

#[component]
fn LayoutTreeNode(
    node: PreviewSnapshotNode,
    windows: Vec<PreviewSessionWindow>,
    #[prop(optional)] depth: usize,
) -> AnyView {
    let label = node.id.clone().unwrap_or_else(|| "_".to_string());
    let rect_label = node
        .rect
        .as_ref()
        .map(|rect| format!("{}x{}", rect.width.round() as i32, rect.height.round() as i32));
    let descendant_titles = descendant_window_titles(&node, &windows);
    let descendant_count = descendant_window_count(&node);
    let children = node.children.clone();
    let node_type = node.node_type.clone();

    view! {
        <div class="text-sm leading-5 text-terminal-muted">
            <div
                class="flex gap-2 items-center py-1 px-2 border border-terminal-border bg-terminal-bg-panel"
                style=format!("margin-left: {}px", depth * 12)
            >
                <span class="text-terminal-dim">{node_type}</span>
                <span class="text-terminal-fg-strong">{label}</span>
                {rect_label
                    .map(|rect| {
                        view! { <span class="text-terminal-faint">{rect}</span> }
                    })}
                <span class="ml-auto text-terminal-faint">{descendant_count}</span>
            </div>

            <div class="grid gap-1 mt-1">
                {(!descendant_titles.is_empty())
                    .then(|| {
                        view! {
                            <div
                                class="text-xs text-terminal-faint"
                                style=format!("margin-left: {}px", depth * 12 + 12)
                            >
                                {descendant_titles}
                            </div>
                        }
                    })}
                {children
                    .into_iter()
                    .map(|child| {
                        view! {
                            <LayoutTreeNode node=child windows=windows.clone() depth=depth + 1 />
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
        .into_any()
}
