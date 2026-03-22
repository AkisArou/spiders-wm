use taffy::geometry::{Line as TaffyLine, Rect as TaffyRect};
use taffy::prelude::{
    AlignContent as TaffyAlignContent, AlignItems as TaffyAlignItems, BoxSizing as TaffyBoxSizing,
    Dimension as TaffyDimension, Display as TaffyDisplay, FlexDirection as TaffyFlexDirection,
    FlexWrap as TaffyFlexWrap, FromFr, FromLength, FromPercent, GridAutoFlow as TaffyGridAutoFlow,
    GridPlacement as TaffyGridPlacement, GridTemplateComponent as TaffyGridTemplateComponent,
    JustifyContent as TaffyJustifyContent, LengthPercentage as TaffyTrackLengthPercentage,
    MaxTrackSizingFunction as TaffyMaxTrackSizingFunction,
    MinTrackSizingFunction as TaffyMinTrackSizingFunction, Position as TaffyPosition,
    RepetitionCount as TaffyRepetitionCount, TaffyAuto, TaffyFitContent, TaffyMaxContent,
    TaffyMinContent, TrackSizingFunction as TaffyTrackSizingFunction,
};
use taffy::style::{
    GridTemplateArea as TaffyGridTemplateArea,
    GridTemplateRepetition as TaffyGridTemplateRepetition,
    LengthPercentage as TaffyLengthPercentage, LengthPercentageAuto as TaffyLengthPercentageAuto,
    Overflow as TaffyOverflow,
};

use super::compile::{BoxSide, CompiledDeclaration, CssValueError};
use super::parse_values::*;
use super::values::*;

pub(super) trait ApplyCompiledDeclaration {
    fn apply(&mut self, declaration: CompiledDeclaration);
}

