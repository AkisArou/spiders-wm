use super::*;
use spiders_scene::{
    AppearanceValue, BorderStyleValue, BoxEdges, ColorValue, ComputedStyle, LayoutSnapshotNode,
    FontWeightValue, LengthPercentage, SizeValue, TextAlignValue, TextTransformValue,
};
use spiders_shared::types::WindowMode;
use spiders_tree::LayoutRect;
use crate::backend::plan::TitlebarPlan;
use crate::actions::{
    active_tiled_window_ids, compute_horizontal_tiled_edges, compute_pointer_render_positions,
    compute_window_borders, configured_mode_for_window, directional_neighbor_window_id,
    inactive_window_ids, top_window_id,
};
use crate::layout_adapter::compute_layout_snapshot;

fn border_length_to_px(length: LengthPercentage) -> i32 {
    match length {
        LengthPercentage::Px(value) | LengthPercentage::Percent(value) => value.round() as i32,
    }
    .max(0)
}

fn river_border_from_box_edges(
    border: BoxEdges<LengthPercentage>,
) -> (river_window_v1::Edges, i32) {
    let edge_widths = [
        (river_window_v1::Edges::Top, border_length_to_px(border.top)),
        (river_window_v1::Edges::Right, border_length_to_px(border.right)),
        (river_window_v1::Edges::Bottom, border_length_to_px(border.bottom)),
        (river_window_v1::Edges::Left, border_length_to_px(border.left)),
    ];

    edge_widths.into_iter().fold(
        (river_window_v1::Edges::None, 0),
        |(edges, width), (edge, edge_width)| {
            if edge_width > 0 {
                (edges | edge, width.max(edge_width))
            } else {
                (edges, width)
            }
        },
    )
}

fn apply_border_styles(
    border: BoxEdges<LengthPercentage>,
    border_style: Option<BoxEdges<BorderStyleValue>>,
) -> BoxEdges<LengthPercentage> {
    let Some(border_style) = border_style else {
        return border;
    };

    BoxEdges {
        top: if matches!(border_style.top, BorderStyleValue::None) {
            LengthPercentage::Px(0.0)
        } else {
            border.top
        },
        right: if matches!(border_style.right, BorderStyleValue::None) {
            LengthPercentage::Px(0.0)
        } else {
            border.right
        },
        bottom: if matches!(border_style.bottom, BorderStyleValue::None) {
            LengthPercentage::Px(0.0)
        } else {
            border.bottom
        },
        left: if matches!(border_style.left, BorderStyleValue::None) {
            LengthPercentage::Px(0.0)
        } else {
            border.left
        },
    }
}

fn river_border_from_layout_node(node: &LayoutSnapshotNode) -> Option<(river_window_v1::Edges, i32)> {
    let style = &node.styles()?.layout;
    let border = apply_border_styles(style.border?, style.border_style);
    Some(river_border_from_box_edges(border))
}

fn river_rgb_component_from_u8(component: u8) -> u32 {
    if component == 0 {
        0
    } else {
        (u32::from(component) << 16) | 0x0000_ffff
    }
}

fn river_alpha_component_from_u8(alpha: u8) -> u32 {
    u32::from(alpha) * 0x0101_0101
}

fn apply_opacity(color: ColorValue, opacity: f32) -> ColorValue {
    let clamped = opacity.clamp(0.0, 1.0);
    ColorValue {
        alpha: ((f32::from(color.alpha) * clamped).round() as u16).min(255) as u8,
        ..color
    }
}

fn river_color_to_color_value(red: u32, green: u32, blue: u32, alpha: u32) -> ColorValue {
    ColorValue {
        red: ((red >> 16) & 0xff) as u8,
        green: ((green >> 16) & 0xff) as u8,
        blue: ((blue >> 16) & 0xff) as u8,
        alpha: ((alpha >> 24) & 0xff) as u8,
    }
}

fn river_border_color_from_color(color: ColorValue) -> (u32, u32, u32, u32) {
    let alpha = u32::from(color.alpha);
    let premultiply = |component: u8| -> u8 {
        ((u32::from(component) * alpha + 127) / 255) as u8
    };

    (
        river_rgb_component_from_u8(premultiply(color.red)),
        river_rgb_component_from_u8(premultiply(color.green)),
        river_rgb_component_from_u8(premultiply(color.blue)),
        river_alpha_component_from_u8(color.alpha),
    )
}

fn river_border_color_from_layout_node(node: &LayoutSnapshotNode) -> Option<(u32, u32, u32, u32)> {
    node.styles()?
        .layout
        .border_color
        .map(river_border_color_from_color)
}

fn titlebar_height_to_px(style: Option<&ComputedStyle>) -> i32 {
    match style.and_then(|style| style.height) {
        Some(SizeValue::LengthPercentage(LengthPercentage::Px(value)))
        | Some(SizeValue::LengthPercentage(LengthPercentage::Percent(value))) => {
            value.round() as i32
        }
        _ => 28,
    }
    .max(1)
}

fn default_titlebar_background(focused: bool) -> ColorValue {
    if focused {
        ColorValue {
            red: 26,
            green: 48,
            blue: 78,
            alpha: 230,
        }
    } else {
        ColorValue {
            red: 28,
            green: 30,
            blue: 38,
            alpha: 220,
        }
    }
}

fn titlebar_background(style: Option<&ComputedStyle>, focused: bool) -> ColorValue {
    style
        .and_then(|style| style.background)
        .unwrap_or_else(|| default_titlebar_background(focused))
}

