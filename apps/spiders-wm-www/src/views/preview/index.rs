use leptos::prelude::*;
use spiders_core::command::WmCommand;
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::wm::WindowGeometry;
use spiders_css::ColorValue;
use spiders_titlebar_core::{
    TitlebarButtonKind, TitlebarButtonsConfig, TitlebarPlan, TitlebarPlanInput, TitlebarPlanPreset,
    build_titlebar_plan_with_preset,
};
use spiders_titlebar_web::{
    WebTitlebarButtonState, WebTitlebarTrailingContent,
    view_model_for_titlebar_with_button_state_and_trailing,
};

use crate::app_state::AppState;
use crate::components::{Panel, PanelBar, TerminalSelect, TerminalSelectOption};
use crate::session::{
    PreviewDiagnostic, PreviewSessionState, PreviewSnapshotNode, preview_layout_ids,
};

fn pane_style(
    geometry: WindowGeometry,
    accent: &str,
    canvas_width: i32,
    canvas_height: i32,
) -> String {
    let left = geometry.x as f32 / canvas_width as f32 * 100.0;
    let top = geometry.y as f32 / canvas_height as f32 * 100.0;
    let width = geometry.width as f32 / canvas_width as f32 * 100.0;
    let height = geometry.height as f32 / canvas_height as f32 * 100.0;

    format!(
        "left: {left:.3}%; top: {top:.3}%; width: {width:.3}%; height: {height:.3}%; --accent: {};",
        accent,
    )
}

fn frame_style(layout_style: Option<&spiders_scene::ComputedStyle>, focused: bool) -> String {
    let background =
        layout_style.and_then(|style| style.background).map(css_color).unwrap_or_else(|| {
            if focused {
                "rgba(20, 24, 33, 0.98)".to_string()
            } else {
                "rgba(24, 27, 36, 0.94)".to_string()
            }
        });
    let opacity = layout_style.and_then(|style| style.opacity).unwrap_or(1.0).clamp(0.0, 1.0);
    let radius = layout_style.and_then(|style| style.border_radius);
    let border = layout_style.and_then(|style| style.border);
    let border_style = layout_style.and_then(|style| style.border_style);
    let border_color = layout_style
        .and_then(|style| style.border_color)
        .or_else(|| {
            layout_style.and_then(|style| style.border_side_colors).and_then(|colors| colors.top)
        })
        .map(css_color)
        .unwrap_or_else(|| {
            if focused {
                "rgba(93, 173, 226, 0.55)".to_string()
            } else {
                "rgba(80, 86, 104, 0.7)".to_string()
            }
        });
    let border_width = border
        .map(|edges| {
            border_length_to_px(edges.top)
                .max(border_length_to_px(edges.right))
                .max(border_length_to_px(edges.bottom))
                .max(border_length_to_px(edges.left))
        })
        .unwrap_or(1)
        .max(0);
    let border_css =
        if matches!(border_style.map(|edges| edges.top), Some(spiders_css::BorderStyleValue::None))
            || border_width == 0
        {
            "border: none;".to_string()
        } else {
            format!("border: {border_width}px solid {border_color};")
        };

    [
        format!("background: {background};"),
        format!("opacity: {opacity:.3};"),
        border_css,
        radius
            .map(|radius| {
                format!(
                    "border-radius: {}px {}px {}px {}px;",
                    radius.top_left, radius.top_right, radius.bottom_right, radius.bottom_left
                )
            })
            .unwrap_or_default(),
    ]
    .join(" ")
}

fn body_style(layout_style: Option<&spiders_scene::ComputedStyle>) -> String {
    let padding = layout_style.and_then(|style| style.padding);
    let overflow = layout_style.and_then(|style| style.overflow_y);
    let padding_css = padding
        .map(|padding| {
            format!(
                "padding: {}px {}px {}px {}px;",
                border_length_to_px(padding.top),
                border_length_to_px(padding.right),
                border_length_to_px(padding.bottom),
                border_length_to_px(padding.left)
            )
        })
        .unwrap_or_default();
    let overflow_css = match overflow {
        Some(spiders_css::OverflowValue::Hidden | spiders_css::OverflowValue::Clip) => {
            "overflow: hidden;".to_string()
        }
        Some(spiders_css::OverflowValue::Scroll) => "overflow: auto;".to_string(),
        _ => String::new(),
    };

    format!("{padding_css} {overflow_css}")
}