impl ApplyCompiledDeclaration for ComputedStyle {
    fn apply(&mut self, declaration: CompiledDeclaration) {
        match declaration {
            CompiledDeclaration::Display(value) => self.display = Some(value),
            CompiledDeclaration::BoxSizing(value) => self.box_sizing = Some(value),
            CompiledDeclaration::AspectRatio(value) => self.aspect_ratio = Some(value),
            CompiledDeclaration::FlexDirection(value) => self.flex_direction = Some(value),
            CompiledDeclaration::FlexWrap(value) => self.flex_wrap = Some(value),
            CompiledDeclaration::FlexGrow(value) => self.flex_grow = Some(value),
            CompiledDeclaration::FlexShrink(value) => self.flex_shrink = Some(value),
            CompiledDeclaration::FlexBasis(value) => self.flex_basis = Some(value),
            CompiledDeclaration::Position(value) => self.position = Some(value),
            CompiledDeclaration::Inset(value) => self.inset = Some(value),
            CompiledDeclaration::InsetSide(side, value) => {
                let mut inset = self.inset.unwrap_or(BoxEdges {
                    top: SizeValue::Auto,
                    right: SizeValue::Auto,
                    bottom: SizeValue::Auto,
                    left: SizeValue::Auto,
                });
                match side {
                    BoxSide::Top => inset.top = value,
                    BoxSide::Right => inset.right = value,
                    BoxSide::Bottom => inset.bottom = value,
                    BoxSide::Left => inset.left = value,
                }
                self.inset = Some(inset);
            }
            CompiledDeclaration::Overflow(x, y) => {
                self.overflow_x = Some(x);
                self.overflow_y = Some(y);
            }
            CompiledDeclaration::OverflowX(value) => self.overflow_x = Some(value),
            CompiledDeclaration::OverflowY(value) => self.overflow_y = Some(value),
            CompiledDeclaration::Width(value) => self.width = Some(value),
            CompiledDeclaration::Height(value) => self.height = Some(value),
            CompiledDeclaration::MinWidth(value) => self.min_width = Some(value),
            CompiledDeclaration::MinHeight(value) => self.min_height = Some(value),
            CompiledDeclaration::MaxWidth(value) => self.max_width = Some(value),
            CompiledDeclaration::MaxHeight(value) => self.max_height = Some(value),
            CompiledDeclaration::AlignItems(value) => self.align_items = Some(value),
            CompiledDeclaration::AlignSelf(value) => self.align_self = Some(value),
            CompiledDeclaration::JustifyItems(value) => self.justify_items = Some(value),
            CompiledDeclaration::JustifySelf(value) => self.justify_self = Some(value),
            CompiledDeclaration::AlignContent(value) => self.align_content = Some(value),
            CompiledDeclaration::JustifyContent(value) => self.justify_content = Some(value),
            CompiledDeclaration::Gap(value) => match &mut self.gap {
                Some(existing) => {
                    if !matches!(value.width, LengthPercentage::Px(px) if px == 0.0) {
                        existing.width = value.width;
                    }
                    if !matches!(value.height, LengthPercentage::Px(px) if px == 0.0) {
                        existing.height = value.height;
                    }
                }
                None => self.gap = Some(value),
            },
            CompiledDeclaration::GridTemplateRows(value) => self.grid_template_rows = Some(value),
            CompiledDeclaration::GridTemplateColumns(value) => {
                self.grid_template_columns = Some(value)
            }
            CompiledDeclaration::GridAutoRows(value) => self.grid_auto_rows = Some(value),
            CompiledDeclaration::GridAutoColumns(value) => self.grid_auto_columns = Some(value),
            CompiledDeclaration::GridAutoFlow(value) => self.grid_auto_flow = Some(value),
            CompiledDeclaration::GridTemplateAreas(value) => self.grid_template_areas = Some(value),
            CompiledDeclaration::GridRow(value) => merge_grid_line(&mut self.grid_row, value),
            CompiledDeclaration::GridColumn(value) => merge_grid_line(&mut self.grid_column, value),
            CompiledDeclaration::Border(value) => self.border = Some(value),
            CompiledDeclaration::BorderSide(side, value) => {
                let mut border = self.border.unwrap_or(BoxEdges {
                    top: LengthPercentage::Px(0.0),
                    right: LengthPercentage::Px(0.0),
                    bottom: LengthPercentage::Px(0.0),
                    left: LengthPercentage::Px(0.0),
                });
                match side {
                    BoxSide::Top => border.top = value,
                    BoxSide::Right => border.right = value,
                    BoxSide::Bottom => border.bottom = value,
                    BoxSide::Left => border.left = value,
                }
                self.border = Some(border);
            }
            CompiledDeclaration::Padding(value) => self.padding = Some(value),
            CompiledDeclaration::PaddingSide(side, value) => {
                let mut padding = self.padding.unwrap_or(BoxEdges {
                    top: LengthPercentage::Px(0.0),
                    right: LengthPercentage::Px(0.0),
                    bottom: LengthPercentage::Px(0.0),
                    left: LengthPercentage::Px(0.0),
                });
                match side {
                    BoxSide::Top => padding.top = value,
                    BoxSide::Right => padding.right = value,
                    BoxSide::Bottom => padding.bottom = value,
                    BoxSide::Left => padding.left = value,
                }
                self.padding = Some(padding);
            }
            CompiledDeclaration::Margin(value) => self.margin = Some(value),
            CompiledDeclaration::MarginSide(side, value) => {
                let mut margin = self.margin.unwrap_or(BoxEdges {
                    top: SizeValue::Auto,
                    right: SizeValue::Auto,
                    bottom: SizeValue::Auto,
                    left: SizeValue::Auto,
                });
                match side {
                    BoxSide::Top => margin.top = value,
                    BoxSide::Right => margin.right = value,
                    BoxSide::Bottom => margin.bottom = value,
                    BoxSide::Left => margin.left = value,
                }
                self.margin = Some(margin);
            }
        }
    }
}

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

pub(super) fn merge_grid_line(
    target: &mut Option<Line<GridPlacementValue>>,
    value: Line<GridPlacementValue>,
) {
    match target {
        Some(existing) => {
            if !matches!(value.start, GridPlacementValue::Auto) {
                existing.start = value.start;
            }
            if !matches!(value.end, GridPlacementValue::Auto) {
                existing.end = value.end;
            }
        }
        None => *target = Some(value),
    }
}

