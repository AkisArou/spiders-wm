use leptos::prelude::*;
use spiders_core::command::WmCommand;
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::wm::WindowGeometry;
use spiders_css::ColorValue;
use spiders_titlebar_core::{
    TitlebarButtonAction, titlebar_button_action_from_data, titlebar_icon_nodes_from_data,
    titlebar_icon_paths, titlebar_icon_view_box,
};
use spiders_wm_runtime::PreviewSnapshotClasses;

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
                "rgba(20, 24, 33, 1.0)".to_string()
            } else {
                "rgba(24, 27, 36, 1.0)".to_string()
            }
        });
    let radius = layout_style.and_then(|style| style.border_radius);
    let border = layout_style.and_then(|style| style.border);
    let border_style = layout_style.and_then(|style| style.border_style);
    let border_color = if focused {
        "rgba(125, 211, 199, 1.0)".to_string()
    } else {
        layout_style
            .and_then(|style| style.border_color)
            .or_else(|| {
                layout_style
                    .and_then(|style| style.border_side_colors)
                    .and_then(|colors| colors.top)
            })
            .map(css_color)
            .unwrap_or_else(|| "rgba(47, 54, 71, 1.0)".to_string())
    };
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
            let border_width = border_width.max(1);
            format!("border: {border_width}px solid {border_color};")
        };

    [
        format!("background: {background};"),
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

    format!("position: relative; width: 100%; {padding_css} {overflow_css}")
}

