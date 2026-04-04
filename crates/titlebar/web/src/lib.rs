use spiders_css::{ColorValue, FontFamilyName, TextAlignValue};
use spiders_titlebar_core::{
    TitlebarButtonKind, TitlebarPlan, titlebar_button_colors, titlebar_text_left_inset,
    titlebar_text_right_inset,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebTitlebarViewModel {
    pub title: String,
    pub outer_style: String,
    pub title_style: String,
    pub trailing_style: String,
    pub buttons: Vec<WebTitlebarButtonViewModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebTitlebarButtonViewModel {
    pub kind: TitlebarButtonKind,
    pub label: String,
    pub style: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WebTitlebarButtonState {
    pub hovered: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebTitlebarTrailingContent {
    pub text: String,
    pub width_px: i32,
}

pub fn view_model_for_titlebar(plan: &TitlebarPlan, _focused: bool) -> WebTitlebarViewModel {
    view_model_for_titlebar_with_button_state_and_trailing(plan, &[], None)
}

pub fn view_model_for_titlebar_with_button_state(
    plan: &TitlebarPlan,
    button_states: &[(TitlebarButtonKind, WebTitlebarButtonState)],
) -> WebTitlebarViewModel {
    view_model_for_titlebar_with_button_state_and_trailing(plan, button_states, None)
}

pub fn view_model_for_titlebar_with_button_state_and_trailing(
    plan: &TitlebarPlan,
    button_states: &[(TitlebarButtonKind, WebTitlebarButtonState)],
    trailing: Option<&WebTitlebarTrailingContent>,
) -> WebTitlebarViewModel {
    let effective_left_padding = titlebar_text_left_inset(plan);
    let effective_right_padding =
        titlebar_text_right_inset(plan, trailing.map(|content| content.width_px).unwrap_or(0));
    let title_style = vec![
        format!("color: {};", css_color(plan.text_color)),
        format!("text-align: {};", css_text_align(plan.text_align)),
        format!("font-family: {};", css_font_family(&plan.font.families)),
        format!("font-size: {}px;", plan.font.size_px.max(1)),
        format!("font-weight: {};", css_font_weight(plan.font.weight)),
        format!("letter-spacing: {}px;", plan.letter_spacing),
        format!(
            "padding: {}px {}px {}px {}px;",
            plan.padding_top.max(0),
            effective_right_padding.max(0),
            plan.padding_bottom.max(0),
            effective_left_padding.max(0)
        ),
        format!("height: {}px;", plan.height.max(1)),
        "box-sizing: border-box;".to_string(),
        "display: flex; align-items: center; gap: 8px; width: 100%; min-width: 0;".to_string(),
        "white-space: nowrap; overflow: hidden; text-overflow: ellipsis;".to_string(),
    ]
    .join(" ");

    let trailing_style = vec![
        format!("position: absolute; right: {}px;", plan.padding_right.max(0)),
        "top: 50%; transform: translateY(-50%); z-index: 1;".to_string(),
        "pointer-events: none; white-space: nowrap;".to_string(),
    ]
    .join(" ");

    let outer_style = vec![
        format!("height: {}px;", plan.height.max(1)),
        format!("background: {};", css_color(plan.background)),
        format!(
            "border-bottom: {}px solid {};",
            plan.border_bottom_width.max(0),
            css_color(plan.border_bottom_color)
        ),
        format!(
            "padding: {}px {}px {}px {}px;",
            plan.padding_top.max(0),
            plan.padding_right.max(0),
            plan.padding_bottom.max(0),
            plan.padding_left.max(0)
        ),
        format!(
            "border-top-left-radius: {}px; border-top-right-radius: {}px;",
            plan.corner_radius_top_left.max(0),
            plan.corner_radius_top_right.max(0)
        ),
        format!("transform: translate({}px, {}px);", plan.offset_x, plan.offset_y),
        "position: relative; min-width: 0; box-sizing: border-box;".to_string(),
        css_box_shadow(plan),
    ]
    .join(" ");

    let buttons = plan
        .buttons
        .iter()
        .map(|button| {
            let state = button_states
                .iter()
                .find_map(|(kind, state)| (*kind == button.kind).then_some(*state))
                .unwrap_or_default();
            WebTitlebarButtonViewModel {
                kind: button.kind,
                label: button.label.clone(),
                style: format!(
                    "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; padding: 0; border-radius: 9999px; border: none; appearance: none; cursor: pointer; background: {}; opacity: {}; transform: scale({}); transform-origin: center; transition: transform 120ms ease, opacity 120ms ease, filter 120ms ease; filter: {}; z-index: 1;",
                    button.rect.x,
                    button.rect.y,
                    button.rect.width.max(0),
                    button.rect.height.max(0),
                    css_button_color(button.kind),
                    if state.active { "1" } else if state.hovered { "0.98" } else { "0.92" },
                    if state.active { "0.94" } else if state.hovered { "1.06" } else { "1" },
                    if state.hovered { "brightness(1.08)" } else { "none" },
                ),
            }
        })
        .collect();

    WebTitlebarViewModel {
        title: plan.title.clone(),
        outer_style,
        title_style,
        trailing_style,
        buttons,
    }
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

fn css_font_family(families: &[FontFamilyName]) -> String {
    if families.is_empty() {
        return "system-ui, sans-serif".to_string();
    }

    families.iter().map(css_font_family_name).collect::<Vec<_>>().join(", ")
}

fn css_font_family_name(family: &FontFamilyName) -> String {
    match family {
        FontFamilyName::Named(name) => name.clone(),
        FontFamilyName::Serif => "serif".to_string(),
        FontFamilyName::SansSerif => "sans-serif".to_string(),
        FontFamilyName::Monospace => "monospace".to_string(),
        FontFamilyName::Cursive => "cursive".to_string(),
        FontFamilyName::Fantasy => "fantasy".to_string(),
        FontFamilyName::SystemUi => "system-ui".to_string(),
    }
}

fn css_font_weight(weight: spiders_css::FontWeightValue) -> &'static str {
    match weight {
        spiders_css::FontWeightValue::Normal => "400",
        spiders_css::FontWeightValue::Bold => "700",
    }
}

fn css_text_align(value: TextAlignValue) -> &'static str {
    match value {
        TextAlignValue::Left | TextAlignValue::Start => "left",
        TextAlignValue::Right | TextAlignValue::End => "right",
        TextAlignValue::Center => "center",
    }
}

fn css_box_shadow(plan: &TitlebarPlan) -> String {
    let Some(shadows) = plan.box_shadow.as_ref() else {
        return String::new();
    };
    if shadows.is_empty() {
        return String::new();
    }

    let value = shadows
        .iter()
        .map(|shadow| {
            format!(
                "{}px {}px {}px {}px {}{}",
                shadow.offset_x,
                shadow.offset_y,
                shadow.blur_radius.max(0),
                shadow.spread_radius,
                css_color(shadow.color.unwrap_or(plan.text_color)),
                if shadow.inset { " inset" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("box-shadow: {value};")
}

fn css_button_color(kind: TitlebarButtonKind) -> String {
    let color = titlebar_button_colors(kind);
    format!(
        "rgba({}, {}, {}, {:.3})",
        color.red,
        color.green,
        color.blue,
        f32::from(color.alpha) / 255.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::WindowId;
    use spiders_css::{ColorValue, FontQuery, FontWeightValue, TextAlignValue, TextTransformValue};

    fn sample_plan() -> TitlebarPlan {
        TitlebarPlan {
            window_id: WindowId::from("window-1"),
            height: 24,
            offset_x: 0,
            offset_y: 0,
            background: ColorValue { red: 20, green: 21, blue: 22, alpha: 255 },
            border_bottom_width: 1,
            border_bottom_color: ColorValue { red: 30, green: 31, blue: 32, alpha: 255 },
            title: "Example".to_string(),
            text_color: ColorValue { red: 240, green: 241, blue: 242, alpha: 255 },
            text_align: TextAlignValue::Left,
            text_transform: TextTransformValue::None,
            font: FontQuery { families: Vec::new(), weight: FontWeightValue::Normal, size_px: 12 },
            letter_spacing: 0,
            box_shadow: None,
            padding_top: 0,
            padding_right: 8,
            padding_bottom: 0,
            padding_left: 8,
            corner_radius_top_left: 0,
            corner_radius_top_right: 0,
            buttons: vec![
                WebTitlebarButtonViewModelTestHelper::button(
                    TitlebarButtonKind::Close,
                    8,
                    4,
                    16,
                    16,
                ),
                WebTitlebarButtonViewModelTestHelper::button(
                    TitlebarButtonKind::ToggleFullscreen,
                    30,
                    4,
                    16,
                    16,
                ),
            ],
        }
    }

    struct WebTitlebarButtonViewModelTestHelper;

    impl WebTitlebarButtonViewModelTestHelper {
        fn button(
            kind: TitlebarButtonKind,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
        ) -> spiders_titlebar_core::TitlebarButtonPlan {
            spiders_titlebar_core::TitlebarButtonPlan {
                kind,
                rect: spiders_titlebar_core::TitlebarButtonRect { x, y, width, height },
                label: kind_label(kind).to_string(),
            }
        }
    }

    fn kind_label(kind: TitlebarButtonKind) -> &'static str {
        match kind {
            TitlebarButtonKind::Close => "close",
            TitlebarButtonKind::ToggleFullscreen => "fullscreen",
            TitlebarButtonKind::ToggleFloating => "floating",
        }
    }

    #[test]
    fn view_model_uses_button_rects_for_absolute_button_layout() {
        let view_model = view_model_for_titlebar(&sample_plan(), true);

        assert!(
            view_model.buttons[0].style.contains("left: 8px; top: 4px; width: 16px; height: 16px;")
        );
        assert!(
            view_model.buttons[1]
                .style
                .contains("left: 30px; top: 4px; width: 16px; height: 16px;")
        );
    }

    #[test]
    fn view_model_reserves_trailing_space_when_trailing_content_exists() {
        let trailing = WebTitlebarTrailingContent { text: "1200x800".to_string(), width_px: 64 };
        let view_model = view_model_for_titlebar_with_button_state_and_trailing(
            &sample_plan(),
            &[],
            Some(&trailing),
        );

        assert!(view_model.title_style.contains("padding: 0px 72px 0px 54px;"));
        assert!(view_model.trailing_style.contains("right: 8px;"));
    }
}