pub(super) fn map_display(display: Display) -> TaffyDisplay {
    match display {
        Display::Block => TaffyDisplay::Block,
        Display::Flex => TaffyDisplay::Flex,
        Display::Grid => TaffyDisplay::Grid,
        Display::None => TaffyDisplay::None,
    }
}

pub(super) fn map_box_sizing(box_sizing: BoxSizingValue) -> TaffyBoxSizing {
    match box_sizing {
        BoxSizingValue::BorderBox => TaffyBoxSizing::BorderBox,
        BoxSizingValue::ContentBox => TaffyBoxSizing::ContentBox,
    }
}

pub(super) fn map_flex_direction(direction: FlexDirectionValue) -> TaffyFlexDirection {
    match direction {
        FlexDirectionValue::Row => TaffyFlexDirection::Row,
        FlexDirectionValue::Column => TaffyFlexDirection::Column,
        FlexDirectionValue::RowReverse => TaffyFlexDirection::RowReverse,
        FlexDirectionValue::ColumnReverse => TaffyFlexDirection::ColumnReverse,
    }
}

pub(super) fn map_flex_wrap(wrap: FlexWrapValue) -> TaffyFlexWrap {
    match wrap {
        FlexWrapValue::NoWrap => TaffyFlexWrap::NoWrap,
        FlexWrapValue::Wrap => TaffyFlexWrap::Wrap,
        FlexWrapValue::WrapReverse => TaffyFlexWrap::WrapReverse,
    }
}

pub(super) fn map_position(position: PositionValue) -> TaffyPosition {
    match position {
        PositionValue::Relative => TaffyPosition::Relative,
        PositionValue::Absolute => TaffyPosition::Absolute,
    }
}

pub(super) fn map_overflow(overflow: OverflowValue) -> TaffyOverflow {
    match overflow {
        OverflowValue::Visible => TaffyOverflow::Visible,
        OverflowValue::Clip => TaffyOverflow::Clip,
        OverflowValue::Hidden => TaffyOverflow::Hidden,
        OverflowValue::Scroll => TaffyOverflow::Scroll,
    }
}

pub(super) fn map_align_items(value: AlignmentValue) -> TaffyAlignItems {
    match value {
        AlignmentValue::Start => TaffyAlignItems::Start,
        AlignmentValue::End => TaffyAlignItems::End,
        AlignmentValue::FlexStart => TaffyAlignItems::FlexStart,
        AlignmentValue::FlexEnd => TaffyAlignItems::FlexEnd,
        AlignmentValue::Center => TaffyAlignItems::Center,
        AlignmentValue::Baseline => TaffyAlignItems::Baseline,
        AlignmentValue::Stretch => TaffyAlignItems::Stretch,
    }
}

pub(super) fn map_align_content(value: ContentAlignmentValue) -> TaffyAlignContent {
    match value {
        ContentAlignmentValue::Start => TaffyAlignContent::Start,
        ContentAlignmentValue::End => TaffyAlignContent::End,
        ContentAlignmentValue::FlexStart => TaffyAlignContent::FlexStart,
        ContentAlignmentValue::FlexEnd => TaffyAlignContent::FlexEnd,
        ContentAlignmentValue::Center => TaffyAlignContent::Center,
        ContentAlignmentValue::Stretch => TaffyAlignContent::Stretch,
        ContentAlignmentValue::SpaceBetween => TaffyAlignContent::SpaceBetween,
        ContentAlignmentValue::SpaceEvenly => TaffyAlignContent::SpaceEvenly,
        ContentAlignmentValue::SpaceAround => TaffyAlignContent::SpaceAround,
    }
}

pub(super) fn map_justify_content(value: ContentAlignmentValue) -> TaffyJustifyContent {
    match value {
        ContentAlignmentValue::Start => TaffyJustifyContent::Start,
        ContentAlignmentValue::End => TaffyJustifyContent::End,
        ContentAlignmentValue::FlexStart => TaffyJustifyContent::FlexStart,
        ContentAlignmentValue::FlexEnd => TaffyJustifyContent::FlexEnd,
        ContentAlignmentValue::Center => TaffyJustifyContent::Center,
        ContentAlignmentValue::Stretch => TaffyJustifyContent::Stretch,
        ContentAlignmentValue::SpaceBetween => TaffyJustifyContent::SpaceBetween,
        ContentAlignmentValue::SpaceEvenly => TaffyJustifyContent::SpaceEvenly,
        ContentAlignmentValue::SpaceAround => TaffyJustifyContent::SpaceAround,
    }
}

