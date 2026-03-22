use thiserror::Error;

use super::grid::*;
use super::parse_values::*;

use crate::style::{
    AlignmentValue, AppearanceValue, BorderStyleValue, BoxEdges, BoxSizingValue, ColorValue,
    ContentAlignmentValue, Display, FlexDirectionValue, FlexWrapValue, FontWeightValue,
    GridAutoFlow, GridPlacementValue, GridTemplate, GridTemplateArea, GridTrackValue,
    LengthPercentage, Line, OverflowValue, PositionValue, Size2, SizeValue, TextAlignValue,
    TextTransformValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSide {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Error, PartialEq)]
pub enum CssValueError {
    #[error("unsupported value `{value}` for property `{property}`")]
    UnsupportedValue { property: String, value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompiledDeclaration {
    Ignored,
    Display(Display),
    BoxSizing(BoxSizingValue),
    AspectRatio(f32),
    Appearance(AppearanceValue),
    Background(ColorValue),
    Color(ColorValue),
    Opacity(f32),
    BorderColor(ColorValue),
    BorderColorSide(BoxSide, ColorValue),
    BorderStyle(BoxEdges<BorderStyleValue>),
    BorderStyleSide(BoxSide, BorderStyleValue),
    BorderRadius(String),
    BoxShadow(String),
    BackdropFilter(String),
    Transform(String),
    TextAlign(TextAlignValue),
    TextTransform(TextTransformValue),
    FontSize(LengthPercentage),
    FontWeight(FontWeightValue),
    LetterSpacing(f32),
    Animation(String),
    Transition(String),
    TransitionProperty(String),
    TransitionDuration(String),
    TransitionTimingFunction(String),
    FlexDirection(FlexDirectionValue),
    FlexWrap(FlexWrapValue),
    FlexGrow(f32),
    FlexShrink(f32),
    FlexBasis(SizeValue),
    Position(PositionValue),
    Inset(BoxEdges<SizeValue>),
    InsetSide(BoxSide, SizeValue),
    Overflow(OverflowValue, OverflowValue),
    OverflowX(OverflowValue),
    OverflowY(OverflowValue),
    Width(SizeValue),
    Height(SizeValue),
    MinWidth(SizeValue),
    MinHeight(SizeValue),
    MaxWidth(SizeValue),
    MaxHeight(SizeValue),
    AlignItems(AlignmentValue),
    AlignSelf(AlignmentValue),
    JustifyItems(AlignmentValue),
    JustifySelf(AlignmentValue),
    AlignContent(ContentAlignmentValue),
    JustifyContent(ContentAlignmentValue),
    Gap(Size2<LengthPercentage>),
    GridTemplateRows(GridTemplate),
    GridTemplateColumns(GridTemplate),
    GridAutoRows(Vec<GridTrackValue>),
    GridAutoColumns(Vec<GridTrackValue>),
    GridAutoFlow(GridAutoFlow),
    GridTemplateAreas(Vec<GridTemplateArea>),
    GridRow(Line<GridPlacementValue>),
    GridColumn(Line<GridPlacementValue>),
    Border(BoxEdges<LengthPercentage>),
    BorderSide(BoxSide, LengthPercentage),
    Padding(BoxEdges<LengthPercentage>),
    PaddingSide(BoxSide, LengthPercentage),
    Margin(BoxEdges<SizeValue>),
    MarginSide(BoxSide, SizeValue),
}

pub fn compile_declaration(
    parsed: &ParsedDeclaration,
) -> Result<CompiledDeclaration, CssValueError> {
    compile_declaration_from_value(&parsed.property, &parsed.value)
}

pub fn compile_declaration_from_value(
    property: &str,
    value: &CssValue,
) -> Result<CompiledDeclaration, CssValueError> {
    match property {
        "display" => Ok(CompiledDeclaration::Display(parse_display_direct(
            property, value,
        )?)),
        "box-sizing" => Ok(CompiledDeclaration::BoxSizing(parse_box_sizing_direct(
            property, value,
        )?)),
        "aspect-ratio" => Ok(CompiledDeclaration::AspectRatio(parse_aspect_ratio_direct(
            property, value,
        )?)),
        "appearance" => Ok(CompiledDeclaration::Appearance(parse_appearance_direct(
            property, value,
        )?)),
        "background" | "background-color" => Ok(CompiledDeclaration::Background(parse_color_direct(
            property, value,
        )?)),
        "color" => Ok(CompiledDeclaration::Color(parse_color_direct(
            property, value,
        )?)),
        "opacity" => Ok(CompiledDeclaration::Opacity(parse_number_direct(
            property, value,
        )?)),
        "border-color" => Ok(CompiledDeclaration::BorderColor(parse_color_direct(
            property, value,
        )?)),
        "border-style" => Ok(CompiledDeclaration::BorderStyle(parse_box_border_styles_direct(
            property, value,
        )?)),
        "border-top-style" => Ok(CompiledDeclaration::BorderStyleSide(
            BoxSide::Top,
            parse_border_style_direct(property, value)?,
        )),
        "border-right-style" => Ok(CompiledDeclaration::BorderStyleSide(
            BoxSide::Right,
            parse_border_style_direct(property, value)?,
        )),
        "border-bottom-style" => Ok(CompiledDeclaration::BorderStyleSide(
            BoxSide::Bottom,
            parse_border_style_direct(property, value)?,
        )),
        "border-left-style" => Ok(CompiledDeclaration::BorderStyleSide(
            BoxSide::Left,
            parse_border_style_direct(property, value)?,
        )),
        "border-top-color" => Ok(CompiledDeclaration::BorderColorSide(
            BoxSide::Top,
            parse_color_direct(property, value)?,
        )),
        "border-right-color" => Ok(CompiledDeclaration::BorderColorSide(
            BoxSide::Right,
            parse_color_direct(property, value)?,
        )),
        "border-bottom-color" => Ok(CompiledDeclaration::BorderColorSide(
            BoxSide::Bottom,
            parse_color_direct(property, value)?,
        )),
        "border-left-color" => Ok(CompiledDeclaration::BorderColorSide(
            BoxSide::Left,
            parse_color_direct(property, value)?,
        )),
        "border-radius" => Ok(CompiledDeclaration::BorderRadius(parse_raw_text_direct(
            property, value,
        )?)),
        "box-shadow" => Ok(CompiledDeclaration::BoxShadow(parse_raw_text_direct(
            property, value,
        )?)),
        "backdrop-filter" => Ok(CompiledDeclaration::BackdropFilter(parse_raw_text_direct(
            property, value,
        )?)),
        "transform" => Ok(CompiledDeclaration::Transform(parse_raw_text_direct(
            property, value,
        )?)),
        "text-align" => Ok(CompiledDeclaration::TextAlign(parse_text_align_direct(
            property, value,
        )?)),
        "text-transform" => Ok(CompiledDeclaration::TextTransform(
            parse_text_transform_direct(property, value)?,
        )),
        "font-size" => Ok(CompiledDeclaration::FontSize(parse_length_percentage_word(
            property,
            value.text.trim(),
        )?)),
        "font-weight" => Ok(CompiledDeclaration::FontWeight(parse_font_weight_direct(
            property, value,
        )?)),
        "letter-spacing" => Ok(CompiledDeclaration::LetterSpacing(
            parse_letter_spacing_direct(property, value)?,
        )),
        "animation" => Ok(CompiledDeclaration::Animation(parse_raw_text_direct(
            property, value,
        )?)),
        "animation-name"
        | "animation-duration"
        | "animation-timing-function"
        | "animation-delay"
        | "animation-iteration-count"
        | "animation-direction"
        | "animation-fill-mode"
        | "animation-play-state" => Ok(CompiledDeclaration::Ignored),
        "transition" => Ok(CompiledDeclaration::Transition(parse_raw_text_direct(
            property, value,
        )?)),
        "transition-property" => Ok(CompiledDeclaration::TransitionProperty(parse_raw_text_direct(
            property, value,
        )?)),
        "transition-duration" => Ok(CompiledDeclaration::TransitionDuration(parse_raw_text_direct(
            property, value,
        )?)),
        "transition-timing-function" => Ok(CompiledDeclaration::TransitionTimingFunction(parse_raw_text_direct(
            property, value,
        )?)),
        "transition-delay" | "transition-behavior" => Ok(CompiledDeclaration::Ignored),
        "flex-direction" => Ok(CompiledDeclaration::FlexDirection(
            parse_flex_direction_direct(property, value)?,
        )),
        "flex-wrap" => Ok(CompiledDeclaration::FlexWrap(parse_flex_wrap_direct(
            property, value,
        )?)),
        "flex-grow" => Ok(CompiledDeclaration::FlexGrow(parse_number_direct(
            property, value,
        )?)),
        "flex-shrink" => Ok(CompiledDeclaration::FlexShrink(parse_number_direct(
            property, value,
        )?)),
        "flex-basis" => Ok(CompiledDeclaration::FlexBasis(parse_size_value_direct(
            property, value,
        )?)),
        "position" => Ok(CompiledDeclaration::Position(parse_position_direct(
            property, value,
        )?)),
        "inset" => Ok(CompiledDeclaration::Inset(parse_box_edges_size_direct(
            property, value,
        )?)),
        "top" => Ok(CompiledDeclaration::InsetSide(
            BoxSide::Top,
            parse_size_value_direct(property, value)?,
        )),
        "right" => Ok(CompiledDeclaration::InsetSide(
            BoxSide::Right,
            parse_size_value_direct(property, value)?,
        )),
        "bottom" => Ok(CompiledDeclaration::InsetSide(
            BoxSide::Bottom,
            parse_size_value_direct(property, value)?,
        )),
        "left" => Ok(CompiledDeclaration::InsetSide(
            BoxSide::Left,
            parse_size_value_direct(property, value)?,
        )),
        "overflow" => {
            let (x, y) = parse_overflow_pair_direct(property, value)?;
            Ok(CompiledDeclaration::Overflow(x, y))
        }
        "overflow-x" => Ok(CompiledDeclaration::OverflowX(parse_overflow_direct(
            property, value,
        )?)),
        "overflow-y" => Ok(CompiledDeclaration::OverflowY(parse_overflow_direct(
            property, value,
        )?)),
        "width" => Ok(CompiledDeclaration::Width(parse_size_value_direct(
            property, value,
        )?)),
        "height" => Ok(CompiledDeclaration::Height(parse_size_value_direct(
            property, value,
        )?)),
        "min-width" => Ok(CompiledDeclaration::MinWidth(parse_size_value_direct(
            property, value,
        )?)),
        "min-height" => Ok(CompiledDeclaration::MinHeight(parse_size_value_direct(
            property, value,
        )?)),
        "max-width" => Ok(CompiledDeclaration::MaxWidth(parse_size_value_direct(
            property, value,
        )?)),
        "max-height" => Ok(CompiledDeclaration::MaxHeight(parse_size_value_direct(
            property, value,
        )?)),
        "align-items" => Ok(CompiledDeclaration::AlignItems(parse_alignment_direct(
            property, value,
        )?)),
        "align-self" => Ok(CompiledDeclaration::AlignSelf(parse_alignment_direct(
            property, value,
        )?)),
        "justify-items" => Ok(CompiledDeclaration::JustifyItems(parse_alignment_direct(
            property, value,
        )?)),
        "justify-self" => Ok(CompiledDeclaration::JustifySelf(parse_alignment_direct(
            property, value,
        )?)),
        "align-content" => Ok(CompiledDeclaration::AlignContent(
            parse_content_alignment_direct(property, value)?,
        )),
        "justify-content" => Ok(CompiledDeclaration::JustifyContent(
            parse_content_alignment_direct(property, value)?,
        )),
        "gap" => Ok(CompiledDeclaration::Gap(parse_gap_direct(property, value)?)),
        "row-gap" => Ok(CompiledDeclaration::Gap(parse_axis_gap_direct(
            property, value, true,
        )?)),
        "column-gap" => Ok(CompiledDeclaration::Gap(parse_axis_gap_direct(
            property, value, false,
        )?)),
        "grid-template-rows" => Ok(CompiledDeclaration::GridTemplateRows(parse_grid_tracks(
            property, value,
        )?)),
        "grid-template-columns" => Ok(CompiledDeclaration::GridTemplateColumns(parse_grid_tracks(
            property, value,
        )?)),
        "grid-auto-rows" => Ok(CompiledDeclaration::GridAutoRows(parse_grid_auto_tracks(
            property, value,
        )?)),
        "grid-auto-columns" => Ok(CompiledDeclaration::GridAutoColumns(
            parse_grid_auto_tracks(property, value)?,
        )),
        "grid-template-areas" => Ok(CompiledDeclaration::GridTemplateAreas(
            parse_grid_template_areas(property, value)?,
        )),
        "grid-row" => Ok(CompiledDeclaration::GridRow(parse_grid_line_shorthand(
            property, value,
        )?)),
        "grid-column" => Ok(CompiledDeclaration::GridColumn(parse_grid_line_shorthand(
            property, value,
        )?)),
        "grid-row-start" | "grid-row-end" => Ok(CompiledDeclaration::GridRow(
            parse_grid_line_side(property, value)?,
        )),
        "grid-column-start" | "grid-column-end" => Ok(CompiledDeclaration::GridColumn(
            parse_grid_line_side(property, value)?,
        )),
        "border-width" => Ok(CompiledDeclaration::Border(parse_box_edges_direct(
            property, value,
        )?)),
        "border-top-width" => Ok(CompiledDeclaration::BorderSide(
            BoxSide::Top,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "border-right-width" => Ok(CompiledDeclaration::BorderSide(
            BoxSide::Right,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "border-bottom-width" => Ok(CompiledDeclaration::BorderSide(
            BoxSide::Bottom,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "border-left-width" => Ok(CompiledDeclaration::BorderSide(
            BoxSide::Left,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "padding" => Ok(CompiledDeclaration::Padding(parse_box_edges_direct(
            property, value,
        )?)),
        "padding-top" => Ok(CompiledDeclaration::PaddingSide(
            BoxSide::Top,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "padding-right" => Ok(CompiledDeclaration::PaddingSide(
            BoxSide::Right,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "padding-bottom" => Ok(CompiledDeclaration::PaddingSide(
            BoxSide::Bottom,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "padding-left" => Ok(CompiledDeclaration::PaddingSide(
            BoxSide::Left,
            parse_length_percentage_word(property, value.text.trim())?,
        )),
        "margin" => Ok(CompiledDeclaration::Margin(parse_box_edges_size_direct(
            property, value,
        )?)),
        "margin-top" => Ok(CompiledDeclaration::MarginSide(
            BoxSide::Top,
            parse_size_value_direct(property, value)?,
        )),
        "margin-right" => Ok(CompiledDeclaration::MarginSide(
            BoxSide::Right,
            parse_size_value_direct(property, value)?,
        )),
        "margin-bottom" => Ok(CompiledDeclaration::MarginSide(
            BoxSide::Bottom,
            parse_size_value_direct(property, value)?,
        )),
        "margin-left" => Ok(CompiledDeclaration::MarginSide(
            BoxSide::Left,
            parse_size_value_direct(property, value)?,
        )),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        }),
    }
}

fn keyword<'a>(property: &str, value: &'a CssValue) -> Result<&'a str, CssValueError> {
    let trimmed = value.text.trim();
    if trimmed.is_empty() || trimmed.split_whitespace().count() != 1 {
        return Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        });
    }
    Ok(trimmed)
}

fn parse_display_direct(property: &str, value: &CssValue) -> Result<Display, CssValueError> {
    match keyword(property, value)? {
        "block" => Ok(Display::Block),
        "flex" => Ok(Display::Flex),
        "grid" => Ok(Display::Grid),
        "none" => Ok(Display::None),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_appearance_direct(
    property: &str,
    value: &CssValue,
) -> Result<AppearanceValue, CssValueError> {
    match keyword(property, value)? {
        "auto" => Ok(AppearanceValue::Auto),
        "none" => Ok(AppearanceValue::None),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        }),
    }
}

fn parse_text_align_direct(property: &str, value: &CssValue) -> Result<TextAlignValue, CssValueError> {
    match keyword(property, value)? {
        "left" => Ok(TextAlignValue::Left),
        "right" => Ok(TextAlignValue::Right),
        "center" => Ok(TextAlignValue::Center),
        "start" => Ok(TextAlignValue::Start),
        "end" => Ok(TextAlignValue::End),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        }),
    }
}

fn parse_text_transform_direct(
    property: &str,
    value: &CssValue,
) -> Result<TextTransformValue, CssValueError> {
    match keyword(property, value)? {
        "none" => Ok(TextTransformValue::None),
        "uppercase" => Ok(TextTransformValue::Uppercase),
        "lowercase" => Ok(TextTransformValue::Lowercase),
        "capitalize" => Ok(TextTransformValue::Capitalize),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        }),
    }
}

fn parse_font_weight_direct(
    property: &str,
    value: &CssValue,
) -> Result<FontWeightValue, CssValueError> {
    match keyword(property, value)? {
        "normal" | "400" => Ok(FontWeightValue::Normal),
        "bold" | "700" => Ok(FontWeightValue::Bold),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        }),
    }
}

fn parse_letter_spacing_direct(property: &str, value: &CssValue) -> Result<f32, CssValueError> {
    match keyword(property, value)? {
        "normal" => Ok(0.0),
        text => text
            .strip_suffix("px")
            .and_then(|number| number.parse::<f32>().ok())
            .ok_or_else(|| CssValueError::UnsupportedValue {
                property: property.to_string(),
                value: value.text.clone(),
            }),
    }
}

fn parse_color_direct(property: &str, value: &CssValue) -> Result<ColorValue, CssValueError> {
    let text = value.text.trim();
    if text.is_empty() {
        return Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        });
    }
    if text.eq_ignore_ascii_case("transparent") {
        return Ok(ColorValue {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        });
    }

    if let Some(color) = parse_hex_color(text) {
        return Ok(color);
    }

    if let Some(color) = parse_rgb_function(text) {
        return Ok(color);
    }

    Err(CssValueError::UnsupportedValue {
        property: property.to_string(),
        value: value.text.clone(),
    })
}

fn parse_border_style_direct(
    property: &str,
    value: &CssValue,
) -> Result<BorderStyleValue, CssValueError> {
    match keyword(property, value)? {
        "none" => Ok(BorderStyleValue::None),
        "solid" => Ok(BorderStyleValue::Solid),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        }),
    }
}