fn titlebar_height_px(
    titlebar_node: Option<&PreviewSnapshotNode>,
    titlebar_style: Option<&spiders_scene::ComputedStyle>,
) -> i32 {
    titlebar_node
        .and_then(|node| node.rect.map(|rect| rect.height.round() as i32))
        .filter(|height| *height > 0)
        .or_else(|| {
            titlebar_style
                .and_then(|style| style.height)
                .map(size_value_to_px)
                .filter(|height| *height > 0)
        })
        .unwrap_or(0)
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

fn size_value_to_px(size: spiders_css::SizeValue) -> i32 {
    match size {
        spiders_css::SizeValue::Auto => 0,
        spiders_css::SizeValue::LengthPercentage(length) => border_length_to_px(length),
    }
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

fn css_size_value(value: spiders_css::SizeValue) -> Option<String> {
    match value {
        spiders_css::SizeValue::Auto => Some("auto".to_string()),
        spiders_css::SizeValue::LengthPercentage(length) => Some(css_length_percentage(length)),
    }
}

fn css_length_percentage(value: spiders_css::LengthPercentage) -> String {
    match value {
        spiders_css::LengthPercentage::Px(value) => format!("{value}px"),
        spiders_css::LengthPercentage::Percent(value) => format!("{value}%"),
    }
}

fn snapshot_class_name(class_name: Option<&PreviewSnapshotClasses>) -> Option<String> {
    match class_name {
        Some(PreviewSnapshotClasses::One(class_name)) => Some(class_name.clone()),
        Some(PreviewSnapshotClasses::Many(class_names)) if !class_names.is_empty() => {
            Some(class_names.join(" "))
        }
        _ => None,
    }
}

fn content_style(node: &PreviewSnapshotNode, container_rect: spiders_core::LayoutRect) -> String {
    let mut parts = Vec::new();
    if let Some(rect) = node.rect {
        if node.node_type == "titlebar" {
            let left = if container_rect.width > 0.0 {
                ((rect.x - container_rect.x) / container_rect.width) * 100.0
            } else {
                0.0
            };
            let top = if container_rect.height > 0.0 {
                ((rect.y - container_rect.y) / container_rect.height) * 100.0
            } else {
                0.0
            };
            let width = if container_rect.width > 0.0 {
                (rect.width / container_rect.width) * 100.0
            } else {
                0.0
            };
            let height = if container_rect.height > 0.0 {
                (rect.height / container_rect.height) * 100.0
            } else {
                0.0
            };

            parts.push(format!(
                "position: absolute; left: {left:.4}%; top: {top:.4}%; width: {width:.4}%; height: {height:.4}%;"
            ));
        } else if node.node_type == "titlebar-button" || node.node_type == "titlebar-icon" {
            if rect.width > 0.0 {
                parts.push(format!("width: {}px;", rect.width));
            }
            if rect.height > 0.0 {
                parts.push(format!("height: {}px;", rect.height));
            }
        }
    }

    if let Some(style) = node.layout_style.as_ref() {
        if let Some(background) = style.background {
            parts.push(format!("background: {};", css_color(background)));
        }
        if let Some(color) = style.color {
            parts.push(format!("color: {};", css_color(color)));
        }
        if let Some(opacity) = style.opacity {
            parts.push(format!("opacity: {:.3};", opacity.clamp(0.0, 1.0)));
        }
        if let Some(padding) = style.padding {
            parts.push(format!(
                "padding: {}px {}px {}px {}px;",
                border_length_to_px(padding.top),
                border_length_to_px(padding.right),
                border_length_to_px(padding.bottom),
                border_length_to_px(padding.left)
            ));
        }
        if let Some(gap) = style.gap {
            parts.push(format!(
                "column-gap: {}px; row-gap: {}px;",
                border_length_to_px(gap.width),
                border_length_to_px(gap.height)
            ));
        }
        if let Some(display) = style.display {
            let display_css = match display {
                spiders_css::Display::Block => "block",
                spiders_css::Display::Flex => "flex",
                spiders_css::Display::Grid => "grid",
                spiders_css::Display::None => "none",
            };
            parts.push(format!("display: {display_css};"));
        }
        if let Some(direction) = style.flex_direction {
            let direction_css = match direction {
                spiders_css::FlexDirectionValue::Row => "row",
                spiders_css::FlexDirectionValue::Column => "column",
                spiders_css::FlexDirectionValue::RowReverse => "row-reverse",
                spiders_css::FlexDirectionValue::ColumnReverse => "column-reverse",
            };
            parts.push(format!("flex-direction: {direction_css};"));
        }
        if let Some(flex_grow) = style.flex_grow {
            parts.push(format!("flex-grow: {flex_grow};"));
        }
        if let Some(flex_shrink) = style.flex_shrink {
            parts.push(format!("flex-shrink: {flex_shrink};"));
        }
        if let Some(flex_basis) = style.flex_basis.and_then(css_size_value) {
            parts.push(format!("flex-basis: {flex_basis};"));
        }
        if let Some(justify) = style.justify_content {
            let justify_css = match justify {
                spiders_css::ContentAlignmentValue::Start => "start",
                spiders_css::ContentAlignmentValue::End => "end",
                spiders_css::ContentAlignmentValue::FlexStart => "flex-start",
                spiders_css::ContentAlignmentValue::FlexEnd => "flex-end",
                spiders_css::ContentAlignmentValue::Center => "center",
                spiders_css::ContentAlignmentValue::Stretch => "stretch",
                spiders_css::ContentAlignmentValue::SpaceBetween => "space-between",
                spiders_css::ContentAlignmentValue::SpaceEvenly => "space-evenly",
                spiders_css::ContentAlignmentValue::SpaceAround => "space-around",
            };
            parts.push(format!("justify-content: {justify_css};"));
        }
        if let Some(align) = style.align_items {
            let align_css = match align {
                spiders_css::AlignmentValue::Start => "start",
                spiders_css::AlignmentValue::End => "end",
                spiders_css::AlignmentValue::FlexStart => "flex-start",
                spiders_css::AlignmentValue::FlexEnd => "flex-end",
                spiders_css::AlignmentValue::Center => "center",
                spiders_css::AlignmentValue::Baseline => "baseline",
                spiders_css::AlignmentValue::Stretch => "stretch",
            };
            parts.push(format!("align-items: {align_css};"));
        }
        if let Some(width) = style.width.and_then(css_size_value) {
            parts.push(format!("width: {width};"));
        }
        if let Some(height) = style.height.and_then(css_size_value) {
            parts.push(format!("height: {height};"));
        }
        if let Some(min_width) = style.min_width.and_then(css_size_value) {
            parts.push(format!("min-width: {min_width};"));
        }
        if let Some(min_height) = style.min_height.and_then(css_size_value) {
            parts.push(format!("min-height: {min_height};"));
        }
        if let Some(max_width) = style.max_width.and_then(css_size_value) {
            parts.push(format!("max-width: {max_width};"));
        }
        if let Some(max_height) = style.max_height.and_then(css_size_value) {
            parts.push(format!("max-height: {max_height};"));
        }
        if let Some(radius) = style.border_radius {
            parts.push(format!(
                "border-radius: {}px {}px {}px {}px;",
                radius.top_left, radius.top_right, radius.bottom_right, radius.bottom_left
            ));
        }
        if let Some(text_align) = style.text_align {
            let text_align_css = match text_align {
                spiders_css::TextAlignValue::Left => "left",
                spiders_css::TextAlignValue::Right => "right",
                spiders_css::TextAlignValue::Center => "center",
                spiders_css::TextAlignValue::Start => "start",
                spiders_css::TextAlignValue::End => "end",
            };
            parts.push(format!("text-align: {text_align_css};"));
        }
        if let Some(font_size) = style.font_size {
            parts.push(format!("font-size: {}px;", border_length_to_px(font_size)));
        }
        if let Some(font_weight) = style.font_weight {
            let font_weight_css = match font_weight {
                spiders_css::FontWeightValue::Normal => "400",
                spiders_css::FontWeightValue::Bold => "700",
            };
            parts.push(format!("font-weight: {font_weight_css};"));
        }
    }

    parts.join(" ")
}

fn titlebar_snapshot_node<'a>(
    node: &'a PreviewSnapshotNode,
    window_id: &spiders_core::WindowId,
) -> Option<&'a PreviewSnapshotNode> {
    if node.window_id.as_ref() == Some(window_id) {
        return node.children.iter().find(|child| child.node_type == "titlebar");
    }

    node.children.iter().find_map(|child| titlebar_snapshot_node(child, window_id))
}