pub(super) fn map_size_value(value: SizeValue) -> TaffyDimension {
    match value {
        SizeValue::Auto => TaffyDimension::auto(),
        SizeValue::LengthPercentage(value) => match value {
            LengthPercentage::Px(value) => TaffyDimension::length(value),
            LengthPercentage::Percent(value) => TaffyDimension::percent(value / 100.0),
        },
    }
}

pub(super) fn map_size_value_auto(value: SizeValue) -> TaffyLengthPercentageAuto {
    match value {
        SizeValue::Auto => TaffyLengthPercentageAuto::AUTO,
        SizeValue::LengthPercentage(value) => map_length_percentage_auto(value),
    }
}

pub(super) fn map_length_percentage(value: LengthPercentage) -> TaffyLengthPercentage {
    match value {
        LengthPercentage::Px(value) => TaffyLengthPercentage::length(value),
        LengthPercentage::Percent(value) => TaffyLengthPercentage::percent(value / 100.0),
    }
}

pub(super) fn map_length_percentage_auto(value: LengthPercentage) -> TaffyLengthPercentageAuto {
    match value {
        LengthPercentage::Px(value) => TaffyLengthPercentageAuto::length(value),
        LengthPercentage::Percent(value) => TaffyLengthPercentageAuto::percent(value / 100.0),
    }
}

pub(super) fn map_box_edges<T, U>(edges: BoxEdges<T>, map: fn(T) -> U) -> TaffyRect<U> {
    TaffyRect {
        left: map(edges.left),
        right: map(edges.right),
        top: map(edges.top),
        bottom: map(edges.bottom),
    }
}

pub(super) fn map_grid_auto_flow(flow: GridAutoFlow) -> TaffyGridAutoFlow {
    match flow {
        GridAutoFlow::Row => TaffyGridAutoFlow::Row,
        GridAutoFlow::Column => TaffyGridAutoFlow::Column,
        GridAutoFlow::RowDense => TaffyGridAutoFlow::RowDense,
        GridAutoFlow::ColumnDense => TaffyGridAutoFlow::ColumnDense,
    }
}

pub(super) fn map_grid_line(value: Line<GridPlacementValue>) -> TaffyLine<TaffyGridPlacement> {
    TaffyLine {
        start: map_grid_placement(value.start),
        end: map_grid_placement(value.end),
    }
}

pub(super) fn map_grid_placement(value: GridPlacementValue) -> TaffyGridPlacement {
    match value {
        GridPlacementValue::Auto => TaffyGridPlacement::Auto,
        GridPlacementValue::Line(line) => TaffyGridPlacement::Line(line.into()),
        GridPlacementValue::NamedLine(name, index) => TaffyGridPlacement::NamedLine(name, index),
        GridPlacementValue::Span(span) => TaffyGridPlacement::Span(span),
        GridPlacementValue::NamedSpan(name, span) => TaffyGridPlacement::NamedSpan(name, span),
    }
}

pub(super) fn map_grid_template_component(
    value: &GridTemplateComponent,
) -> TaffyGridTemplateComponent<String> {
    match value {
        GridTemplateComponent::Single(track) => {
            TaffyGridTemplateComponent::Single(map_grid_track_sizing_function(*track))
        }
        GridTemplateComponent::Repeat(repetition) => {
            TaffyGridTemplateComponent::Repeat(map_grid_track_repeat(repetition))
        }
    }
}

pub(super) fn map_grid_track_repeat(
    repetition: &GridTrackRepeat,
) -> TaffyGridTemplateRepetition<String> {
    TaffyGridTemplateRepetition {
        count: map_grid_repetition_count(repetition.count),
        tracks: repetition
            .tracks
            .iter()
            .copied()
            .map(map_grid_track_sizing_function)
            .collect(),
        line_names: repetition.line_names.clone(),
    }
}