fn parse_raw_text_direct(property: &str, value: &CssValue) -> Result<String, CssValueError> {
    let text = value.text.trim();
    if text.is_empty() {
        Err(CssValueError::UnsupportedValue {
            property: property.to_string(),
            value: value.text.clone(),
        })
    } else {
        Ok(text.to_string())
    }
}

fn parse_hex_color(input: &str) -> Option<ColorValue> {
    let hex = input.strip_prefix('#')?;
    match hex.len() {
        3 => Some(ColorValue {
            red: parse_hex_nibble(hex.as_bytes()[0])? * 17,
            green: parse_hex_nibble(hex.as_bytes()[1])? * 17,
            blue: parse_hex_nibble(hex.as_bytes()[2])? * 17,
            alpha: 255,
        }),
        4 => Some(ColorValue {
            red: parse_hex_nibble(hex.as_bytes()[0])? * 17,
            green: parse_hex_nibble(hex.as_bytes()[1])? * 17,
            blue: parse_hex_nibble(hex.as_bytes()[2])? * 17,
            alpha: parse_hex_nibble(hex.as_bytes()[3])? * 17,
        }),
        6 => Some(ColorValue {
            red: parse_hex_byte(&hex[0..2])?,
            green: parse_hex_byte(&hex[2..4])?,
            blue: parse_hex_byte(&hex[4..6])?,
            alpha: 255,
        }),
        8 => Some(ColorValue {
            red: parse_hex_byte(&hex[0..2])?,
            green: parse_hex_byte(&hex[2..4])?,
            blue: parse_hex_byte(&hex[4..6])?,
            alpha: parse_hex_byte(&hex[6..8])?,
        }),
        _ => None,
    }
}