fn css_color(color: ColorValue) -> String {
    format!(
        "rgba({}, {}, {}, {:.3})",
        color.red,
        color.green,
        color.blue,
        f32::from(color.alpha) / 255.0
    )
}

fn border_length_to_px(length: spiders_css::LengthPercentage) -> i32 {
    match length {
        spiders_css::LengthPercentage::Px(value)
        | spiders_css::LengthPercentage::Percent(value) => value.round() as i32,
    }
    .max(0)
}

fn window_display_title(window: &WindowSnapshot) -> &str {
    window.title.as_deref().unwrap_or_else(|| window.id.as_str())
}

fn window_subtitle(window: &WindowSnapshot) -> &str {
    window.app_id.as_deref().unwrap_or("preview window")
}

fn window_accent(window: &WindowSnapshot) -> String {
    const PALETTE: [&str; 8] =
        ["#7dd3fc", "#f97316", "#34d399", "#facc15", "#818cf8", "#06b6d4", "#e879f9", "#fb7185"];

    let seed = window.title.as_deref().or(window.app_id.as_deref()).unwrap_or(window.id.as_str());
    let hash = seed
        .bytes()
        .chain(window.id.as_str().bytes())
        .fold(0usize, |acc, byte| acc + byte as usize);

    PALETTE[hash % PALETTE.len()].to_string()
}