fn default_titlebar_text_color(focused: bool) -> ColorValue {
    if focused {
        ColorValue {
            red: 235,
            green: 240,
            blue: 248,
            alpha: 255,
        }
    } else {
        ColorValue {
            red: 208,
            green: 214,
            blue: 222,
            alpha: 255,
        }
    }
}

fn titlebar_text_color(style: Option<&ComputedStyle>, focused: bool) -> ColorValue {
    style
        .and_then(|style| style.color)
        .unwrap_or_else(|| default_titlebar_text_color(focused))
}

fn titlebar_text_align(style: Option<&ComputedStyle>) -> TextAlignValue {
    style
        .and_then(|style| style.text_align)
        .unwrap_or(TextAlignValue::Left)
}

fn titlebar_font_weight(style: Option<&ComputedStyle>) -> FontWeightValue {
    style
        .and_then(|style| style.font_weight)
        .unwrap_or(FontWeightValue::Normal)
}

fn titlebar_font_size(style: Option<&ComputedStyle>) -> i32 {
    match style.and_then(|style| style.font_size) {
        Some(LengthPercentage::Px(value)) | Some(LengthPercentage::Percent(value)) => {
            value.round() as i32
        }
        None => 14,
    }
    .clamp(8, 48)
}

fn titlebar_letter_spacing(style: Option<&ComputedStyle>) -> i32 {
    style
        .and_then(|style| style.letter_spacing)
        .unwrap_or(0.0)
        .round() as i32
}

fn style_opacity(style: Option<&ComputedStyle>) -> f32 {
    style
        .and_then(|style| style.opacity)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
}

fn titlebar_padding(style: Option<&ComputedStyle>) -> (i32, i32, i32, i32) {
    let Some(padding) = style.and_then(|style| style.padding) else {
        return (0, 0, 0, 0);
    };

    (
        border_length_to_px(padding.top),
        border_length_to_px(padding.right),
        border_length_to_px(padding.bottom),
        border_length_to_px(padding.left),
    )
}

fn parse_radius_px(raw: &str) -> Option<i32> {
    match raw.trim() {
        "0" | "0.0" => Some(0),
        value => value
            .strip_suffix("px")
            .and_then(|number| number.parse::<f32>().ok())
            .map(|value| value.round() as i32)
            .map(|value| value.max(0)),
    }
}

fn titlebar_corner_radii(
    titlebar_style: Option<&ComputedStyle>,
    window_style: Option<&ComputedStyle>,
) -> (i32, i32) {
    let raw = titlebar_style
        .and_then(|style| style.border_radius.as_deref())
        .or_else(|| window_style.and_then(|style| style.border_radius.as_deref()));
    let Some(raw) = raw else {
        return (0, 0);
    };

    let horizontal = raw.split('/').next().unwrap_or(raw);
    let values = horizontal
        .split_whitespace()
        .filter_map(parse_radius_px)
        .collect::<Vec<_>>();

    match values.as_slice() {
        [single] => (*single, *single),
        [top_left, top_right] => (*top_left, *top_right),
        [top_left, top_right, ..] => (*top_left, *top_right),
        _ => (0, 0),
    }
}

fn titlebar_text(window: &crate::model::WindowState) -> String {
    window
        .title
        .as_ref()
        .filter(|title| !title.trim().is_empty())
        .cloned()
        .or_else(|| {
            window
                .app_id
                .as_ref()
                .filter(|app_id| !app_id.trim().is_empty())
                .cloned()
        })
        .unwrap_or_default()
}

fn apply_titlebar_text_transform(style: Option<&ComputedStyle>, text: String) -> String {
    match style
        .and_then(|style| style.text_transform)
        .unwrap_or(TextTransformValue::None)
    {
        TextTransformValue::None => text,
        TextTransformValue::Uppercase => text.to_uppercase(),
        TextTransformValue::Lowercase => text.to_lowercase(),
        TextTransformValue::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut at_word_start = true;

            for character in text.chars() {
                if at_word_start && character.is_alphanumeric() {
                    result.extend(character.to_uppercase());
                    at_word_start = false;
                } else {
                    result.push(character);
                    if !character.is_alphanumeric() {
                        at_word_start = true;
                    }
                }
            }

            result
        }
    }
}

fn titlebar_bottom_border_width(style: Option<&ComputedStyle>) -> i32 {
    if matches!(
        style
            .and_then(|style| style.border_style)
            .map(|border| border.bottom),
        Some(BorderStyleValue::None)
    ) {
        return 0;
    }

    style
        .and_then(|style| style.border)
        .map(|border| border_length_to_px(border.bottom))
        .unwrap_or(0)
}

fn titlebar_bottom_border_color(
    style: Option<&ComputedStyle>,
    background: ColorValue,
) -> ColorValue {
    if let Some(color) = style
        .and_then(|style| style.border_side_colors)
        .and_then(|colors| colors.bottom)
    {
        return color;
    }

    style
        .and_then(|style| style.border_color)
        .unwrap_or(background)
}

fn decoration_mode_for_window(
    appearance: AppearanceValue,
    has_titlebar_style: bool,
    supports_compositor_titlebar: bool,
    is_fullscreen: bool,
) -> DecorationMode {
    if is_fullscreen {
        return DecorationMode::NoTitlebar;
    }

    match appearance {
        AppearanceValue::Auto if has_titlebar_style && supports_compositor_titlebar => {
            DecorationMode::CompositorTitlebar
        }
        AppearanceValue::Auto => DecorationMode::ClientSide,
        AppearanceValue::None => DecorationMode::NoTitlebar,
    }
}