fn parse_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_hex_byte(input: &str) -> Option<u8> {
    u8::from_str_radix(input, 16).ok()
}

fn parse_rgb_function(input: &str) -> Option<ColorValue> {
    let (name, args) = input.split_once('(')?;
    let args = args.strip_suffix(')')?;
    let parts = args.split(',').map(str::trim).collect::<Vec<_>>();

    match name {
        "rgb" if parts.len() == 3 => Some(ColorValue {
            red: parse_color_channel(parts[0])?,
            green: parse_color_channel(parts[1])?,
            blue: parse_color_channel(parts[2])?,
            alpha: 255,
        }),
        "rgba" if parts.len() == 4 => Some(ColorValue {
            red: parse_color_channel(parts[0])?,
            green: parse_color_channel(parts[1])?,
            blue: parse_color_channel(parts[2])?,
            alpha: parse_alpha_channel(parts[3])?,
        }),
        _ => None,
    }
}

fn parse_color_channel(input: &str) -> Option<u8> {
    input.parse::<u16>().ok().map(|value| value.min(255) as u8)
}

fn parse_alpha_channel(input: &str) -> Option<u8> {
    if let Ok(value) = input.parse::<f32>() {
        return Some((value.clamp(0.0, 1.0) * 255.0).round() as u8);
    }

    input.parse::<u16>().ok().map(|value| value.min(255) as u8)
}

