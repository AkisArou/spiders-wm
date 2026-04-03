use dioxus::prelude::*;

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

#[component]
pub fn PreviewView(session: Signal<PreviewSessionState>) -> Element {
    let mut show_sidebar = use_signal(|| false);
    let snapshot = session();
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
    let layout_options = PreviewLayoutId::ALL
        .iter()
        .map(|layout| TerminalSelectOption {
            value: layout.title().to_string(),
            label: layout.display_title().to_string(),
        })
        .collect::<Vec<_>>();
    let workspace_buttons = workspace_names.iter().cloned().map(|workspace_name| {
        let target_workspace = workspace_name.clone();

        rsx! {
            button {
                class: if workspace_name == active_workspace_name { "border border-terminal-info bg-terminal-info/10 px-2 py-0.5 text-terminal-info" } else { "border border-terminal-border bg-terminal-bg-subtle px-2 py-0.5 text-terminal-dim hover:text-terminal-fg" },
                onclick: move |_| session.with_mut(|state| state.select_workspace(target_workspace.clone())),
                "{workspace_name}"
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
                class: if is_focused { "text-terminal-fg absolute overflow-hidden border border-terminal-info bg-terminal-bg-active p-4 text-left text-xs shadow-[0_0_0_1px_color-mix(in_srgb,var(--color-terminal-info)_40%,transparent),0_18px_50px_rgba(0,0,0,0.36)]" } else { "text-terminal-fg absolute overflow-hidden border border-terminal-border-strong bg-terminal-bg-panel p-4 text-left text-xs shadow-[0_14px_36px_rgba(0,0,0,0.24)]" },
                style: "{style}",
                onclick: move |_| session.with_mut(|state| state.set_focus(target_id.clone())),

                div { class: "mb-4 flex items-center justify-between gap-3",
                    span {
                        class: "inline-grid h-9 w-9 place-items-center rounded-full text-[0.9rem] font-semibold text-black",
                        style: "background: color-mix(in srgb, {accent} 86%, white);",
                        "{badge}"
                    }
                    span { class: "text-[0.7rem] uppercase tracking-[0.16em] text-[color:color-mix(in_srgb,var(--accent)_72%,white)]",
                        "{subtitle}"
                    }
                }
                h3 { class: "mb-1 text-base font-semibold tracking-[-0.02em]", "{title}" }
                p { class: "text-terminal-muted text-sm", "{dimensions}" }
            }
        }
    });

    rsx! {
        section { class: if show_sidebar() { "grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2 xl:grid-cols-[minmax(0,1.55fr)_22rem]" } else { "grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2" },

            div { class: "grid min-h-0 gap-2",
                Panel {
                    PanelBar { class: Some("grid grid-cols-[auto_minmax(0,1fr)_auto] gap-2".to_string()),
                        div { class: "flex min-w-0 items-center gap-1 overflow-x-auto",
                            {workspace_buttons}
                        }

                        div { class: "text-terminal-fg-strong min-w-0 truncate px-2 text-center",
                            "{focused_window_label}"
                        }

                        div { class: "flex items-center gap-2 justify-self-end",
                            TerminalSelect {
                                value: snapshot.active_layout.title().to_string(),
                                aria_label: "Select preview layout".to_string(),
                                options: layout_options,
                                onchange: move |layout_name| {
                                    if let Some(layout) = PreviewLayoutId::ALL
                                        .iter()
                                        .copied()
                                        .find(|candidate| candidate.title() == layout_name)
                                    {
                                        session.with_mut(|state| state.switch_layout(layout));
                                    }
                                },
                            }
                            span { "{snapshot.visible_window_count()} windows" }
                            button {
                                class: if show_sidebar() { "border border-terminal-info bg-terminal-info/10 px-2 py-0.5 text-terminal-info" } else { "border border-terminal-border bg-terminal-bg-subtle px-2 py-0.5 text-terminal-dim hover:text-terminal-fg" },
                                onclick: move |_| show_sidebar.set(!show_sidebar()),
                                if show_sidebar() {
                                    "Hide info"
                                } else {
                                    "Show info"
                                }
                            }
                        }
                    }

                    div { class: "min-h-0 flex-1 overflow-hidden p-2",
                        div {
                            class: "bg-terminal-bg-subtle relative h-full min-h-72 w-full overflow-hidden border border-terminal-border",
                            style: "background-image: linear-gradient(color-mix(in srgb, var(--color-terminal-bg) 72%, transparent), color-mix(in srgb, var(--color-terminal-bg-subtle) 58%, transparent)), url('/archlinux-logo.svg'); background-position: center, center; background-repeat: no-repeat, no-repeat; background-size: cover, min(34rem, 56%);",
                            {window_cards}
                        }
                    }
                }

                Panel { class: Some("border-terminal-border/80".to_string()),
                    PanelBar {
                        div { "preview://controls" }
                        div { class: "text-terminal-muted", "{snapshot.summary()}" }
                    }

                    div { class: "grid gap-3 p-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-center",
                        div { class: "grid gap-1",
                            p { class: "text-terminal-fg-strong text-sm font-medium",
                                "{snapshot.display_title()}"
                            }
                            p { class: "text-terminal-muted text-sm", "{snapshot.prompt()}" }
                        }

                        div { class: "grid min-w-[16rem] grid-cols-3 gap-2",
                            button {
                                class: "border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong",
                                onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Up)),
                                "Up"
                            }
                            button {
                                class: "border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong",
                                onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Left)),
                                "Left"
                            }
                            button {
                                class: "border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong",
                                onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Right)),
                                "Right"
                            }
                            button {
                                class: "border border-terminal-border bg-terminal-bg-panel px-3 py-2 text-terminal-fg hover:border-terminal-info hover:text-terminal-fg-strong",
                                onclick: move |_| session.with_mut(|state| state.navigate(NavigationDirection::Down)),
                                "Down"
                            }
                            button {
                                class: "col-span-2 border border-terminal-info bg-terminal-info/10 px-3 py-2 text-terminal-info hover:bg-terminal-info/15",
                                onclick: move |_| session.with_mut(PreviewSessionState::reset),
                                "Reset"
                            }
                        }
                    }
                }
            }

            if show_sidebar() {
                div { class: "grid min-h-0 gap-2 xl:grid-rows-[auto_auto_auto_minmax(10rem,0.8fr)_minmax(12rem,1fr)]",
                    InspectorPanel { title: "session://windows".to_string(),
                        WindowList {
                            windows: visible_windows,
                            empty_label: "no windows".to_string(),
                        }
                    }

                    InspectorPanel { title: "session://unclaimed".to_string(),
                        WindowList {
                            windows: unclaimed_windows,
                            empty_label: "all claimed".to_string(),
                        }
                    }

                    InspectorPanel { title: "session://focus".to_string(),
                        div { class: "grid gap-2 p-2 text-sm",
                            div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1",
                                span { class: "text-terminal-faint block text-xs uppercase tracking-[0.12em]",
                                    "focused"
                                }
                                span { class: "text-terminal-fg-strong block pt-1",
                                    "{focused_window_label}"
                                }
                            }
                            div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1",
                                span { class: "text-terminal-faint block text-xs uppercase tracking-[0.12em]",
                                    "scope path"
                                }
                                if current_scope_path.is_empty() {
                                    span { class: "text-terminal-muted block pt-1", "none" }
                                } else {
                                    div { class: "flex flex-wrap gap-1 pt-1",
                                        for scope in current_scope_path {
                                            span { class: "border border-terminal-border bg-terminal-bg-hover px-2 py-0.5 text-xs text-terminal-fg-strong",
                                                "{scope}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    InspectorPanel { title: "scene://diagnostics".to_string(),
                        if diagnostics.is_empty() {
                            div { class: "p-2 text-sm text-terminal-faint", "no diagnostics" }
                        } else {
                            div { class: "grid gap-1 p-2 text-sm",
                                for diagnostic in diagnostics.iter().cloned() {
                                    div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1",
                                        div { class: "flex items-center gap-2 text-xs",
                                            span { class: "text-terminal-warn", "{diagnostic.level}" }
                                            span { class: "text-terminal-dim", "{diagnostic.source}" }
                                        }
                                        div { class: "pt-1 text-terminal-fg", "{diagnostic.message}" }
                                    }
                                }
                            }
                        }
                    }

                    InspectorPanel { title: "session://log".to_string(),
                        div { class: "grid gap-1 p-2 text-sm",
                            if event_log.is_empty() {
                                div { class: "text-terminal-faint", "no events" }
                            } else {
                                for entry in event_log.iter().cloned() {
                                    div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-terminal-fg",
                                        "{entry}"
                                    }
                                }
                            }

                            if !remembered_rows.is_empty() {
                                div { class: "pt-2 text-xs uppercase tracking-[0.12em] text-terminal-faint",
                                    "remembered scopes"
                                }
                                for (scope , window_name) in remembered_rows.iter().cloned() {
                                    div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1",
                                        div { class: "text-terminal-info text-xs", "{scope}" }
                                        div { class: "pt-1 text-terminal-fg text-sm",
                                            "{window_name}"
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

#[component]
fn InspectorPanel(title: String, children: Element) -> Element {
    rsx! {
        Panel {
            PanelBar {
                div { "{title}" }
            }
            div { class: "min-h-0 flex-1 overflow-auto", {children} }
        }
    }
}

#[component]
fn WindowList(windows: Vec<PreviewSessionWindow>, empty_label: String) -> Element {
    if windows.is_empty() {
        return rsx! {
            div { class: "p-2 text-sm text-terminal-faint", "{empty_label}" }
        };
    }

    rsx! {
        div { class: "grid gap-1 p-2 text-sm",
            for window in windows {
                div { class: "border border-terminal-border bg-terminal-bg-panel px-2 py-1",
                    div { class: "text-terminal-info text-xs", "{window.badge} · {window.subtitle}" }
                    div { class: "pt-1 text-terminal-fg", "{window.display_title()}" }
                    div { class: "pt-1 text-terminal-muted text-xs",
                        "{window.geometry.width} × {window.geometry.height}"
                    }
                }
            }
        }
    }
}