impl RiverBackendState {
    pub(super) fn plan_tiled_manage_layout(&mut self) -> Vec<ManageWindowPlan> {
        let active_window_ids = self.active_workspace_window_ids();
        if active_window_ids.is_empty() {
            return Vec::new();
        }

        let active_state_ids = active_tiled_window_ids(
            &self.runtime_state,
            &active_window_ids
                .iter()
                .filter_map(|window_id| {
                    self.registry
                        .windows
                        .get(window_id)
                        .map(|window| window.state_id.clone())
                })
                .collect::<Vec<_>>(),
        );
        if active_state_ids.is_empty() {
            return Vec::new();
        }
        let tiled_edges = compute_horizontal_tiled_edges(&active_state_ids);

        if let Some(snapshot) = compute_layout_snapshot(
            &mut self.layout_service,
            &mut self.scene_cache,
            &self.config,
            &self.runtime_state,
            &active_state_ids,
        ) {
            return tiled_edges
                .into_iter()
                .filter_map(|edges| {
                    snapshot
                        .find_by_window_id(&edges.window_id)
                        .map(|node| ManageWindowPlan {
                            window_id: edges.window_id,
                            width: node.rect().width.round() as i32,
                            height: node.rect().height.round() as i32,
                            tiled_edges: edges.tiled_edges,
                        })
                })
                .collect();
        }

        let (_, origin_y, total_width, total_height) = self.current_output_geometry();
        compute_horizontal_tiles(&active_state_ids, 0, origin_y, total_width, total_height)
            .into_iter()
            .map(|tile| ManageWindowPlan {
                window_id: tile.window_id,
                width: tile.width,
                height: tile.height,
                tiled_edges: tile.tiled_edges,
            })
            .collect()
    }

    pub(super) fn plan_tiled_render_layout(&mut self) -> Vec<RenderWindowPlan> {
        let active_window_ids = self.active_workspace_window_ids();
        if active_window_ids.is_empty() {
            return Vec::new();
        }

        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();
        let active_state_ids = active_tiled_window_ids(
            &self.runtime_state,
            &active_window_ids
                .iter()
                .filter_map(|window_id| {
                    self.registry
                        .windows
                        .get(window_id)
                        .map(|window| window.state_id.clone())
                })
                .collect::<Vec<_>>(),
        );
        if active_state_ids.is_empty() {
            return Vec::new();
        }

        if let Some(snapshot) = compute_layout_snapshot(
            &mut self.layout_service,
            &mut self.scene_cache,
            &self.config,
            &self.runtime_state,
            &active_state_ids,
        ) {
            return active_state_ids
                .into_iter()
                .filter_map(|window_id| {
                    snapshot.find_by_window_id(&window_id).map(|node| {
                        let rect = node.rect();
                        RenderWindowPlan {
                            window_id,
                            x: rect.x.round() as i32,
                            y: rect.y.round() as i32,
                            width: rect.width.round() as i32,
                            height: rect.height.round() as i32,
                        }
                    })
                })
                .collect();
        }

        compute_horizontal_tiles(
            &active_state_ids,
            origin_x,
            origin_y,
            total_width,
            total_height,
        )
        .into_iter()
        .map(|tile| RenderWindowPlan {
            window_id: tile.window_id,
            x: tile.x,
            y: tile.y,
            width: tile.width,
            height: tile.height,
        })
        .collect()
    }

    pub(super) fn plan_window_borders(&mut self) -> Vec<BorderPlan> {
        let all_edges = river_window_v1::Edges::Top
            | river_window_v1::Edges::Bottom
            | river_window_v1::Edges::Left
            | river_window_v1::Edges::Right;
        let active_workspace_window_ids = self.active_workspace_window_state_ids();
        let active_tiled_state_ids = active_tiled_window_ids(&self.runtime_state, &active_workspace_window_ids);
        let snapshot = if active_tiled_state_ids.is_empty() {
            None
        } else {
            compute_layout_snapshot(
                &mut self.layout_service,
                &mut self.scene_cache,
                &self.config,
                &self.runtime_state,
                &active_tiled_state_ids,
            )
        };

        compute_window_borders(&self.runtime_state, &active_workspace_window_ids)
            .into_iter()
            .map(|border| {
                let default_edges = if border.width > 0 {
                    all_edges
                } else {
                    river_window_v1::Edges::None
                };
                let mut plan = BorderPlan {
                    window_id: border.window_id.clone(),
                    width: border.width,
                    edges: default_edges,
                    red: border.red,
                    green: border.green,
                    blue: border.blue,
                    alpha: border.alpha,
                };

                if let Some(snapshot) = snapshot.as_ref()
                    && let Some(node) = snapshot.find_by_window_id(&border.window_id)
                    && let Some((edges, width)) = river_border_from_layout_node(node)
                {
                    plan.edges = edges;
                    plan.width = width;
                }

                if let Some(snapshot) = snapshot.as_ref()
                    && let Some(node) = snapshot.find_by_window_id(&border.window_id)
                    && let Some((red, green, blue, alpha)) = river_border_color_from_layout_node(node)
                {
                    plan.red = red;
                    plan.green = green;
                    plan.blue = blue;
                    plan.alpha = alpha;
                }

                if let Some(snapshot) = snapshot.as_ref()
                    && let Some(node) = snapshot.find_by_window_id(&border.window_id)
                    && let Some(opacity) = node.styles().and_then(|styles| styles.layout.opacity)
                {
                    let color = apply_opacity(
                        river_color_to_color_value(plan.red, plan.green, plan.blue, plan.alpha),
                        opacity,
                    );
                    (plan.red, plan.green, plan.blue, plan.alpha) =
                        river_border_color_from_color(color);
                }

                plan
            })
            .collect()
    }