fn parse_box_sizing_direct(
    property: &str,
    value: &CssValue,
) -> Result<BoxSizingValue, CssValueError> {
    match keyword(property, value)? {
        "border-box" => Ok(BoxSizingValue::BorderBox),
        "content-box" => Ok(BoxSizingValue::ContentBox),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_aspect_ratio_direct(property: &str, value: &CssValue) -> Result<f32, CssValueError> {
    let trimmed = value.text.trim();
    if let Some((left, right)) = trimmed.split_once('/') {
        let left = left
            .trim()
            .parse::<f32>()
            .map_err(|_| CssValueError::UnsupportedValue {
                property: property.into(),
                value: value.text.clone(),
            })?;
        let right = right
            .trim()
            .parse::<f32>()
            .map_err(|_| CssValueError::UnsupportedValue {
                property: property.into(),
                value: value.text.clone(),
            })?;
        if right == 0.0 {
            return Err(CssValueError::UnsupportedValue {
                property: property.into(),
                value: value.text.clone(),
            });
        }
        return Ok(left / right);
    }

    trimmed
        .parse::<f32>()
        .map_err(|_| CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        })
}

fn parse_flex_direction_direct(
    property: &str,
    value: &CssValue,
) -> Result<FlexDirectionValue, CssValueError> {
    match keyword(property, value)? {
        "row" => Ok(FlexDirectionValue::Row),
        "column" => Ok(FlexDirectionValue::Column),
        "row-reverse" => Ok(FlexDirectionValue::RowReverse),
        "column-reverse" => Ok(FlexDirectionValue::ColumnReverse),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_flex_wrap_direct(
    property: &str,
    value: &CssValue,
) -> Result<FlexWrapValue, CssValueError> {
    match keyword(property, value)? {
        "nowrap" => Ok(FlexWrapValue::NoWrap),
        "wrap" => Ok(FlexWrapValue::Wrap),
        "wrap-reverse" => Ok(FlexWrapValue::WrapReverse),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_position_direct(property: &str, value: &CssValue) -> Result<PositionValue, CssValueError> {
    match keyword(property, value)? {
        "relative" => Ok(PositionValue::Relative),
        "absolute" => Ok(PositionValue::Absolute),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_overflow_direct(property: &str, value: &CssValue) -> Result<OverflowValue, CssValueError> {
    match keyword(property, value)? {
        "visible" => Ok(OverflowValue::Visible),
        "clip" => Ok(OverflowValue::Clip),
        "hidden" => Ok(OverflowValue::Hidden),
        "scroll" => Ok(OverflowValue::Scroll),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_overflow_pair_direct(
    property: &str,
    value: &CssValue,
) -> Result<(OverflowValue, OverflowValue), CssValueError> {
    let values = split_words(value)
        .into_iter()
        .map(|word| {
            parse_overflow_direct(
                property,
                &CssValue {
                    text: word.into(),
                    components: Vec::new(),
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    match values.as_slice() {
        [single] => Ok((*single, *single)),
        [x, y] => Ok((*x, *y)),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_alignment_direct(
    property: &str,
    value: &CssValue,
) -> Result<AlignmentValue, CssValueError> {
    match keyword(property, value)? {
        "start" => Ok(AlignmentValue::Start),
        "end" => Ok(AlignmentValue::End),
        "flex-start" => Ok(AlignmentValue::FlexStart),
        "flex-end" => Ok(AlignmentValue::FlexEnd),
        "center" => Ok(AlignmentValue::Center),
        "baseline" => Ok(AlignmentValue::Baseline),
        "stretch" => Ok(AlignmentValue::Stretch),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_content_alignment_direct(
    property: &str,
    value: &CssValue,
) -> Result<ContentAlignmentValue, CssValueError> {
    match keyword(property, value)? {
        "start" => Ok(ContentAlignmentValue::Start),
        "end" => Ok(ContentAlignmentValue::End),
        "flex-start" => Ok(ContentAlignmentValue::FlexStart),
        "flex-end" => Ok(ContentAlignmentValue::FlexEnd),
        "center" => Ok(ContentAlignmentValue::Center),
        "stretch" => Ok(ContentAlignmentValue::Stretch),
        "space-between" => Ok(ContentAlignmentValue::SpaceBetween),
        "space-evenly" => Ok(ContentAlignmentValue::SpaceEvenly),
        "space-around" => Ok(ContentAlignmentValue::SpaceAround),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_gap_direct(
    property: &str,
    value: &CssValue,
) -> Result<Size2<LengthPercentage>, CssValueError> {
    let values = split_words(value)
        .into_iter()
        .map(|word| parse_length_percentage_word(property, word))
        .collect::<Result<Vec<_>, _>>()?;

    match values.as_slice() {
        [single] => Ok(Size2 {
            width: *single,
            height: *single,
        }),
        [row, column] => Ok(Size2 {
            width: *column,
            height: *row,
        }),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn parse_axis_gap_direct(
    property: &str,
    value: &CssValue,
    is_row: bool,
) -> Result<Size2<LengthPercentage>, CssValueError> {
    let parsed = match split_words(value).as_slice() {
        [single] => parse_length_percentage_word(property, single)?,
        _ => {
            return Err(CssValueError::UnsupportedValue {
                property: property.into(),
                value: value.text.clone(),
            });
        }
    };

    Ok(if is_row {
        Size2 {
            width: LengthPercentage::Px(0.0),
            height: parsed,
        }
    } else {
        Size2 {
            width: parsed,
            height: LengthPercentage::Px(0.0),
        }
    })
}

fn parse_number_direct(property: &str, value: &CssValue) -> Result<f32, CssValueError> {
    value
        .text
        .trim()
        .parse::<f32>()
        .map_err(|_| CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        })
}

fn split_words(value: &CssValue) -> Vec<&str> {
    value.text.split_whitespace().collect()
}

fn parse_length_percentage_word(
    property: &str,
    word: &str,
) -> Result<LengthPercentage, CssValueError> {
    if word == "0" || word == "0.0" {
        return Ok(LengthPercentage::Px(0.0));
    }
    if let Some(number) = word.strip_suffix("px") {
        return number
            .parse::<f32>()
            .map(LengthPercentage::Px)
            .map_err(|_| CssValueError::UnsupportedValue {
                property: property.into(),
                value: word.into(),
            });
    }
    if let Some(number) = word.strip_suffix('%') {
        return number
            .parse::<f32>()
            .map(LengthPercentage::Percent)
            .map_err(|_| CssValueError::UnsupportedValue {
                property: property.into(),
                value: word.into(),
            });
    }
    Err(CssValueError::UnsupportedValue {
        property: property.into(),
        value: word.into(),
    })
}

fn parse_size_value_direct(property: &str, value: &CssValue) -> Result<SizeValue, CssValueError> {
    match split_words(value).as_slice() {
        ["auto"] => Ok(SizeValue::Auto),
        [single] => Ok(SizeValue::LengthPercentage(parse_length_percentage_word(
            property, single,
        )?)),
        _ => Err(CssValueError::UnsupportedValue {
            property: property.into(),
            value: value.text.clone(),
        }),
    }
}

fn expand_box_sides<T: Copy>(values: &[T]) -> Option<BoxEdges<T>> {
    match values {
        [a] => Some(BoxEdges {
            top: *a,
            right: *a,
            bottom: *a,
            left: *a,
        }),
        [vertical, horizontal] => Some(BoxEdges {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        }),
        [top, horizontal, bottom] => Some(BoxEdges {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        }),
        [top, right, bottom, left] => Some(BoxEdges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => None,
    }
}

fn parse_box_edges_direct(
    property: &str,
    value: &CssValue,
) -> Result<BoxEdges<LengthPercentage>, CssValueError> {
    let parsed = split_words(value)
        .into_iter()
        .map(|word| parse_length_percentage_word(property, word))
        .collect::<Result<Vec<_>, _>>()?;
    expand_box_sides(&parsed).ok_or_else(|| CssValueError::UnsupportedValue {
        property: property.into(),
        value: value.text.clone(),
    })
}

fn parse_box_border_styles_direct(
    property: &str,
    value: &CssValue,
) -> Result<BoxEdges<BorderStyleValue>, CssValueError> {
    let parsed = split_words(value)
        .into_iter()
        .map(|word| {
            parse_border_style_direct(
                property,
                &CssValue {
                    text: word.to_string(),
                    components: Vec::new(),
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    expand_box_sides(&parsed).ok_or_else(|| CssValueError::UnsupportedValue {
        property: property.into(),
        value: value.text.clone(),
    })
}

fn parse_box_edges_size_direct(
    property: &str,
    value: &CssValue,
) -> Result<BoxEdges<SizeValue>, CssValueError> {
    let parsed = split_words(value)
        .into_iter()
        .map(|word| {
            if word == "auto" {
                Ok(SizeValue::Auto)
            } else {
                parse_length_percentage_word(property, word).map(SizeValue::LengthPercentage)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    expand_box_sides(&parsed).ok_or_else(|| CssValueError::UnsupportedValue {
        property: property.into(),
        value: value.text.clone(),
    })
}

// ── Value utility helpers (used by grid and other css submodules) ──────────────

pub(super) fn text_for_value(value: &CssValue) -> &str {
    value.text.as_str()
}

pub(super) fn normalized_components(value: &CssValue) -> Vec<&CssValueToken> {
    value
        .components
        .iter()
        .filter(|component| !matches!(component, CssValueToken::Whitespace))
        .collect()
}

pub(super) fn normalized_components_owned(value: &CssValue) -> Vec<CssValueToken> {
    value
        .components
        .iter()
        .filter(|component| !matches!(component, CssValueToken::Whitespace))
        .cloned()
        .collect()
}

pub(super) fn parse_length_percentage(
    property: &str,
    value: &CssValue,
) -> Result<LengthPercentage, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Integer(0)] => Ok(LengthPercentage::Px(0.0)),
        [CssValueToken::Number(number)] if *number == 0.0 => Ok(LengthPercentage::Px(0.0)),
        [CssValueToken::Dimension(dimension)] => {
            parse_dimension_length_percentage(property, dimension)
        }
        [CssValueToken::Percentage(percent)] => Ok(LengthPercentage::Percent(*percent)),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

pub(super) fn parse_dimension_length_percentage(
    property: &str,
    dimension: &CssDimension,
) -> Result<LengthPercentage, CssValueError> {
    if dimension.unit.eq_ignore_ascii_case("px") {
        Ok(LengthPercentage::Px(dimension.value))
    } else {
        Err(invalid_value(
            property,
            &format!("{}{}", dimension.value, dimension.unit),
        ))
    }
}

pub(super) fn function_args_value(function: &CssFunction) -> CssValue {
    CssValue {
        text: components_to_text(&function.value),
        components: function.value.clone(),
    }
}

pub(super) fn split_function_args(function: &CssFunction) -> Vec<CssValue> {
    let mut groups = Vec::new();
    let mut current = Vec::new();

    for component in &function.value {
        if matches!(component, CssValueToken::Delimiter(CssDelimiter::Comma)) {
            groups.push(CssValue {
                text: components_to_text(&current),
                components: std::mem::take(&mut current),
            });
            continue;
        }
        current.push(component.clone());
    }

    groups.push(CssValue {
        text: components_to_text(&current),
        components: current,
    });
    groups
}

pub(super) fn slice_to_value(components: &[&CssValueToken]) -> CssValue {
    let owned = components
        .iter()
        .map(|component| (*component).clone())
        .collect::<Vec<_>>();
    CssValue {
        text: components_to_text(&owned),
        components: owned,
    }
}

pub(super) fn components_to_text(components: &[CssValueToken]) -> String {
    let mut output = String::new();
    for component in components {
        output.push_str(&component_text(component));
    }
    output.trim().to_owned()
}

pub(super) fn component_text(component: &CssValueToken) -> String {
    match component {
        CssValueToken::Ident(value) => value.clone(),
        CssValueToken::String(value) => format!("\"{value}\""),
        CssValueToken::Number(value) => value.to_string(),
        CssValueToken::Integer(value) => value.to_string(),
        CssValueToken::Dimension(value) => format!("{}{}", value.value, value.unit),
        CssValueToken::Percentage(value) => format!("{value}%"),
        CssValueToken::Function(function) => {
            format!("{}({})", function.name, components_to_text(&function.value))
        }
        CssValueToken::SimpleBlock(block) => {
            let (open, close) = match block.kind {
                CssSimpleBlockKind::Bracket => ('[', ']'),
                CssSimpleBlockKind::Parenthesis => ('(', ')'),
                CssSimpleBlockKind::Brace => ('{', '}'),
            };
            format!("{open}{}{close}", components_to_text(&block.value))
        }
        CssValueToken::Delimiter(CssDelimiter::Comma) => ",".into(),
        CssValueToken::Delimiter(CssDelimiter::Solidus) => "/".into(),
        CssValueToken::Delimiter(CssDelimiter::Semicolon) => ";".into(),
        CssValueToken::Whitespace => " ".into(),
        CssValueToken::Unknown(value) => value.clone(),
    }
}

pub(super) fn invalid_value(property: &str, value: &str) -> CssValueError {
    CssValueError::UnsupportedValue {
        property: property.to_owned(),
        value: value.to_owned(),
    }
}
