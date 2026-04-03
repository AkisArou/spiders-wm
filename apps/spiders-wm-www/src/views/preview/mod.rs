use leptos::prelude::*;

use crate::app_state::AppState;
use crate::components::{Panel, PanelBar, TerminalSelect, TerminalSelectOption};
use crate::session::{PreviewLayoutId, PreviewSessionState, PreviewSessionWindow};
use spiders_core::navigation::NavigationDirection;

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
            <div class="grid min-h-0 gap-2">
                <Panel>
                    <PanelBar class="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-2">
                        <div class="flex min-w-0 items-center gap-1 overflow-x-auto">
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
                                                    if app_state.session.get().active_workspace_name == class_workspace {
                                                        format!("{TOGGLE_BUTTON_BASE} border-terminal-info bg-terminal-info/10 text-terminal-info")
                                                    } else {
                                                        format!("{TOGGLE_BUTTON_BASE} border-terminal-border bg-terminal-bg-subtle text-terminal-dim hover:text-terminal-fg")
                                                    }
                                                }
                                                on:click=move |_| {
                                                    app_state.session.update(|state| {
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

                        <div class="text-terminal-fg-strong min-w-0 truncate px-2 text-center">
                            {move || focused_window_label(&app_state.session.get())}
                        </div>

                        <div class="flex items-center gap-2 justify-self-end">
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
                                        app_state.session.update(|state| state.switch_layout(layout));
                                    }
                                })
                            />

                            <span>
                                {move || format!("{} windows", app_state.session.get().visible_window_count())}
                            </span>

                            <button
                                class=move || {
                                    if show_sidebar.get() {
                                        format!("{TOGGLE_BUTTON_BASE} border-terminal-info bg-terminal-info/10 text-terminal-info")
                                    } else {
                                        format!("{TOGGLE_BUTTON_BASE} border-terminal-border bg-terminal-bg-subtle text-terminal-dim hover:text-terminal-fg")
                                    }
                                }
                                on:click=move |_| show_sidebar.update(|value| *value = !*value)
                            >
                                {move || if show_sidebar.get() { "Hide info" } else { "Show info" }}
                            </button>
                        </div>
                    </PanelBar>

                    <div class="min-h-0 flex-1 overflow-hidden p-2">
                        <div
                            class="bg-terminal-bg-subtle relative h-full min-h-72 w-full overflow-hidden border border-terminal-border"
                            style="background-image: linear-gradient(color-mix(in srgb, var(--color-terminal-bg) 72%, transparent), color-mix(in srgb, var(--color-terminal-bg-subtle) 58%, transparent)), url('/spiders-wm-logo.png'); background-position: center, center; background-repeat: no-repeat, no-repeat; background-size: cover, min(34rem, 56%);"
                        >
                            {move || {
                                app_state
                                    .session
                                    .get()
                                    .claimed_visible_windows()
                                    .into_iter()
                                    .map(|window| {
                                        let style_window = window.clone();
                                        let focus_target = window.id.clone();
                                        let focused_id = window.id.clone();
                                        let badge = window.badge.clone();
                                        let subtitle = window.subtitle.clone();
                                        let title = window.display_title().to_string();
                                        let dimensions =
                                            format!("{} × {}", window.geometry.width, window.geometry.height);
                                        let accent_style = format!(
                                            "background: color-mix(in srgb, {} 86%, white);",
                                            window.accent
                                        );

                                        view! {
                                            <button
                                                class=move || {
                                                    if app_state
                                                        .session
                                                        .get()
                                                        .focused_window_id()
                                                        .as_ref()
                                                        == Some(&focused_id)
                                                    {
                                                        "text-terminal-fg absolute overflow-hidden border border-terminal-info bg-terminal-bg-active p-4 text-left text-xs shadow-[0_0_0_1px_color-mix(in_srgb,var(--color-terminal-info)_40%,transparent),0_18px_50px_rgba(0,0,0,0.36)]"
                                                    } else {
                                                        "text-terminal-fg absolute overflow-hidden border border-terminal-border-strong bg-terminal-bg-panel p-4 text-left text-xs shadow-[0_14px_36px_rgba(0,0,0,0.24)]"
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
                                                <div class="mb-4 flex items-center justify-between gap-3">
                                                    <span
                                                        class="inline-grid h-9 w-9 place-items-center rounded-full text-[0.9rem] font-semibold text-black"
                                                        style=accent_style
                                                    >
                                                        {badge}
                                                    </span>
                                                    <span class="text-[0.7rem] uppercase tracking-[0.16em] text-[color-mix(in_srgb,var(--accent)_72%,white)]">
                                                        {subtitle}
                                                    </span>
                                                </div>
                                                <h3 class="mb-1 text-base font-semibold tracking-[-0.02em]">
                                                    {title}
                                                </h3>
                                                <p class="text-terminal-muted text-sm">{dimensions}</p>
                                            </button>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                    </div>
                </Panel>

                <Panel class="border-terminal-border/80">
                    <PanelBar>
                        <div>"preview://controls"</div>
                        <div class="text-terminal-muted">
                            {move || app_state.session.get().summary().to_string()}
                        </div>
                    </PanelBar>

                    <div class="grid gap-3 p-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
                        <div class="grid gap-1">
                            <p class="text-terminal-fg-strong text-sm font-medium">
                                {move || app_state.session.get().display_title().to_string()}
                            </p>
                            <p class="text-terminal-muted text-sm">
                                {move || app_state.session.get().prompt().to_string()}
                            </p>
                        </div>

                        <div class="grid min-w-[16rem] grid-cols-3 gap-2">
                            <button
                                class="border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong"
                                on:click=move |_| {
                                    app_state.session.update(|state| state.navigate(NavigationDirection::Up));
                                }
                            >
                                "Up"
                            </button>
                            <button
                                class="border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong"
                                on:click=move |_| {
                                    app_state.session.update(|state| state.navigate(NavigationDirection::Left));
                                }
                            >
                                "Left"
                            </button>
                            <button
                                class="border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong"
                                on:click=move |_| {
                                    app_state.session.update(|state| state.navigate(NavigationDirection::Right));
                                }
                            >
                                "Right"
                            </button>
                            <button
                                class="border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong"
                                on:click=move |_| {
                                    app_state.session.update(|state| state.navigate(NavigationDirection::Down));
                                }
                            >
                                "Down"
                            </button>
                            <button
                                class="col-span-2 border border-terminal-info bg-terminal-info/10 px-3 py-2 text-terminal-info hover:bg-terminal-info/15"
                                on:click=move |_| app_state.session.update(PreviewSessionState::reset)
                            >
                                "Reset"
                            </button>
                        </div>
                    </div>
                </Panel>
            </div>

            <Show when=move || show_sidebar.get()>
                <div class="grid min-h-0 gap-2 xl:grid-rows-[auto_auto_auto_minmax(10rem,0.8fr)_minmax(12rem,1fr)]">
                    <InspectorPanel title="session://windows">
                        <WindowList
                            windows=Signal::derive(move || app_state.session.get().visible_windows())
                            empty_label="no windows"
                        />
                    </InspectorPanel>

                    <InspectorPanel title="session://unclaimed">
                        <WindowList
                            windows=Signal::derive(move || app_state.session.get().unclaimed_visible_windows())
                            empty_label="all claimed"
                        />
                    </InspectorPanel>

                    <InspectorPanel title="session://focus">
                        <div class="grid gap-2 p-2 text-sm">
                            <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                                <span class="text-terminal-faint block text-xs uppercase tracking-[0.12em]">
                                    "focused"
                                </span>
                                <span class="text-terminal-fg-strong block pt-1">
                                    {move || focused_window_label(&app_state.session.get())}
                                </span>
                            </div>

                            <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                                <span class="text-terminal-faint block text-xs uppercase tracking-[0.12em]">
                                    "scope path"
                                </span>

                                <Show
                                    when=move || !app_state.session.get().current_scope_path().is_empty()
                                    fallback=move || {
                                        view! {
                                            <span class="text-terminal-muted block pt-1">"none"</span>
                                        }
                                    }
                                >
                                    <div class="flex flex-wrap gap-1 pt-1">
                                        {move || {
                                            app_state
                                                .session
                                                .get()
                                                .current_scope_path()
                                                .into_iter()
                                                .map(|scope| {
                                                    view! {
                                                        <span class="border border-terminal-border bg-terminal-bg-hover px-2 py-0.5 text-xs text-terminal-fg-strong">
                                                            {scope}
                                                        </span>
                                                    }
                                                })
                                                .collect_view()
                                        }}
                                    </div>
                                </Show>
                            </div>
                        </div>
                    </InspectorPanel>

                    <InspectorPanel title="scene://diagnostics">
                        <Show
                            when=move || !app_state.session.get().diagnostics.is_empty()
                            fallback=move || {
                                view! {
                                    <div class="p-2 text-sm text-terminal-faint">"no diagnostics"</div>
                                }
                            }
                        >
                            <div class="grid gap-1 p-2 text-sm">
                                {move || {
                                    app_state
                                        .session
                                        .get()
                                        .diagnostics
                                        .into_iter()
                                        .map(|diagnostic| {
                                            view! {
                                                <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                                                    <div class="flex items-center gap-2 text-xs">
                                                        <span class="text-terminal-warn">{diagnostic.level}</span>
                                                        <span class="text-terminal-dim">{diagnostic.source}</span>
                                                    </div>
                                                    <div class="pt-1 text-terminal-fg">{diagnostic.message}</div>
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </div>
                        </Show>
                    </InspectorPanel>

                    <InspectorPanel title="session://log">
                        <div class="grid gap-1 p-2 text-sm">
                            <Show
                                when=move || !app_state.session.get().event_log.is_empty()
                                fallback=move || {
                                    view! { <div class="text-terminal-faint">"no events"</div> }
                                }
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

                            <Show when=move || !app_state.session.get().remembered_rows().is_empty()>
                                <div class="pt-2 text-xs uppercase tracking-[0.12em] text-terminal-faint">
                                    "remembered scopes"
                                </div>
                                {move || {
                                    app_state
                                        .session
                                        .get()
                                        .remembered_rows()
                                        .into_iter()
                                        .map(|(scope, window_name)| {
                                            view! {
                                                <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                                                    <div class="text-terminal-info text-xs">{scope}</div>
                                                    <div class="pt-1 text-terminal-fg text-sm">{window_name}</div>
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </Show>
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
            <div class="min-h-0 flex-1 overflow-auto">{children()}</div>
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
                            let label = format!("{} · {}", window.badge, window.subtitle);
                            let title = window.display_title().to_string();
                            let geometry =
                                format!("{} × {}", window.geometry.width, window.geometry.height);

                            view! {
                                <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                                    <div class="text-terminal-info text-xs">{label}</div>
                                    <div class="pt-1 text-terminal-fg">{title}</div>
                                    <div class="pt-1 text-terminal-muted text-xs">{geometry}</div>
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
        </Show>
    }
}