fn titlebar_plan_with_styles(
    window: &WindowSnapshot,
    focused: bool,
    layout_style: Option<spiders_scene::ComputedStyle>,
    titlebar_style: Option<spiders_scene::ComputedStyle>,
) -> TitlebarPlan {
    build_titlebar_plan_with_preset(
        &TitlebarPlanInput {
            window_id: window.id.clone(),
            title: window_display_title(window).to_string(),
            focused,
            titlebar_style,
            window_style: layout_style,
            default_background_focused: spiders_css::ColorValue {
                red: 22,
                green: 31,
                blue: 45,
                alpha: 245,
            },
            default_background_unfocused: spiders_css::ColorValue {
                red: 25,
                green: 25,
                blue: 30,
                alpha: 235,
            },
            default_text_color_focused: spiders_css::ColorValue {
                red: 230,
                green: 233,
                blue: 239,
                alpha: 255,
            },
            default_text_color_unfocused: spiders_css::ColorValue {
                red: 230,
                green: 233,
                blue: 239,
                alpha: 255,
            },
            offset_x: 0,
            offset_y: 0,
            effective_opacity: 1.0,
            buttons: TitlebarButtonsConfig::default(),
        },
        &TitlebarPlanPreset::default(),
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

fn descendant_window_titles(node: &PreviewSnapshotNode, windows: &[WindowSnapshot]) -> String {
    let mut ids = Vec::new();
    descendant_window_ids(node, &mut ids);

    ids.into_iter()
        .map(|window_id| {
            windows
                .iter()
                .find(|window| window.id.as_str() == window_id)
                .map(|window| window_display_title(window).to_string())
                .unwrap_or(window_id)
        })
        .collect::<Vec<_>>()
        .join("  |  ")
}

#[component]
pub fn PreviewView() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let show_sidebar = RwSignal::new(false);
    let layout_options = preview_layout_ids()
        .map(|layout| TerminalSelectOption {
            value: layout.as_str().to_string(),
            label: layout.as_str().to_string(),
        })
        .collect::<Vec<_>>();
    let layout_value =
        Signal::derive(move || app_state.session.get().active_layout().as_str().to_string());

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
                                    .workspace_names()
                                    .iter()
                                    .cloned()
                                    .map(|workspace_name| {
                                        let target_workspace = workspace_name.clone();
                                        let class_workspace = workspace_name.clone();

                                        view! {
                                            <button
                                                class=move || {
                                                    if app_state.session.get().active_workspace_name()
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
                                onchange=Callback::new(move |layout_name: String| {
                                    if preview_layout_ids()
                                        .any(|candidate| candidate.as_str() == layout_name)
                                    {
                                        app_state
                                            .session
                                            .update(|state| {
                                                state.switch_layout(layout_name.as_str().into())
                                            });
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
                            when=move || {
                                let session = app_state.session.get();
                                session.snapshot_root.is_some() || !session.diagnostics.is_empty()
                            }
                            fallback=move || {
                                view! {
                                    <div class="flex justify-center items-center p-3 h-full text-sm text-terminal-faint min-h-72">
                                        "loading wasm preview..."
                                    </div>
                                }
                            }
                        >
                            {move || {
                                let session = app_state.session.get();
                                if session.snapshot_root.is_none() {
                                    view! {
                                        <div class="overflow-auto p-3 w-full h-full text-sm border border-terminal-border bg-terminal-bg-subtle text-terminal-muted min-h-72">
                                            <DiagnosticsList diagnostics=Signal::derive(move || {
                                                app_state.session.get().diagnostics.clone()
                                            }) />
                                        </div>
                                    }
                                        .into_any()
                                } else {
                                    view! {
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

                                            {app_state
                                                .session
                                                .get()
                                                .claimed_visible_windows()
                                                .into_iter()
                                                .map(|window| {
                                                    let style_window = window.clone();
                                                    let surface_window = window.clone();
                                                    let focus_target = window.id.clone();
                                                    let pane_focus_target = focus_target.clone();
                                                    let focused_id = window.id.clone();
                                                    let accent = window_accent(&window);
                                                    let geometry = app_state
                                                        .session
                                                        .get()
                                                        .window_geometry(&window.id);
                                                    let dimensions = format!(
                                                        "{}x{}",
                                                        geometry.width,
                                                        geometry.height,
                                                    );
                                                    let is_foot = window.app_id.as_deref() == Some("foot");
                                                    let focused = app_state
                                                        .session
                                                        .get()
                                                        .focused_window_id()
                                                        .as_ref()
                                                        == Some(&window.id);
                                                    let (layout_style, titlebar_style) = app_state
                                                        .session
                                                        .get()
                                                        .window_titlebar_styles(&window.id);
                                                    let hovered_button = RwSignal::new(None::<TitlebarButtonKind>);
                                                    let active_button = RwSignal::new(None::<TitlebarButtonKind>);
                                                    let frame_style_value = frame_style(layout_style.as_ref(), focused);
                                                    let body_style_value = body_style(layout_style.as_ref());
                                                    let titlebar_plan = titlebar_plan_with_styles(
                                                        &window,
                                                        focused,
                                                        layout_style,
                                                        titlebar_style,
                                                    );
                                                    let trailing = WebTitlebarTrailingContent {
                                                        text: dimensions.clone(),
                                                        width_px: (dimensions.chars().count() as i32 * 8 + 8).max(0),
                                                    };
                                                    let titlebar = view_model_for_titlebar_with_button_state_and_trailing(
                                                        &titlebar_plan,
                                                        &titlebar_plan
                                                            .buttons
                                                            .iter()
                                                            .map(|button| {
                                                                (
                                                                    button.kind,
                                                                    WebTitlebarButtonState {
                                                                        hovered: hovered_button.get() == Some(button.kind),
                                                                        active: active_button.get() == Some(button.kind),
                                                                    },
                                                                )
                                                            })
                                                            .collect::<Vec<_>>(),
                                                        Some(&trailing),
                                                    );

                                                    view! {
                                                        <div
                                                            class=move || {
                                                                if app_state.session.get().focused_window_id().as_ref()
                                                                    == Some(&focused_id)
                                                                {
                                                                    "text-terminal-fg absolute z-20 overflow-hidden text-left text-xs cursor-pointer"
                                                                } else {
                                                                    "text-terminal-fg absolute z-20 overflow-hidden text-left text-xs cursor-pointer"
                                                                }
                                                            }
                                                            style=move || {
                                                                let snapshot = app_state.session.get();
                                                                format!(
                                                                    "{} {}",
                                                                    pane_style(
                                                                    snapshot.window_geometry(&style_window.id),
                                                                    &accent,
                                                                    snapshot.canvas_width(),
                                                                    snapshot.canvas_height(),
                                                                ),
                                                                    frame_style_value,
                                                                )
                                                            }
                                                            on:click=move |_| {
                                                                app_state
                                                                    .session
                                                                    .update(|state| state.set_focus(pane_focus_target.clone()));
                                                            }
                                                        >
                                                            <div style=titlebar.outer_style.clone()>
                                                                {titlebar
                                                                    .buttons
                                                                    .iter()
                                                                    .cloned()
                                                                    .map(|button| {
                                                                        let button_kind = button.kind;
                                                                        let button_label = button.label.clone();
                                                                        let button_aria_label = button.label.clone();
                                                                        let button_focus_target = focus_target.clone();
                                                                        view! {
                                                                            <button
                                                                                style=button.style
                                                                                title=button_label
                                                                                aria-label=button_aria_label
                                                                                on:pointerenter=move |_| {
                                                                                    hovered_button.set(Some(button_kind));
                                                                                }
                                                                                on:pointerleave=move |_| {
                                                                                    hovered_button.set(None);
                                                                                    active_button.set(None);
                                                                                }
                                                                                on:mousedown=move |_| {
                                                                                    active_button.set(Some(button_kind));
                                                                                }
                                                                                on:mouseup=move |_| {
                                                                                    active_button.set(None);
                                                                                }
                                                                                on:click=move |event| {
                                                                                    event.stop_propagation();
                                                                                    active_button.set(None);
                                                                                    app_state.session.update(|state| {
                                                                                        state.set_focus(button_focus_target.clone());
                                                                                        state.apply_command(match button_kind {
                                                                                            TitlebarButtonKind::Close => WmCommand::CloseFocusedWindow,
                                                                                            TitlebarButtonKind::ToggleFullscreen => WmCommand::ToggleFullscreen,
                                                                                            TitlebarButtonKind::ToggleFloating => WmCommand::ToggleFloating,
                                                                                        });
                                                                                    });
                                                                                }
                                                                            />
                                                                        }
                                                                    })
                                                                    .collect_view()}
                                                                <div class="flex justify-between gap-2 items-center w-full min-w-0 text-xs" style=titlebar.title_style.clone()>
                                                                    <span class="truncate min-w-0 flex-1">{titlebar.title}</span>
                                                                </div>
                                                                <span class="text-terminal-dim shrink-0 text-xs" style=titlebar.trailing_style.clone()>{trailing.text}</span>
                                                            </div>

                                                            <div style=body_style_value.clone()>
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
                                                            </div>
                                                        </div>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                    }
                                        .into_any()
                                }
                            }}
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
    #[prop(into)] windows: Signal<Vec<WindowSnapshot>>,
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
                            let title = window_display_title(&window).to_string();
                            let app_id = window.app_id.unwrap_or_else(|| "unknown".to_string());

                            view! {
                                <div class=if window.focused {
                                    "flex items-center gap-2 border border-terminal-info bg-terminal-bg-active px-2 py-1"
                                } else {
                                    "flex items-center gap-2 border border-terminal-border bg-terminal-bg-panel px-2 py-1"
                                }>
                                    <span class="text-terminal-fg-strong">{title}</span>
                                    <Show when=move || window.mode.is_floating()>
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
fn WindowSurface(window: WindowSnapshot) -> impl IntoView {
    view! {
        <div class="flex flex-col p-2 text-sm text-terminal-muted h-[calc(100%-1.5rem)]">
            <div>
                <div class="text-terminal-fg-strong">{window_subtitle(&window).to_string()}</div>
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
    #[prop(into)] windows: Signal<Vec<WindowSnapshot>>,
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
                                format!("{}:{}", index + 1, window_display_title(&window))
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
    windows: Vec<WindowSnapshot>,
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