    pub(super) fn plan_window_appearance(&mut self) -> Vec<AppearancePlan> {
        let active_window_ids = self.active_workspace_window_state_ids();
        if active_window_ids.is_empty() {
            return Vec::new();
        }

        let Some(snapshot) = compute_layout_snapshot(
            &mut self.layout_service,
            &mut self.scene_cache,
            &self.config,
            &self.runtime_state,
            &active_window_ids,
        ) else {
            return Vec::new();
        };

        active_window_ids
            .into_iter()
            .filter_map(|window_id| {
                let node = snapshot.find_by_window_id(&window_id)?;
                let object_id = self.window_object_id(&window_id)?;
                let window = self.registry.windows.get(&object_id)?;
                let window_state = self.runtime_state.windows.get(&window_id)?;
                let appearance = node
                    .styles()
                    .and_then(|styles| styles.layout.appearance)
                    .unwrap_or(AppearanceValue::Auto);
                let has_titlebar_style = node
                    .styles()
                    .and_then(|styles| styles.titlebar.as_ref())
                    .is_some();
                let supports_compositor_titlebar = self.compositor.is_some()
                    && self.shm.is_some()
                    && window.supports_ssd;
                let decoration_mode = decoration_mode_for_window(
                    appearance,
                    has_titlebar_style,
                    supports_compositor_titlebar,
                    matches!(window_state.mode, WindowMode::Fullscreen),
                );

                Some(AppearancePlan {
                    window_id,
                    decoration_mode,
                })
            })
            .collect()
    }

    pub(super) fn plan_window_titlebars(&mut self) -> Vec<TitlebarPlan> {
        let appearance = self
            .plan_window_appearance()
            .into_iter()
            .filter(|plan| matches!(plan.decoration_mode, DecorationMode::CompositorTitlebar))
            .map(|plan| plan.window_id)
            .collect::<Vec<_>>();
        if appearance.is_empty() {
            return Vec::new();
        }

        let Some(snapshot) = compute_layout_snapshot(
            &mut self.layout_service,
            &mut self.scene_cache,
            &self.config,
            &self.runtime_state,
            &appearance,
        ) else {
            return Vec::new();
        };

        appearance
            .into_iter()
            .filter_map(|window_id| {
                let node = snapshot.find_by_window_id(&window_id)?;
                let titlebar_style = node.styles().and_then(|styles| styles.titlebar.as_ref());
                let window_style = node.styles().map(|styles| &styles.layout);
                let focused = self.runtime_state.focused_window_id.as_ref() == Some(&window_id);
                let window = self.runtime_state.windows.get(&window_id)?;
                let opacity = style_opacity(titlebar_style);
                let background = apply_opacity(titlebar_background(titlebar_style, focused), opacity);
                let text_color = apply_opacity(titlebar_text_color(titlebar_style, focused), opacity);
                let text_align = titlebar_text_align(titlebar_style);
                let font_size = titlebar_font_size(titlebar_style);
                let font_weight = titlebar_font_weight(titlebar_style);
                let letter_spacing = titlebar_letter_spacing(titlebar_style);
                let border_bottom_color = apply_opacity(
                    titlebar_bottom_border_color(titlebar_style, background),
                    opacity,
                );
                let (padding_top, padding_right, padding_bottom, padding_left) =
                    titlebar_padding(titlebar_style);
                let (corner_radius_top_left, corner_radius_top_right) =
                    titlebar_corner_radii(titlebar_style, window_style);
                let title = apply_titlebar_text_transform(titlebar_style, titlebar_text(window));
                Some(TitlebarPlan {
                    window_id,
                    height: titlebar_height_to_px(titlebar_style),
                    background,
                    border_bottom_width: titlebar_bottom_border_width(titlebar_style),
                    border_bottom_color,
                    title,
                    text_color,
                    text_align,
                    font_size,
                    font_weight,
                    letter_spacing,
                    padding_top,
                    padding_right,
                    padding_bottom,
                    padding_left,
                    corner_radius_top_left,
                    corner_radius_top_right,
                })
            })
            .collect()
    }

    pub(super) fn plan_window_mode_updates(&self) -> Vec<WindowModePlan> {
        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();

        self.active_workspace_window_state_ids()
            .into_iter()
            .filter_map(|window_id| {
                let window = self.runtime_state.windows.get(&window_id)?;
                let mode = configured_mode_for_window(&self.config, window)?;
                let (x, y, width, height) = match &mode {
                    WindowMode::Floating { rect } => {
                        let rect = rect.unwrap_or(LayoutRect {
                            x: origin_x as f32 + (total_width as f32 * 0.1),
                            y: origin_y as f32 + (total_height as f32 * 0.1),
                            width: (total_width as f32 * 0.8).max(1.0),
                            height: (total_height as f32 * 0.8).max(1.0),
                        });
                        (
                            rect.x.round() as i32,
                            rect.y.round() as i32,
                            rect.width.round() as i32,
                            rect.height.round() as i32,
                        )
                    }
                    WindowMode::Fullscreen => {
                        (origin_x, origin_y, total_width.max(1), total_height.max(1))
                    }
                    WindowMode::Tiled => return None,
                };

                Some(WindowModePlan {
                    window_id,
                    mode,
                    x,
                    y,
                    width,
                    height,
                })
            })
            .collect()
    }

