use serde::{Deserialize, Serialize};
use spiders_core::WindowId;
use spiders_css::{
    AppearanceValue, BorderStyleValue, BoxShadowValue, ColorValue, FontQuery, FontWeightValue,
    LengthPercentage, TextAlignValue, TextTransformValue,
};
use spiders_scene::ComputedStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecorationMode {
    ClientSide,
    CompositorTitlebar,
    NoTitlebar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearancePlan {
    pub window_id: WindowId,
    pub decoration_mode: DecorationMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarPlan {
    pub window_id: WindowId,
    pub height: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub background: ColorValue,
    pub border_bottom_width: i32,
    pub border_bottom_color: ColorValue,
    pub title: String,
    pub text_color: ColorValue,
    pub text_align: TextAlignValue,
    pub text_transform: TextTransformValue,
    pub font: FontQuery,
    pub letter_spacing: i32,
    pub box_shadow: Option<Vec<BoxShadowValue>>,
    pub padding_top: i32,
    pub padding_right: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub corner_radius_top_left: i32,
    pub corner_radius_top_right: i32,
    pub buttons: Vec<TitlebarButtonPlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TitlebarButtonKind {
    Close,
    ToggleFullscreen,
    ToggleFloating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonPlan {
    pub kind: TitlebarButtonKind,
    pub rect: TitlebarButtonRect,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonColors {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonsConfig {
    pub close: bool,
    pub fullscreen: bool,
    pub floating: bool,
}

impl Default for TitlebarButtonsConfig {
    fn default() -> Self {
        Self { close: true, fullscreen: true, floating: true }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TitlebarPlanInput {
    pub window_id: WindowId,
    pub title: String,
    pub focused: bool,
    pub titlebar_style: Option<ComputedStyle>,
    pub window_style: Option<ComputedStyle>,
    pub default_background_focused: ColorValue,
    pub default_background_unfocused: ColorValue,
    pub default_text_color_focused: ColorValue,
    pub default_text_color_unfocused: ColorValue,
    pub offset_x: i32,
    pub offset_y: i32,
    pub effective_opacity: f32,
    pub buttons: TitlebarButtonsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitlebarPlanPreset {
    pub height: i32,
    pub border_bottom_width: i32,
    pub border_bottom_color_focused: ColorValue,
    pub border_bottom_color_unfocused: ColorValue,
    pub font: FontQuery,
    pub font_unfocused_weight: FontWeightValue,
    pub padding_left: i32,
    pub padding_right: i32,
}

impl Default for TitlebarPlanPreset {
    fn default() -> Self {
        Self {
            height: 24,
            border_bottom_width: 1,
            border_bottom_color_focused: ColorValue { red: 93, green: 173, blue: 226, alpha: 190 },
            border_bottom_color_unfocused: ColorValue { red: 94, green: 99, blue: 118, alpha: 150 },
            font: FontQuery {
                families: vec![
                    spiders_css::FontFamilyName::SystemUi,
                    spiders_css::FontFamilyName::SansSerif,
                ],
                weight: FontWeightValue::Bold,
                size_px: 12,
            },
            font_unfocused_weight: FontWeightValue::Normal,
            padding_left: 8,
            padding_right: 8,
        }
    }
}

pub fn build_titlebar_plan(input: &TitlebarPlanInput) -> TitlebarPlan {
    let titlebar_style = input.titlebar_style.as_ref();
    let window_style = input.window_style.as_ref();
    let background = apply_opacity(
        titlebar_background(
            titlebar_style,
            input.focused,
            input.default_background_focused,
            input.default_background_unfocused,
        ),
        input.effective_opacity,
    );
    let text_color = apply_opacity(
        titlebar_text_color(
            titlebar_style,
            input.focused,
            input.default_text_color_focused,
            input.default_text_color_unfocused,
        ),
        input.effective_opacity,
    );
    let text_transform =
        titlebar_style.and_then(|style| style.text_transform).unwrap_or(TextTransformValue::None);

    TitlebarPlan {
        window_id: input.window_id.clone(),
        title: apply_titlebar_text_transform(text_transform, input.title.clone()),
        height: titlebar_height_to_px(titlebar_style),
        offset_x: input.offset_x,
        offset_y: input.offset_y,
        background,
        border_bottom_width: titlebar_bottom_border_width(titlebar_style),
        border_bottom_color: apply_opacity(
            titlebar_bottom_border_color(titlebar_style, background),
            input.effective_opacity,
        ),
        text_color,
        text_align: titlebar_text_align(titlebar_style),
        text_transform,
        font: titlebar_font_query(titlebar_style),
        letter_spacing: titlebar_letter_spacing(titlebar_style),
        box_shadow: titlebar_box_shadow(titlebar_style, window_style),
        padding_top: titlebar_padding(titlebar_style).0,
        padding_right: titlebar_padding(titlebar_style).1,
        padding_bottom: titlebar_padding(titlebar_style).2,
        padding_left: titlebar_padding(titlebar_style).3,
        corner_radius_top_left: titlebar_corner_radii(titlebar_style, window_style).0,
        corner_radius_top_right: titlebar_corner_radii(titlebar_style, window_style).1,
        buttons: build_default_titlebar_buttons(
            titlebar_height_to_px(titlebar_style),
            &input.buttons,
        ),
    }
}

pub fn apply_titlebar_plan_preset(
    mut plan: TitlebarPlan,
    focused: bool,
    preset: &TitlebarPlanPreset,
) -> TitlebarPlan {
    plan.height = preset.height;
    plan.border_bottom_width = preset.border_bottom_width;
    plan.border_bottom_color = if focused {
        preset.border_bottom_color_focused
    } else {
        preset.border_bottom_color_unfocused
    };
    plan.font = FontQuery {
        weight: if focused { preset.font.weight } else { preset.font_unfocused_weight },
        ..preset.font.clone()
    };
    plan.padding_left = preset.padding_left;
    plan.padding_right = preset.padding_right;
    plan.buttons = build_default_titlebar_buttons(plan.height, &TitlebarButtonsConfig::default());
    plan
}

pub fn build_titlebar_plan_with_preset(
    input: &TitlebarPlanInput,
    preset: &TitlebarPlanPreset,
) -> TitlebarPlan {
    apply_titlebar_plan_preset(build_titlebar_plan(input), input.focused, preset)
}

fn build_default_titlebar_buttons(
    height: i32,
    config: &TitlebarButtonsConfig,
) -> Vec<TitlebarButtonPlan> {
    let size = (height - 8).clamp(10, 18);
    let top = ((height - size) / 2).max(0);
    let gap = 6;
    let start_x = 8;
    let mut entries = Vec::new();
    if config.close {
        entries.push((TitlebarButtonKind::Close, "close"));
    }
    if config.fullscreen {
        entries.push((TitlebarButtonKind::ToggleFullscreen, "fullscreen"));
    }
    if config.floating {
        entries.push((TitlebarButtonKind::ToggleFloating, "floating"));
    }

    entries
        .into_iter()
        .enumerate()
        .map(|(index, (kind, label))| TitlebarButtonPlan {
            kind,
            rect: TitlebarButtonRect {
                x: start_x + index as i32 * (size + gap),
                y: top,
                width: size,
                height: size,
            },
            label: label.to_string(),
        })
        .collect()
}

pub fn decoration_mode_for_window(
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

pub fn titlebar_text_from_window(title: Option<&str>, app_id: Option<&str>) -> String {
    title
        .filter(|title| !title.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| app_id.filter(|app_id| !app_id.trim().is_empty()).map(str::to_owned))
        .unwrap_or_default()
}

pub fn titlebar_button_colors(kind: TitlebarButtonKind) -> TitlebarButtonColors {
    match kind {
        TitlebarButtonKind::Close => {
            TitlebarButtonColors { red: 235, green: 87, blue: 87, alpha: 220 }
        }
        TitlebarButtonKind::ToggleFullscreen => {
            TitlebarButtonColors { red: 242, green: 201, blue: 76, alpha: 220 }
        }
        TitlebarButtonKind::ToggleFloating => {
            TitlebarButtonColors { red: 111, green: 207, blue: 151, alpha: 220 }
        }
    }
}

pub fn titlebar_text_left_inset(plan: &TitlebarPlan) -> i32 {
    let buttons_right =
        plan.buttons.iter().map(|button| button.rect.x + button.rect.width).max().unwrap_or(0);

    if buttons_right > 0 { plan.padding_left.max(buttons_right + 8) } else { plan.padding_left }
}

pub fn titlebar_text_right_inset(plan: &TitlebarPlan, trailing_width: i32) -> i32 {
    if trailing_width > 0 { plan.padding_right.max(trailing_width + 8) } else { plan.padding_right }
}

fn titlebar_font_family(style: Option<&ComputedStyle>) -> Option<spiders_css::FontFamilyValue> {
    style
        .and_then(|style| style.font_family.as_ref())
        .cloned()
        .filter(|families| !families.is_empty())
}

fn titlebar_font_weight(style: Option<&ComputedStyle>) -> FontWeightValue {
    style.and_then(|style| style.font_weight).unwrap_or(FontWeightValue::Normal)
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

fn titlebar_font_query(style: Option<&ComputedStyle>) -> FontQuery {
    FontQuery {
        families: titlebar_font_family(style).unwrap_or_default(),
        weight: titlebar_font_weight(style),
        size_px: titlebar_font_size(style),
    }
}

fn titlebar_letter_spacing(style: Option<&ComputedStyle>) -> i32 {
    style.and_then(|style| style.letter_spacing).unwrap_or(0.0).round() as i32
}

fn titlebar_box_shadow(
    titlebar_style: Option<&ComputedStyle>,
    window_style: Option<&ComputedStyle>,
) -> Option<Vec<BoxShadowValue>> {
    titlebar_style
        .and_then(|style| style.box_shadow.as_ref())
        .or_else(|| window_style.and_then(|style| style.box_shadow.as_ref()))
        .cloned()
        .filter(|shadow| !shadow.is_empty())
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

fn titlebar_corner_radii(
    titlebar_style: Option<&ComputedStyle>,
    window_style: Option<&ComputedStyle>,
) -> (i32, i32) {
    let radius = titlebar_style
        .and_then(|style| style.border_radius)
        .or_else(|| window_style.and_then(|style| style.border_radius));
    let Some(radius) = radius else {
        return (0, 0);
    };

    (radius.top_left, radius.top_right)
}

fn apply_titlebar_text_transform(transform: TextTransformValue, text: String) -> String {
    match transform {
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
        style.and_then(|style| style.border_style).map(|border| border.bottom),
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
    if let Some(color) =
        style.and_then(|style| style.border_side_colors).and_then(|colors| colors.bottom)
    {
        return color;
    }

    style.and_then(|style| style.border_color).unwrap_or(background)
}

fn titlebar_background(
    style: Option<&ComputedStyle>,
    focused: bool,
    focused_default: ColorValue,
    unfocused_default: ColorValue,
) -> ColorValue {
    style.and_then(|style| style.background).unwrap_or(if focused {
        focused_default
    } else {
        unfocused_default
    })
}

fn titlebar_text_color(
    style: Option<&ComputedStyle>,
    focused: bool,
    focused_default: ColorValue,
    unfocused_default: ColorValue,
) -> ColorValue {
    style.and_then(|style| style.color).unwrap_or(if focused {
        focused_default
    } else {
        unfocused_default
    })
}

fn titlebar_text_align(style: Option<&ComputedStyle>) -> TextAlignValue {
    style.and_then(|style| style.text_align).unwrap_or(TextAlignValue::Left)
}

fn titlebar_height_to_px(style: Option<&ComputedStyle>) -> i32 {
    match style.and_then(|style| style.height) {
        Some(spiders_css::SizeValue::Auto) => 28,
        Some(spiders_css::SizeValue::LengthPercentage(LengthPercentage::Px(value)))
        | Some(spiders_css::SizeValue::LengthPercentage(LengthPercentage::Percent(value))) => {
            value.round() as i32
        }
        _ => 28,
    }
    .clamp(16, 64)
}

fn border_length_to_px(length: LengthPercentage) -> i32 {
    match length {
        LengthPercentage::Px(value) | LengthPercentage::Percent(value) => value.round() as i32,
    }
    .max(0)
}

fn apply_opacity(color: ColorValue, opacity: f32) -> ColorValue {
    let alpha = (f32::from(color.alpha) * opacity.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8;
    ColorValue { alpha, ..color }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> TitlebarPlanInput {
        TitlebarPlanInput {
            window_id: WindowId::from("window-1"),
            title: "Example".to_string(),
            focused: true,
            titlebar_style: None,
            window_style: None,
            default_background_focused: ColorValue { red: 1, green: 2, blue: 3, alpha: 255 },
            default_background_unfocused: ColorValue { red: 4, green: 5, blue: 6, alpha: 255 },
            default_text_color_focused: ColorValue { red: 10, green: 11, blue: 12, alpha: 255 },
            default_text_color_unfocused: ColorValue { red: 13, green: 14, blue: 15, alpha: 255 },
            offset_x: 0,
            offset_y: 0,
            effective_opacity: 1.0,
            buttons: TitlebarButtonsConfig::default(),
        }
    }

    #[test]
    fn default_button_layout_uses_shared_order_and_spacing() {
        let plan = build_titlebar_plan(&base_input());

        assert_eq!(plan.height, 28);
        assert_eq!(plan.buttons.len(), 3);

        assert_eq!(plan.buttons[0].kind, TitlebarButtonKind::Close);
        assert_eq!(plan.buttons[1].kind, TitlebarButtonKind::ToggleFullscreen);
        assert_eq!(plan.buttons[2].kind, TitlebarButtonKind::ToggleFloating);

        assert_eq!(plan.buttons[0].rect, TitlebarButtonRect { x: 8, y: 5, width: 18, height: 18 });
        assert_eq!(plan.buttons[1].rect, TitlebarButtonRect { x: 32, y: 5, width: 18, height: 18 });
        assert_eq!(plan.buttons[2].rect, TitlebarButtonRect { x: 56, y: 5, width: 18, height: 18 });
    }

    #[test]
    fn text_left_inset_uses_button_bounds_when_buttons_exist() {
        let mut plan = build_titlebar_plan(&base_input());
        plan.padding_left = 12;

        assert_eq!(titlebar_text_left_inset(&plan), 82);
    }

    #[test]
    fn text_left_inset_respects_padding_when_buttons_are_disabled() {
        let mut input = base_input();
        input.buttons = TitlebarButtonsConfig { close: false, fullscreen: false, floating: false };

        let mut plan = build_titlebar_plan(&input);
        plan.padding_left = 12;

        assert_eq!(plan.buttons.len(), 0);
        assert_eq!(titlebar_text_left_inset(&plan), 12);
    }

    #[test]
    fn text_right_inset_respects_padding_when_no_trailing_content_exists() {
        let mut plan = build_titlebar_plan(&base_input());
        plan.padding_right = 10;

        assert_eq!(titlebar_text_right_inset(&plan, 0), 10);
    }

    #[test]
    fn text_right_inset_reserves_space_for_trailing_content() {
        let mut plan = build_titlebar_plan(&base_input());
        plan.padding_right = 10;

        assert_eq!(titlebar_text_right_inset(&plan, 48), 56);
    }

    #[test]
    fn shared_button_colors_match_expected_palette() {
        assert_eq!(
            titlebar_button_colors(TitlebarButtonKind::Close),
            TitlebarButtonColors { red: 235, green: 87, blue: 87, alpha: 220 }
        );
        assert_eq!(
            titlebar_button_colors(TitlebarButtonKind::ToggleFullscreen),
            TitlebarButtonColors { red: 242, green: 201, blue: 76, alpha: 220 }
        );
        assert_eq!(
            titlebar_button_colors(TitlebarButtonKind::ToggleFloating),
            TitlebarButtonColors { red: 111, green: 207, blue: 151, alpha: 220 }
        );
    }

    #[test]
    fn preview_preset_applies_shared_preview_defaults() {
        let mut input = base_input();
        input.focused = false;
        let plan = build_titlebar_plan_with_preset(&input, &TitlebarPlanPreset::default());

        assert_eq!(plan.height, 24);
        assert_eq!(plan.border_bottom_width, 1);
        assert_eq!(
            plan.border_bottom_color,
            ColorValue { red: 94, green: 99, blue: 118, alpha: 150 }
        );
        assert_eq!(plan.font.size_px, 12);
        assert_eq!(plan.font.weight, FontWeightValue::Normal);
        assert_eq!(plan.padding_left, 8);
        assert_eq!(plan.padding_right, 8);
        assert_eq!(plan.buttons[0].rect, TitlebarButtonRect { x: 8, y: 4, width: 16, height: 16 });
    }
}