pub(super) fn map_grid_repetition_count(value: GridRepetitionCount) -> TaffyRepetitionCount {
    match value {
        GridRepetitionCount::AutoFill => TaffyRepetitionCount::AutoFill,
        GridRepetitionCount::AutoFit => TaffyRepetitionCount::AutoFit,
        GridRepetitionCount::Count(count) => TaffyRepetitionCount::Count(count),
    }
}

pub(super) fn map_grid_template_area(area: &GridTemplateArea) -> TaffyGridTemplateArea<String> {
    TaffyGridTemplateArea {
        name: area.name.clone(),
        row_start: area.row_start,
        row_end: area.row_end,
        column_start: area.column_start,
        column_end: area.column_end,
    }
}

pub(super) fn map_grid_track_sizing_function(value: GridTrackValue) -> TaffyTrackSizingFunction {
    match value {
        GridTrackValue::Auto => TaffyTrackSizingFunction::AUTO,
        GridTrackValue::MinContent => TaffyTrackSizingFunction::MIN_CONTENT,
        GridTrackValue::MaxContent => TaffyTrackSizingFunction::MAX_CONTENT,
        GridTrackValue::LengthPercentage(value) => match value {
            LengthPercentage::Px(value) => TaffyTrackSizingFunction::from_length(value),
            LengthPercentage::Percent(value) => {
                TaffyTrackSizingFunction::from_percent(value / 100.0)
            }
        },
        GridTrackValue::Fraction(value) => TaffyTrackSizingFunction::from_fr(value),
        GridTrackValue::FitContent(value) => {
            TaffyTrackSizingFunction::fit_content(map_track_length_percentage(value))
        }
        GridTrackValue::MinMax(min, max) => TaffyTrackSizingFunction {
            min: map_grid_track_min_value(min),
            max: map_grid_track_max_value(max),
        },
    }
}

pub(super) fn map_grid_track_min_value(value: GridTrackMinValue) -> TaffyMinTrackSizingFunction {
    match value {
        GridTrackMinValue::Auto => TaffyMinTrackSizingFunction::AUTO,
        GridTrackMinValue::MinContent => TaffyMinTrackSizingFunction::MIN_CONTENT,
        GridTrackMinValue::MaxContent => TaffyMinTrackSizingFunction::MAX_CONTENT,
        GridTrackMinValue::LengthPercentage(value) => match value {
            LengthPercentage::Px(value) => TaffyMinTrackSizingFunction::length(value),
            LengthPercentage::Percent(value) => TaffyMinTrackSizingFunction::percent(value / 100.0),
        },
    }
}

pub(super) fn map_grid_track_max_value(value: GridTrackMaxValue) -> TaffyMaxTrackSizingFunction {
    match value {
        GridTrackMaxValue::Auto => TaffyMaxTrackSizingFunction::AUTO,
        GridTrackMaxValue::MinContent => TaffyMaxTrackSizingFunction::MIN_CONTENT,
        GridTrackMaxValue::MaxContent => TaffyMaxTrackSizingFunction::MAX_CONTENT,
        GridTrackMaxValue::LengthPercentage(value) => match value {
            LengthPercentage::Px(value) => TaffyMaxTrackSizingFunction::length(value),
            LengthPercentage::Percent(value) => TaffyMaxTrackSizingFunction::percent(value / 100.0),
        },
        GridTrackMaxValue::Fraction(value) => TaffyMaxTrackSizingFunction::fr(value),
        GridTrackMaxValue::FitContent(value) => {
            TaffyMaxTrackSizingFunction::fit_content(map_track_length_percentage(value))
        }
    }
}

pub(super) fn map_track_length_percentage(value: LengthPercentage) -> TaffyTrackLengthPercentage {
    match value {
        LengthPercentage::Px(value) => TaffyTrackLengthPercentage::from_length(value),
        LengthPercentage::Percent(value) => TaffyTrackLengthPercentage::from_percent(value / 100.0),
    }
}