    pub(super) fn plan_toggle_floating_command(
        &self,
        seat_id: &ObjectId,
    ) -> Option<WindowModePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let window = self.runtime_state.windows.get(&window_id)?;
        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();

        let mode = match &window.mode {
            WindowMode::Floating { .. } => {
                WindowMode::Tiled
            }
            WindowMode::Tiled | WindowMode::Fullscreen => {
                WindowMode::Floating {
                    rect: Some(window.last_floating_rect.unwrap_or(
                        LayoutRect {
                            x: origin_x as f32 + (total_width as f32 * 0.1),
                            y: origin_y as f32 + (total_height as f32 * 0.1),
                            width: (total_width as f32 * 0.8).max(1.0),
                            height: (total_height as f32 * 0.8).max(1.0),
                        },
                    )),
                }
            }
        };

        let (x, y, width, height) = match &mode {
            WindowMode::Tiled => (
                window.x,
                window.y,
                window.width.max(1),
                window.height.max(1),
            ),
            WindowMode::Floating { rect } => {
                let rect = rect.unwrap();
                (
                    rect.x.round() as i32,
                    rect.y.round() as i32,
                    rect.width.round() as i32,
                    rect.height.round() as i32,
                )
            }
            WindowMode::Fullscreen => {
                (origin_x, origin_y, total_width.max(1), total_height.max(1))
            }
        };