fn titlebar_action_command(node: &PreviewSnapshotNode) -> Option<WmCommand> {
    match titlebar_button_action_from_data(&node.data) {
        Some(TitlebarButtonAction::Close) => Some(WmCommand::CloseFocusedWindow),
        Some(TitlebarButtonAction::ToggleFullscreen) => Some(WmCommand::ToggleFullscreen),
        Some(TitlebarButtonAction::ToggleFloating) => Some(WmCommand::ToggleFloating),
        _ => None,
    }
}

fn titlebar_icon_paths_for_node(node: &PreviewSnapshotNode) -> Vec<String> {
    titlebar_icon_nodes_from_data(&node.data)
        .map(|nodes| titlebar_icon_paths(&nodes))
        .unwrap_or_default()
}

fn titlebar_icon_view_box_for_node(node: &PreviewSnapshotNode) -> String {
    titlebar_icon_nodes_from_data(&node.data)
        .and_then(|nodes| titlebar_icon_view_box(&nodes))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "0 0 16 16".to_string())
}

#[component]
fn SnapshotIconNode(node: PreviewSnapshotNode) -> impl IntoView {
    let paths = titlebar_icon_paths_for_node(&node);
    let view_box = titlebar_icon_view_box_for_node(&node);

    view! {
        <svg viewBox=view_box width="100%" height="100%" fill="currentColor" aria-hidden="true">
            {paths
                .into_iter()
                .map(|d| view! { <path d=d /> })
                .collect_view()}
        </svg>
    }
}