        Some(WindowModePlan {
            window_id,
            mode,
            x,
            y,
            width,
            height,
        })
    }

    pub(super) fn plan_toggle_fullscreen_command(
        &self,
        seat_id: &ObjectId,
    ) -> Option<WindowModePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let window = self.runtime_state.windows.get(&window_id)?;
        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();

        let mode = match &window.mode {
            WindowMode::Fullscreen => {
                if let Some(rect) = window.last_floating_rect {
                    WindowMode::Floating { rect: Some(rect) }
                } else {
                    WindowMode::Tiled
                }
            }
            WindowMode::Tiled
            | WindowMode::Floating { .. } => {
                WindowMode::Fullscreen
            }
        };

        let (x, y, width, height) = match &mode {
            WindowMode::Fullscreen => {
                (origin_x, origin_y, total_width.max(1), total_height.max(1))
            }
            WindowMode::Tiled => (
                window.x,
                window.y,
                window.width.max(1),
                window.height.max(1),
            ),
            WindowMode::Floating { rect } => {
                let rect = rect.as_ref()?;
                (
                    rect.x.round() as i32,
                    rect.y.round() as i32,
                    rect.width.round() as i32,
                    rect.height.round() as i32,
                )
            }
        };

        Some(WindowModePlan {
            window_id,
            mode,
            x,
            y,
            width,
            height,
        })
    }

    pub(super) fn plan_focus_for_seat(&self, _seat_id: &ObjectId) -> FocusPlan {
        let top_window_id = top_window_id(&self.active_workspace_window_state_ids());

        match top_window_id {
            Some(window_id) => FocusPlan::FocusWindow { window_id },
            None => FocusPlan::ClearFocus,
        }
    }

    pub(super) fn plan_close_focused_window(&self, seat_id: &ObjectId) -> Option<CloseWindowPlan> {
        self.seat_focused_state_window_id(seat_id)
            .map(|window_id| CloseWindowPlan { window_id })
    }

    pub(super) fn plan_activate_workspace_command(
        &self,
        workspace_id: spiders_tree::WorkspaceId,
    ) -> ActivateWorkspacePlan {
        ActivateWorkspacePlan {
            workspace_id,
            focus: FocusPlan::ClearFocus,
        }
    }

    pub(super) fn plan_move_focused_window_to_workspace_command(
        &self,
        seat_id: &ObjectId,
        workspace_id: spiders_tree::WorkspaceId,
    ) -> Option<MoveFocusedWindowToWorkspacePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let focus = self.plan_focus_for_seat(seat_id);

        Some(MoveFocusedWindowToWorkspacePlan {
            window_id,
            workspace_id,
            focus,
        })
    }

    pub(super) fn plan_move_direction_command(
        &self,
        seat_id: &ObjectId,
        direction: FocusDirection,
    ) -> Option<MoveWindowInWorkspacePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let active_window_ids = self.active_workspace_window_state_ids();
        let target_window_id = directional_neighbor_window_id(
            &self.runtime_state,
            &active_window_ids,
            &window_id,
            direction,
        )?;

        Some(MoveWindowInWorkspacePlan {
            window_id: window_id.clone(),
            target_window_id,
            focus: FocusPlan::FocusWindow { window_id },
        })
    }

    pub(super) fn plan_focus_window_command(
        &self,
        window_id: spiders_tree::WindowId,
    ) -> Option<(MoveWindowToTopPlan, FocusPlan)> {
        self.window_object_id(&window_id)?;
        Some((
            MoveWindowToTopPlan {
                window_id: window_id.clone(),
            },
            FocusPlan::FocusWindow { window_id },
        ))
    }

    pub(super) fn plan_focus_direction_command(
        &self,
        seat_id: &ObjectId,
        direction: FocusDirection,
    ) -> Option<(MoveWindowToTopPlan, FocusPlan)> {
        let active_state_ids = self.active_workspace_window_state_ids();
        if active_state_ids.len() <= 1 {
            return None;
        }

        let focused_state_id = self
            .seat_focused_state_window_id(seat_id)
            .or_else(|| active_state_ids.last().cloned());

        let target_state_id = focus_target_in_direction(
            &self.runtime_state,
            &active_state_ids,
            direction,
            focused_state_id.as_ref(),
        )?;

        Some((
            MoveWindowToTopPlan {
                window_id: target_state_id.clone(),
            },
            FocusPlan::FocusWindow {
                window_id: target_state_id,
            },
        ))
    }

    pub(super) fn plan_pointer_render_ops(&self) -> Vec<PointerRenderPlan> {
        compute_pointer_render_positions(&self.runtime_state)
            .into_iter()
            .map(|position| PointerRenderPlan {
                window_id: position.window_id,
                x: position.x,
                y: position.y,
            })
            .collect()
    }

    pub(super) fn plan_inactive_tiled_windows(&self) -> Vec<ClearTiledStatePlan> {
        inactive_window_ids(
            &self.active_workspace_window_state_ids(),
            &self
                .runtime_state
                .window_stack
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .into_iter()
        .map(|window_id| ClearTiledStatePlan { window_id })
        .collect()
    }

    pub(super) fn plan_offscreen_windows(&self) -> Vec<OffscreenWindowPlan> {
        inactive_window_ids(
            &self.active_workspace_window_state_ids(),
            &self
                .runtime_state
                .window_stack
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .into_iter()
        .map(|window_id| OffscreenWindowPlan {
            window_id,
            x: -20_000,
            y: -20_000,
        })
        .collect()
    }

    pub(super) fn apply_tiled_manage_layout(&mut self) {
        let clear_plan = self.plan_inactive_tiled_windows();
        self.apply_clear_tiled_state_plan(&clear_plan);

        if !self.active_workspace_window_ids().is_empty() {
            let plan = self.plan_tiled_manage_layout();
            self.apply_manage_window_plan(&plan);
        }
    }

    pub(super) fn apply_tiled_render_layout(&mut self) {
        let offscreen_plan = self.plan_offscreen_windows();
        self.apply_offscreen_window_plan(&offscreen_plan);

        if !self.active_workspace_window_ids().is_empty() {
            let plan = self.plan_tiled_render_layout();
            self.apply_render_window_plan(&plan);
        }
    }

    pub(super) fn apply_window_borders(&mut self) {
        let plan = self.plan_window_borders();
        self.apply_border_plan(&plan);
    }

    pub(super) fn apply_window_appearance(&mut self) {
        let plan = self.plan_window_appearance();
        self.apply_appearance_plan(&plan);
    }

    pub(super) fn apply_window_titlebars(&mut self) {
        let plan = self.plan_window_titlebars();
        self.apply_titlebar_plan(&plan);
    }

    pub(super) fn has_active_pointer_op(&self) -> bool {
        self.runtime_state
            .seats
            .values()
            .any(|seat| !matches!(seat.pointer_op, SeatPointerOpState::None))
    }

    pub(super) fn focus_top_window_for_seat(&mut self, seat_id: &ObjectId) {
        let plan = self.plan_focus_for_seat(seat_id);
        self.apply_focus_plan(seat_id, &plan);
    }

    pub(super) fn plan_command(&self, seat_id: &ObjectId, command: RiverCommand) -> CommandPlan {
        match command {
            RiverCommand::Spawn { command } => CommandPlan::Spawn { command },
            RiverCommand::ActivateWorkspace { workspace_id } => {
                CommandPlan::ActivateWorkspace(self.plan_activate_workspace_command(workspace_id))
            }
            RiverCommand::AssignFocusedWindowToWorkspace { workspace_id } => self
                .plan_move_focused_window_to_workspace_command(seat_id, workspace_id)
                .map(CommandPlan::MoveFocusedWindowToWorkspace)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::MoveDirection { direction } => self
                .plan_move_direction_command(seat_id, direction)
                .map(CommandPlan::MoveWindowInWorkspace)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::ToggleFloating => self
                .plan_toggle_floating_command(seat_id)
                .map(CommandPlan::SetWindowMode)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::ToggleFullscreen => self
                .plan_toggle_fullscreen_command(seat_id)
                .map(CommandPlan::SetWindowMode)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::FocusOutput { output_id } => CommandPlan::FocusOutput { output_id },
            RiverCommand::FocusWindow { window_id } => self
                .plan_focus_window_command(window_id)
                .map(|(stack, focus)| CommandPlan::FocusWindow { stack, focus })
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::CloseFocusedWindow => CommandPlan::CloseFocusedWindow,
            RiverCommand::FocusDirection { direction } => self
                .plan_focus_direction_command(seat_id, direction)
                .map(|(stack, focus)| CommandPlan::FocusDirection { stack, focus })
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::ReloadConfig
            | RiverCommand::SetLayout { .. }
            | RiverCommand::CycleLayoutNext
            | RiverCommand::CycleLayoutPrevious
            | RiverCommand::SetFloatingWindowGeometry { .. }
            | RiverCommand::Unsupported { .. } => CommandPlan::Noop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_scene::{AppearanceValue, ComputedStyle, SceneNodeStyle};
    use spiders_tree::{LayoutNodeMeta, LayoutRect, WindowId};

    #[test]
    fn river_border_from_box_edges_maps_nonzero_edges_and_uses_max_width() {
        let (edges, width) = river_border_from_box_edges(BoxEdges {
            top: LengthPercentage::Px(1.0),
            right: LengthPercentage::Px(0.0),
            bottom: LengthPercentage::Px(2.0),
            left: LengthPercentage::Px(3.0),
        });

        assert_eq!(width, 3);
        assert_eq!(
            edges,
            river_window_v1::Edges::Top
                | river_window_v1::Edges::Bottom
                | river_window_v1::Edges::Left
        );
    }

    #[test]
    fn river_border_from_layout_node_reads_scene_border_style() {
        let node = LayoutSnapshotNode::Window {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            styles: Some(SceneNodeStyle {
                layout: ComputedStyle {
                    border: Some(BoxEdges {
                        top: LengthPercentage::Px(0.0),
                        right: LengthPercentage::Px(4.0),
                        bottom: LengthPercentage::Px(4.0),
                        left: LengthPercentage::Px(0.0),
                    }),
                    ..ComputedStyle::default()
                },
                titlebar: None,
            }),
            window_id: Some(WindowId::from("w1")),
        };

        assert_eq!(
            river_border_from_layout_node(&node),
            Some((
                river_window_v1::Edges::Right | river_window_v1::Edges::Bottom,
                4,
            ))
        );
    }

    #[test]
    fn river_border_from_layout_node_honors_border_style_none_edges() {
        let node = LayoutSnapshotNode::Window {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            styles: Some(SceneNodeStyle {
                layout: ComputedStyle {
                    border: Some(BoxEdges {
                        top: LengthPercentage::Px(5.0),
                        right: LengthPercentage::Px(4.0),
                        bottom: LengthPercentage::Px(3.0),
                        left: LengthPercentage::Px(2.0),
                    }),
                    border_style: Some(BoxEdges {
                        top: BorderStyleValue::None,
                        right: BorderStyleValue::Solid,
                        bottom: BorderStyleValue::None,
                        left: BorderStyleValue::Solid,
                    }),
                    ..ComputedStyle::default()
                },
                titlebar: None,
            }),
            window_id: Some(WindowId::from("w1")),
        };

        assert_eq!(
            river_border_from_layout_node(&node),
            Some((
                river_window_v1::Edges::Right | river_window_v1::Edges::Left,
                4,
            ))
        );
    }

    #[test]
    fn river_border_color_from_layout_node_reads_scene_border_color() {
        let node = LayoutSnapshotNode::Window {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            styles: Some(SceneNodeStyle {
                layout: ComputedStyle {
                    border_color: Some(ColorValue {
                        red: 40,
                        green: 85,
                        blue: 119,
                        alpha: 255,
                    }),
                    ..ComputedStyle::default()
                },
                titlebar: None,
            }),
            window_id: Some(WindowId::from("w1")),
        };

        assert_eq!(
            river_border_color_from_layout_node(&node),
            Some((0x0028_ffff, 0x0055_ffff, 0x0077_ffff, 0xffff_ffff))
        );
    }

    #[test]
    fn river_border_color_premultiplies_alpha() {
        assert_eq!(
            river_border_color_from_color(ColorValue {
                red: 255,
                green: 128,
                blue: 0,
                alpha: 128,
            }),
            (0x0080_ffff, 0x0040_ffff, 0, 0x8080_8080)
        );
    }

    #[test]
    fn titlebar_bottom_border_helpers_read_style_values() {
        let style = ComputedStyle {
            border: Some(BoxEdges {
                top: LengthPercentage::Px(0.0),
                right: LengthPercentage::Px(0.0),
                bottom: LengthPercentage::Px(3.0),
                left: LengthPercentage::Px(0.0),
            }),
            border_style: Some(BoxEdges {
                top: BorderStyleValue::None,
                right: BorderStyleValue::None,
                bottom: BorderStyleValue::Solid,
                left: BorderStyleValue::None,
            }),
            border_color: Some(ColorValue {
                red: 12,
                green: 18,
                blue: 24,
                alpha: 255,
            }),
            border_side_colors: Some(BoxEdges {
                top: None,
                right: None,
                bottom: Some(ColorValue {
                    red: 21,
                    green: 22,
                    blue: 23,
                    alpha: 255,
                }),
                left: None,
            }),
            ..ComputedStyle::default()
        };

        assert_eq!(titlebar_bottom_border_width(Some(&style)), 3);
        assert_eq!(
            titlebar_bottom_border_color(
                Some(&style),
                ColorValue {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 4,
                }
            ),
            ColorValue {
                red: 21,
                green: 22,
                blue: 23,
                alpha: 255,
            }
        );
    }

    #[test]
    fn titlebar_bottom_border_color_falls_back_to_background() {
        let background = ColorValue {
            red: 33,
            green: 44,
            blue: 55,
            alpha: 200,
        };

        assert_eq!(titlebar_bottom_border_width(None), 0);
        assert_eq!(titlebar_bottom_border_color(None, background), background);
    }

    #[test]
    fn titlebar_text_prefers_window_title_then_app_id() {
        let mut window = crate::model::WindowState {
            id: WindowId::from("w1"),
            app_id: Some("foot".into()),
            title: Some("terminal".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            identifier: None,
            unreliable_pid: None,
            output_id: None,
            workspace_ids: Vec::new(),
            is_new: false,
            closed: false,
            mapped: true,
            mode: WindowMode::Tiled,
            focused: false,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            last_floating_rect: None,
        };

        assert_eq!(titlebar_text(&window), "terminal");
        window.title = Some("   ".into());
        assert_eq!(titlebar_text(&window), "foot");
    }

    #[test]
    fn titlebar_padding_converts_lengths_to_pixels() {
        let style = ComputedStyle {
            padding: Some(BoxEdges {
                top: LengthPercentage::Px(3.0),
                right: LengthPercentage::Px(4.0),
                bottom: LengthPercentage::Px(5.0),
                left: LengthPercentage::Px(6.0),
            }),
            ..ComputedStyle::default()
        };

        assert_eq!(titlebar_padding(Some(&style)), (3, 4, 5, 6));
    }

    #[test]
    fn titlebar_text_transform_applies_supported_modes() {
        let uppercase = ComputedStyle {
            text_transform: Some(TextTransformValue::Uppercase),
            ..ComputedStyle::default()
        };
        let capitalize = ComputedStyle {
            text_transform: Some(TextTransformValue::Capitalize),
            ..ComputedStyle::default()
        };

        assert_eq!(
            apply_titlebar_text_transform(Some(&uppercase), "Term 42".into()),
            "TERM 42"
        );
        assert_eq!(
            apply_titlebar_text_transform(Some(&capitalize), "hello river world".into()),
            "Hello River World"
        );
    }

    #[test]
    fn titlebar_text_align_defaults_left() {
        assert_eq!(titlebar_text_align(None), TextAlignValue::Left);
        assert_eq!(
            titlebar_text_align(Some(&ComputedStyle {
                text_align: Some(TextAlignValue::End),
                ..ComputedStyle::default()
            })),
            TextAlignValue::End
        );
    }

    #[test]
    fn titlebar_typography_defaults_and_conversions() {
        assert_eq!(titlebar_font_size(None), 14);
        assert_eq!(titlebar_font_weight(None), FontWeightValue::Normal);
        assert_eq!(titlebar_letter_spacing(None), 0);
        assert_eq!(
            titlebar_font_size(Some(&ComputedStyle {
                font_size: Some(LengthPercentage::Px(17.0)),
                ..ComputedStyle::default()
            })),
            17
        );
        assert_eq!(
            titlebar_font_weight(Some(&ComputedStyle {
                font_weight: Some(FontWeightValue::Bold),
                ..ComputedStyle::default()
            })),
            FontWeightValue::Bold
        );
        assert_eq!(
            titlebar_letter_spacing(Some(&ComputedStyle {
                letter_spacing: Some(1.6),
                ..ComputedStyle::default()
            })),
            2
        );
    }

    #[test]
    fn apply_opacity_scales_alpha_only() {
        assert_eq!(
            apply_opacity(
                ColorValue {
                    red: 10,
                    green: 20,
                    blue: 30,
                    alpha: 200,
                },
                0.5,
            ),
            ColorValue {
                red: 10,
                green: 20,
                blue: 30,
                alpha: 100,
            }
        );
    }

    #[test]
    fn titlebar_corner_radii_uses_titlebar_then_window_radius() {
        let titlebar_style = ComputedStyle {
            border_radius: Some("12px 6px 0 0".into()),
            ..ComputedStyle::default()
        };
        let window_style = ComputedStyle {
            border_radius: Some("20px 18px".into()),
            ..ComputedStyle::default()
        };

        assert_eq!(titlebar_corner_radii(Some(&titlebar_style), Some(&window_style)), (12, 6));
        assert_eq!(titlebar_corner_radii(None, Some(&window_style)), (20, 18));
    }

    #[test]
    fn titlebar_bottom_border_width_honors_none_style() {
        let style = ComputedStyle {
            border: Some(BoxEdges {
                top: LengthPercentage::Px(0.0),
                right: LengthPercentage::Px(0.0),
                bottom: LengthPercentage::Px(4.0),
                left: LengthPercentage::Px(0.0),
            }),
            border_style: Some(BoxEdges {
                top: BorderStyleValue::None,
                right: BorderStyleValue::None,
                bottom: BorderStyleValue::None,
                left: BorderStyleValue::None,
            }),
            ..ComputedStyle::default()
        };

        assert_eq!(titlebar_bottom_border_width(Some(&style)), 0);
    }

    #[test]
    fn decoration_mode_requires_titlebar_style_for_auto_titlebars() {
        assert_eq!(
            decoration_mode_for_window(AppearanceValue::Auto, false, true, false),
            DecorationMode::ClientSide
        );
        assert_eq!(
            decoration_mode_for_window(AppearanceValue::Auto, true, true, false),
            DecorationMode::CompositorTitlebar
        );
        assert_eq!(
            decoration_mode_for_window(AppearanceValue::Auto, true, false, false),
            DecorationMode::ClientSide
        );
    }

    #[test]
    fn decoration_mode_keeps_none_as_no_titlebar() {
        assert_eq!(
            decoration_mode_for_window(AppearanceValue::None, true, true, false),
            DecorationMode::NoTitlebar
        );
        assert_eq!(
            decoration_mode_for_window(AppearanceValue::Auto, true, true, true),
            DecorationMode::NoTitlebar
        );
    }
}