#[component]
fn SnapshotContentNode(
    node: PreviewSnapshotNode,
    container_rect: spiders_core::LayoutRect,
    on_action: Callback<WmCommand>,
) -> AnyView {
    let text = node.text.clone().unwrap_or_default();
    let children = node.children.clone();
    let style = content_style(&node, container_rect);
    let command = titlebar_action_command(&node);
    let is_button = node.node_type == "titlebar-button" && command.is_some();
    let next_container_rect = node.rect.unwrap_or(container_rect);
    let class_name = snapshot_class_name(node.class_name.as_ref());

    if !is_button {
        let icon_paths = titlebar_icon_paths_for_node(&node);
        if !icon_paths.is_empty() {
            return view! {
                <div class=class_name style=style>
                    <SnapshotIconNode node=node />
                </div>
            }
            .into_any();
        }
    }

    if is_button {
        let command = command.expect("checked above");
        view! {
            <button
                type="button"
                class=class_name
                style=style
                on:click=move |event| {
                    event.stop_propagation();
                    on_action.run(command.clone());
                }
            >
                {(!text.is_empty()).then(|| view! { <span>{text.clone()}</span> })}
                {children
                    .into_iter()
                    .map(|child| {
                        view! {
                            <SnapshotContentNode
                                node=child
                                container_rect=next_container_rect
                                on_action=on_action
                            />
                        }
                    })
                    .collect_view()}
            </button>
        }
        .into_any()
    } else {
        view! {
            <div class=class_name style=style>
                {(!text.is_empty()).then(|| view! { <span>{text.clone()}</span> })}
                {children
                    .into_iter()
                    .map(|child| {
                        view! {
                            <SnapshotContentNode
                                node=child
                                container_rect=next_container_rect
                                on_action=on_action
                            />
                        }
                    })
                    .collect_view()}
            </div>
        }
        .into_any()
    }
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
                                            class="overflow-hidden relative w-full h-full bg-terminal-bg min-h-72"
                                        >
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
                                                    let focused = Signal::derive(move || {
                                                        app_state.session.get().focused_window_id().as_ref()
                                                            == Some(&focused_id)
                                                    });
                                                    let titlebar_node = app_state
                                                        .session
                                                        .get()
                                                        .snapshot_root
                                                        .as_ref()
                                                        .and_then(|root| titlebar_snapshot_node(root, &window.id))
                                                        .cloned();
                                                    let (layout_style, titlebar_style) = app_state
                                                        .session
                                                        .get()
                                                        .window_titlebar_styles(&window.id);
                                                    let show_titlebar_node = titlebar_node.clone();
                                                    let show_titlebar_style = titlebar_style.clone();
                                                    let render_titlebar_node = titlebar_node.clone();
                                                    let render_titlebar_style = titlebar_style.clone();
                                                    let fallback_window_title = window_display_title(&window).to_string();
                                                    let fallback_window_id = window.id.clone();
                                                    let titlebar_focus_target = focus_target.clone();
                                                    let titlebar_action = Callback::new(move |command: WmCommand| {
                                                        app_state.session.update(|state| {
                                                            state.set_focus(titlebar_focus_target.clone());
                                                            state.apply_command(command);
                                                        });
                                                    });
                                                    let body_style_value = body_style(layout_style.as_ref());
                                                    let resolved_titlebar_height = titlebar_height_px(
                                                        titlebar_node.as_ref(),
                                                        titlebar_style.as_ref(),
                                                    );

                                                    view! {
                                                        <div
                                                            class=move || {
                                                                if focused.get() {
                                                                    "text-terminal-fg absolute z-20 overflow-hidden text-left text-xs cursor-pointer"
                                                                } else {
                                                                    "text-terminal-fg absolute z-20 overflow-hidden text-left text-xs cursor-pointer"
                                                                }
                                                            }
                                                            attr:data-focused=move || {
                                                                if focused.get() { "true" } else { "false" }
                                                            }
                                                            style=move || {
                                                                let snapshot = app_state.session.get();
                                                                let layout_style = snapshot
                                                                    .window_titlebar_styles(&style_window.id)
                                                                    .0;
                                                                format!(
                                                                    "{} {}",
                                                                    pane_style(
                                                                    snapshot.window_geometry(&style_window.id),
                                                                    &accent,
                                                                    snapshot.canvas_width(),
                                                                    snapshot.canvas_height(),
                                                                ),
                                                                    frame_style(layout_style.as_ref(), focused.get()),
                                                                )
                                                            }
                                                            on:click=move |_| {
                                                                app_state
                                                                    .session
                                                                    .update(|state| state.set_focus(pane_focus_target.clone()));
                                                            }
                                                        >
                                                            <Show
                                                                when=move || show_titlebar_node.is_some() || show_titlebar_style.is_some()
                                                                fallback=move || view! { <></> }
                                                            >
                                                                {render_titlebar_node
                                                                    .clone()
                                                                    .map(|node| {
                                                                        let container_rect = spiders_core::LayoutRect {
                                                                            x: geometry.x as f32,
                                                                            y: geometry.y as f32,
                                                                            width: geometry.width as f32,
                                                                            height: geometry.height as f32,
                                                                        };
                                                                        view! {
                                                                            <SnapshotContentNode
                                                                                node=node
                                                                                container_rect=container_rect
                                                                                on_action=titlebar_action
                                                                            />
                                                                        }
                                                                        .into_any()
                                                                    })
                                                                    .unwrap_or_else(|| {
                                                                        let fallback_height = render_titlebar_style
                                                                            .as_ref()
                                                                            .and_then(|style| style.height)
                                                                            .map(size_value_to_px)
                                                                            .unwrap_or(28) as f32;
                                                                        view! {
                                                                            <div style=content_style(&PreviewSnapshotNode {
                                                                                node_type: "titlebar".to_string(),
                                                                                id: None,
                                                                                class_name: None,
                                                                                rect: Some(spiders_core::LayoutRect {
                                                                                    x: 0.0,
                                                                                    y: 0.0,
                                                                                    width: geometry.width as f32,
                                                                                    height: fallback_height,
                                                                                }),
                                                                                window_id: None,
                                                                                axis: None,
                                                                                reverse: false,
                                                                                layout_style: render_titlebar_style.clone(),
                                                                                titlebar_style: None,
                                                                                text: None,
                                                                                data: Default::default(),
                                                                                children: Vec::new(),
                                                                            }, spiders_core::LayoutRect {
                                                                                x: 0.0,
                                                                                y: 0.0,
                                                                                width: geometry.width as f32,
                                                                                height: geometry.height as f32,
                                                                            })>
                                                                                <span class="truncate min-w-0 flex-1">{fallback_window_title.clone()}</span>
                                                                                <span class="text-terminal-dim shrink-0 text-xs">{dimensions.clone()}</span>
                                                                            </div>
                                                                        }
                                                                        .into_any()
                                                                    })}
                                                            </Show>

                                                            <div style=move || {
                                                                if resolved_titlebar_height > 0 {
                                                                    format!(
                                                                        "margin-top: {}px; width: 100%; height: calc(100% - {}px); box-sizing: border-box; {}",
                                                                        resolved_titlebar_height,
                                                                        resolved_titlebar_height,
                                                                        body_style_value.clone(),
                                                                    )
                                                                } else {
                                                                    format!(
                                                                        "height: 100%; width: 100%; box-sizing: border-box; {}",
                                                                        body_style_value.clone(),
                                                                    )
                                                                }
                                                            }>
                                                            {if is_foot {
                                                                view! {
                                                                    <FootTerminal focused=Signal::derive(move || {
                                                                        app_state.session.get().focused_window_id().as_ref()
                                                                            == Some(&fallback_window_id)
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
        <div class="flex h-full w-full flex-col text-sm text-terminal-muted">
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
        <div class="flex h-full w-full items-start text-sm bg-terminal-bg text-terminal-fg">
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
