use std::collections::BTreeMap;

use spiders_shared::layout::ResolvedLayoutNode;
use taffy::geometry::{Line as TaffyLine, Rect as TaffyRect, Size as TaffySize};
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
    Overflow as TaffyOverflow, Style as TaffyStyle,
};
use thiserror::Error;

use super::domain::{
    AlignmentValue, BoxEdges, BoxSizingValue, ComputedStyle, ContentAlignmentValue, CssDelimiter,
    CssDimension, CssFunction, CssSimpleBlock, CssSimpleBlockKind, CssValue, CssValueToken,
    Declaration, Display, FlexDirectionValue, FlexWrapValue, GridAutoFlow, GridPlacementValue,
    GridRepetitionCount, GridTemplate, GridTemplateArea, GridTemplateComponent, GridTrackMaxValue,
    GridTrackMinValue, GridTrackRepeat, GridTrackValue, LengthPercentage, Line, MatchedRule,
    NodeSelector, OverflowValue, PositionValue, Selector, Size2, SizeValue, StyleSheet,
};

#[derive(Debug, Error, PartialEq)]
pub enum CssValueError {
    #[error("unsupported value `{value}` for property `{property}`")]
    UnsupportedValue { property: String, value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeComputedStyle {
    pub node: ResolvedLayoutNode,
    pub computed: ComputedStyle,
    pub taffy_style: TaffyStyle,
    pub children: Vec<NodeComputedStyle>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyledLayoutTree {
    pub root: NodeComputedStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompiledDeclaration {
    Display(Display),
    BoxSizing(BoxSizingValue),
    AspectRatio(f32),
    FlexDirection(FlexDirectionValue),
    FlexWrap(FlexWrapValue),
    FlexGrow(f32),
    FlexShrink(f32),
    FlexBasis(SizeValue),
    Position(PositionValue),
    Inset(BoxEdges<SizeValue>),
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
    Padding(BoxEdges<LengthPercentage>),
    Margin(BoxEdges<SizeValue>),
}

pub fn matching_rules<'a>(
    sheet: &'a StyleSheet,
    node: &ResolvedLayoutNode,
) -> Vec<MatchedRule<'a>> {
    sheet
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            rule.selectors
                .iter()
                .any(|selector| selector_matches(selector, node))
        })
        .map(|(rule_index, rule)| MatchedRule { rule_index, rule })
        .collect()
}

pub fn compute_style(
    sheet: &StyleSheet,
    node: &ResolvedLayoutNode,
) -> Result<ComputedStyle, CssValueError> {
    let mut style = ComputedStyle::default();

    for matched_rule in matching_rules(sheet, node) {
        for declaration in &matched_rule.rule.declarations {
            style.apply(compile_declaration(declaration)?);
        }
    }

    Ok(style)
}

pub fn selector_matches(selector: &Selector, node: &ResolvedLayoutNode) -> bool {
    let meta = node.meta();

    match selector {
        Selector::Type(node_selector) => node_selector_matches(*node_selector, node),
        Selector::Id(id) => meta.id.as_deref() == Some(id.as_str()),
        Selector::Class(class) => meta.class.iter().any(|value| value == class),
        Selector::Attribute(attribute) => meta.data.get(&attribute.name) == Some(&attribute.value),
    }
}

fn node_selector_matches(selector: NodeSelector, node: &ResolvedLayoutNode) -> bool {
    matches!(
        (selector, node.node_type()),
        (
            NodeSelector::Workspace,
            spiders_shared::layout::RuntimeLayoutNodeType::Workspace
        ) | (
            NodeSelector::Group,
            spiders_shared::layout::RuntimeLayoutNodeType::Group
        ) | (
            NodeSelector::Window,
            spiders_shared::layout::RuntimeLayoutNodeType::Window
        )
    )
}

pub fn map_computed_style_to_taffy(style: &ComputedStyle) -> TaffyStyle {
    let mut taffy_style = TaffyStyle::default();

    if let Some(display) = style.display {
        taffy_style.display = map_display(display);
    }
    if let Some(box_sizing) = style.box_sizing {
        taffy_style.box_sizing = map_box_sizing(box_sizing);
    }
    if let Some(aspect_ratio) = style.aspect_ratio {
        taffy_style.aspect_ratio = Some(aspect_ratio);
    }
    if let Some(direction) = style.flex_direction {
        taffy_style.flex_direction = map_flex_direction(direction);
    }
    if let Some(wrap) = style.flex_wrap {
        taffy_style.flex_wrap = map_flex_wrap(wrap);
    }
    if let Some(flex_grow) = style.flex_grow {
        taffy_style.flex_grow = flex_grow;
    }
    if let Some(flex_shrink) = style.flex_shrink {
        taffy_style.flex_shrink = flex_shrink;
    }
    if let Some(flex_basis) = style.flex_basis {
        taffy_style.flex_basis = map_size_value(flex_basis);
    }
    if let Some(position) = style.position {
        taffy_style.position = map_position(position);
    }
    if let Some(inset) = style.inset {
        taffy_style.inset = map_box_edges(inset, map_size_value_auto);
    }
    if style.overflow_x.is_some() || style.overflow_y.is_some() {
        taffy_style.overflow = taffy::geometry::Point {
            x: style
                .overflow_x
                .map(map_overflow)
                .unwrap_or(TaffyOverflow::Visible),
            y: style
                .overflow_y
                .map(map_overflow)
                .unwrap_or(TaffyOverflow::Visible),
        };
    }
    if style.width.is_some() || style.height.is_some() {
        taffy_style.size = TaffySize {
            width: style
                .width
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
            height: style
                .height
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
        };
    }
    if style.min_width.is_some() || style.min_height.is_some() {
        taffy_style.min_size = TaffySize {
            width: style
                .min_width
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
            height: style
                .min_height
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
        };
    }
    if style.max_width.is_some() || style.max_height.is_some() {
        taffy_style.max_size = TaffySize {
            width: style
                .max_width
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
            height: style
                .max_height
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
        };
    }
    if let Some(gap) = style.gap {
        taffy_style.gap = TaffySize {
            width: map_length_percentage(gap.width),
            height: map_length_percentage(gap.height),
        };
    }
    if let Some(align_items) = style.align_items {
        taffy_style.align_items = Some(map_align_items(align_items));
    }
    if let Some(align_self) = style.align_self {
        taffy_style.align_self = Some(map_align_items(align_self));
    }
    if let Some(justify_items) = style.justify_items {
        taffy_style.justify_items = Some(map_align_items(justify_items));
    }
    if let Some(justify_self) = style.justify_self {
        taffy_style.justify_self = Some(map_align_items(justify_self));
    }
    if let Some(align_content) = style.align_content {
        taffy_style.align_content = Some(map_align_content(align_content));
    }
    if let Some(justify_content) = style.justify_content {
        taffy_style.justify_content = Some(map_justify_content(justify_content));
    }
    if let Some(tracks) = &style.grid_template_rows {
        taffy_style.grid_template_rows = tracks
            .components
            .iter()
            .map(map_grid_template_component)
            .collect();
        taffy_style.grid_template_row_names = tracks.line_names.clone();
    }
    if let Some(tracks) = &style.grid_template_columns {
        taffy_style.grid_template_columns = tracks
            .components
            .iter()
            .map(map_grid_template_component)
            .collect();
        taffy_style.grid_template_column_names = tracks.line_names.clone();
    }
    if let Some(tracks) = &style.grid_auto_rows {
        taffy_style.grid_auto_rows = tracks
            .iter()
            .copied()
            .map(map_grid_track_sizing_function)
            .collect();
    }
    if let Some(tracks) = &style.grid_auto_columns {
        taffy_style.grid_auto_columns = tracks
            .iter()
            .copied()
            .map(map_grid_track_sizing_function)
            .collect();
    }
    if let Some(flow) = style.grid_auto_flow {
        taffy_style.grid_auto_flow = map_grid_auto_flow(flow);
    }
    if let Some(areas) = &style.grid_template_areas {
        taffy_style.grid_template_areas = areas.iter().map(map_grid_template_area).collect();
    }
    if let Some(grid_row) = &style.grid_row {
        taffy_style.grid_row = map_grid_line(grid_row.clone());
    }
    if let Some(grid_column) = &style.grid_column {
        taffy_style.grid_column = map_grid_line(grid_column.clone());
    }
    if let Some(border) = style.border {
        taffy_style.border = map_box_edges(border, map_length_percentage);
    }
    if let Some(padding) = style.padding {
        taffy_style.padding = map_box_edges(padding, map_length_percentage);
    }
    if let Some(margin) = style.margin {
        taffy_style.margin = map_box_edges(margin, map_size_value_auto);
    }

    taffy_style
}

pub fn compile_declaration(
    declaration: &Declaration,
) -> Result<CompiledDeclaration, CssValueError> {
    let property = declaration.property.as_str();
    let value = &declaration.value;

    match property {
        "display" => Ok(CompiledDeclaration::Display(parse_display(
            property, value,
        )?)),
        "box-sizing" => Ok(CompiledDeclaration::BoxSizing(parse_box_sizing(
            property, value,
        )?)),
        "aspect-ratio" => Ok(CompiledDeclaration::AspectRatio(parse_aspect_ratio(
            property, value,
        )?)),
        "flex-direction" => Ok(CompiledDeclaration::FlexDirection(parse_flex_direction(
            property, value,
        )?)),
        "flex-wrap" => Ok(CompiledDeclaration::FlexWrap(parse_flex_wrap(
            property, value,
        )?)),
        "flex-grow" => Ok(CompiledDeclaration::FlexGrow(parse_number(
            property, value,
        )?)),
        "flex-shrink" => Ok(CompiledDeclaration::FlexShrink(parse_number(
            property, value,
        )?)),
        "flex-basis" => Ok(CompiledDeclaration::FlexBasis(parse_size_value(
            property, value,
        )?)),
        "position" => Ok(CompiledDeclaration::Position(parse_position(
            property, value,
        )?)),
        "inset" => Ok(CompiledDeclaration::Inset(parse_box_edges_size(
            property, value,
        )?)),
        "top" | "right" | "bottom" | "left" => Ok(CompiledDeclaration::Inset(parse_inset_side(
            property, value,
        )?)),
        "overflow" => {
            let (x, y) = parse_overflow_pair(property, value)?;
            Ok(CompiledDeclaration::Overflow(x, y))
        }
        "overflow-x" => Ok(CompiledDeclaration::OverflowX(parse_overflow(
            property, value,
        )?)),
        "overflow-y" => Ok(CompiledDeclaration::OverflowY(parse_overflow(
            property, value,
        )?)),
        "width" => Ok(CompiledDeclaration::Width(parse_size_value(
            property, value,
        )?)),
        "height" => Ok(CompiledDeclaration::Height(parse_size_value(
            property, value,
        )?)),
        "min-width" => Ok(CompiledDeclaration::MinWidth(parse_size_value(
            property, value,
        )?)),
        "min-height" => Ok(CompiledDeclaration::MinHeight(parse_size_value(
            property, value,
        )?)),
        "max-width" => Ok(CompiledDeclaration::MaxWidth(parse_size_value(
            property, value,
        )?)),
        "max-height" => Ok(CompiledDeclaration::MaxHeight(parse_size_value(
            property, value,
        )?)),
        "align-items" => Ok(CompiledDeclaration::AlignItems(parse_alignment(
            property, value,
        )?)),
        "align-self" => Ok(CompiledDeclaration::AlignSelf(parse_alignment(
            property, value,
        )?)),
        "justify-items" => Ok(CompiledDeclaration::JustifyItems(parse_alignment(
            property, value,
        )?)),
        "justify-self" => Ok(CompiledDeclaration::JustifySelf(parse_alignment(
            property, value,
        )?)),
        "align-content" => Ok(CompiledDeclaration::AlignContent(parse_content_alignment(
            property, value,
        )?)),
        "justify-content" => Ok(CompiledDeclaration::JustifyContent(
            parse_content_alignment(property, value)?,
        )),
        "gap" => Ok(CompiledDeclaration::Gap(parse_gap(property, value)?)),
        "row-gap" => Ok(CompiledDeclaration::Gap(parse_axis_gap(
            property, value, true,
        )?)),
        "column-gap" => Ok(CompiledDeclaration::Gap(parse_axis_gap(
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
        "grid-auto-flow" => Ok(CompiledDeclaration::GridAutoFlow(parse_grid_auto_flow(
            property, value,
        )?)),
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
        "border-width" => Ok(CompiledDeclaration::Border(parse_box_edges(
            property, value,
        )?)),
        "padding" => Ok(CompiledDeclaration::Padding(parse_box_edges(
            property, value,
        )?)),
        "margin" => Ok(CompiledDeclaration::Margin(parse_box_edges_size(
            property, value,
        )?)),
        _ => Err(CssValueError::UnsupportedValue {
            property: declaration.property.clone(),
            value: declaration.value.text.clone(),
        }),
    }
}

impl ComputedStyle {
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
            CompiledDeclaration::Padding(value) => self.padding = Some(value),
            CompiledDeclaration::Margin(value) => self.margin = Some(value),
        }
    }
}

fn text_for_value(value: &CssValue) -> &str {
    value.text.as_str()
}

fn normalized_components(value: &CssValue) -> Vec<&CssValueToken> {
    value
        .components
        .iter()
        .filter(|component| !matches!(component, CssValueToken::Whitespace))
        .collect()
}

fn split_components(value: &CssValue) -> Vec<Vec<CssValueToken>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();

    for component in &value.components {
        if matches!(component, CssValueToken::Whitespace) {
            if !current.is_empty() {
                groups.push(std::mem::take(&mut current));
            }
            continue;
        }

        current.push(component.clone());
    }

    if !current.is_empty() {
        groups.push(current);
    }

    groups
}

fn normalized_components_owned(value: &CssValue) -> Vec<CssValueToken> {
    value
        .components
        .iter()
        .filter(|component| !matches!(component, CssValueToken::Whitespace))
        .cloned()
        .collect()
}

fn parse_ident_keyword<'a>(property: &str, value: &'a CssValue) -> Result<&'a str, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Ident(ident)] => Ok(ident.as_str()),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_display(property: &str, value: &CssValue) -> Result<Display, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "block" => Ok(Display::Block),
        "flex" => Ok(Display::Flex),
        "grid" => Ok(Display::Grid),
        "none" => Ok(Display::None),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_box_sizing(property: &str, value: &CssValue) -> Result<BoxSizingValue, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "border-box" => Ok(BoxSizingValue::BorderBox),
        "content-box" => Ok(BoxSizingValue::ContentBox),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_aspect_ratio(property: &str, value: &CssValue) -> Result<f32, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Number(number)] => Ok(*number),
        [CssValueToken::Integer(number)] => Ok(*number as f32),
        [CssValueToken::Integer(width), CssValueToken::Delimiter(CssDelimiter::Solidus), CssValueToken::Integer(height)]
            if *height != 0 =>
        {
            Ok(*width as f32 / *height as f32)
        }
        [CssValueToken::Number(width), CssValueToken::Delimiter(CssDelimiter::Solidus), CssValueToken::Number(height)]
            if *height != 0.0 =>
        {
            Ok(*width / *height)
        }
        [CssValueToken::Integer(width), CssValueToken::Delimiter(CssDelimiter::Solidus), CssValueToken::Number(height)]
            if *height != 0.0 =>
        {
            Ok(*width as f32 / *height)
        }
        [CssValueToken::Number(width), CssValueToken::Delimiter(CssDelimiter::Solidus), CssValueToken::Integer(height)]
            if *height != 0 =>
        {
            Ok(*width / *height as f32)
        }
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_flex_direction(
    property: &str,
    value: &CssValue,
) -> Result<FlexDirectionValue, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "row" => Ok(FlexDirectionValue::Row),
        "column" => Ok(FlexDirectionValue::Column),
        "row-reverse" => Ok(FlexDirectionValue::RowReverse),
        "column-reverse" => Ok(FlexDirectionValue::ColumnReverse),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_flex_wrap(property: &str, value: &CssValue) -> Result<FlexWrapValue, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "nowrap" => Ok(FlexWrapValue::NoWrap),
        "wrap" => Ok(FlexWrapValue::Wrap),
        "wrap-reverse" => Ok(FlexWrapValue::WrapReverse),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_position(property: &str, value: &CssValue) -> Result<PositionValue, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "relative" => Ok(PositionValue::Relative),
        "absolute" => Ok(PositionValue::Absolute),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_overflow(property: &str, value: &CssValue) -> Result<OverflowValue, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "visible" => Ok(OverflowValue::Visible),
        "clip" => Ok(OverflowValue::Clip),
        "hidden" => Ok(OverflowValue::Hidden),
        "scroll" => Ok(OverflowValue::Scroll),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_overflow_pair(
    property: &str,
    value: &CssValue,
) -> Result<(OverflowValue, OverflowValue), CssValueError> {
    let values = split_components(value)
        .into_iter()
        .map(|components| {
            parse_overflow(
                property,
                &CssValue {
                    text: components_to_text(&components),
                    components,
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    match values.as_slice() {
        [single] => Ok((*single, *single)),
        [x, y] => Ok((*x, *y)),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_alignment(property: &str, value: &CssValue) -> Result<AlignmentValue, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "start" => Ok(AlignmentValue::Start),
        "end" => Ok(AlignmentValue::End),
        "flex-start" => Ok(AlignmentValue::FlexStart),
        "flex-end" => Ok(AlignmentValue::FlexEnd),
        "center" => Ok(AlignmentValue::Center),
        "baseline" => Ok(AlignmentValue::Baseline),
        "stretch" => Ok(AlignmentValue::Stretch),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_content_alignment(
    property: &str,
    value: &CssValue,
) -> Result<ContentAlignmentValue, CssValueError> {
    match parse_ident_keyword(property, value)? {
        "start" => Ok(ContentAlignmentValue::Start),
        "end" => Ok(ContentAlignmentValue::End),
        "flex-start" => Ok(ContentAlignmentValue::FlexStart),
        "flex-end" => Ok(ContentAlignmentValue::FlexEnd),
        "center" => Ok(ContentAlignmentValue::Center),
        "stretch" => Ok(ContentAlignmentValue::Stretch),
        "space-between" => Ok(ContentAlignmentValue::SpaceBetween),
        "space-evenly" => Ok(ContentAlignmentValue::SpaceEvenly),
        "space-around" => Ok(ContentAlignmentValue::SpaceAround),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_gap(property: &str, value: &CssValue) -> Result<Size2<LengthPercentage>, CssValueError> {
    let values = split_components(value)
        .into_iter()
        .map(|components| {
            parse_length_percentage(
                property,
                &CssValue {
                    text: components_to_text(&components),
                    components,
                },
            )
        })
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
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_axis_gap(
    property: &str,
    value: &CssValue,
    is_row: bool,
) -> Result<Size2<LengthPercentage>, CssValueError> {
    let parsed = parse_length_percentage(property, value)?;
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

fn parse_number(property: &str, value: &CssValue) -> Result<f32, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Number(number)] => Ok(*number),
        [CssValueToken::Integer(number)] => Ok(*number as f32),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_size_value(property: &str, value: &CssValue) -> Result<SizeValue, CssValueError> {
    if matches!(parse_ident_keyword(property, value), Ok("auto")) {
        return Ok(SizeValue::Auto);
    }

    Ok(SizeValue::LengthPercentage(parse_length_percentage(
        property, value,
    )?))
}

fn parse_length_percentage(
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

fn parse_dimension_length_percentage(
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

fn parse_box_edges_size(
    property: &str,
    value: &CssValue,
) -> Result<BoxEdges<SizeValue>, CssValueError> {
    let values = split_components(value)
        .into_iter()
        .map(|components| {
            parse_size_value(
                property,
                &CssValue {
                    text: components_to_text(&components),
                    components,
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    match values.as_slice() {
        [a] => Ok(BoxEdges {
            top: *a,
            right: *a,
            bottom: *a,
            left: *a,
        }),
        [vertical, horizontal] => Ok(BoxEdges {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        }),
        [top, horizontal, bottom] => Ok(BoxEdges {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        }),
        [top, right, bottom, left] => Ok(BoxEdges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_inset_side(
    property: &str,
    value: &CssValue,
) -> Result<BoxEdges<SizeValue>, CssValueError> {
    let size = parse_size_value(property, value)?;
    let auto = SizeValue::Auto;
    Ok(match property {
        "top" => BoxEdges {
            top: size,
            right: auto,
            bottom: auto,
            left: auto,
        },
        "right" => BoxEdges {
            top: auto,
            right: size,
            bottom: auto,
            left: auto,
        },
        "bottom" => BoxEdges {
            top: auto,
            right: auto,
            bottom: size,
            left: auto,
        },
        "left" => BoxEdges {
            top: auto,
            right: auto,
            bottom: auto,
            left: size,
        },
        _ => return Err(invalid_value(property, text_for_value(value))),
    })
}

fn parse_box_edges(
    property: &str,
    value: &CssValue,
) -> Result<BoxEdges<LengthPercentage>, CssValueError> {
    let values = split_components(value)
        .into_iter()
        .map(|components| {
            parse_length_percentage(
                property,
                &CssValue {
                    text: components_to_text(&components),
                    components,
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    match values.as_slice() {
        [a] => Ok(BoxEdges {
            top: *a,
            right: *a,
            bottom: *a,
            left: *a,
        }),
        [vertical, horizontal] => Ok(BoxEdges {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        }),
        [top, horizontal, bottom] => Ok(BoxEdges {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        }),
        [top, right, bottom, left] => Ok(BoxEdges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_grid_auto_flow(property: &str, value: &CssValue) -> Result<GridAutoFlow, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Ident(row)] if row == "row" => Ok(GridAutoFlow::Row),
        [CssValueToken::Ident(column)] if column == "column" => Ok(GridAutoFlow::Column),
        [CssValueToken::Ident(first), CssValueToken::Ident(second)]
            if (first == "row" && second == "dense") || (first == "dense" && second == "row") =>
        {
            Ok(GridAutoFlow::RowDense)
        }
        [CssValueToken::Ident(first), CssValueToken::Ident(second)]
            if (first == "column" && second == "dense")
                || (first == "dense" && second == "column") =>
        {
            Ok(GridAutoFlow::ColumnDense)
        }
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_grid_tracks(property: &str, value: &CssValue) -> Result<GridTemplate, CssValueError> {
    let components = normalized_components_owned(value);
    if components.is_empty() {
        return Err(invalid_value(property, text_for_value(value)));
    }

    let mut index = 0;
    let mut line_names = Vec::new();
    let mut template_components = Vec::new();
    let mut pending_line_names = Vec::new();

    while index < components.len() {
        match &components[index] {
            CssValueToken::SimpleBlock(block) if block.kind == CssSimpleBlockKind::Bracket => {
                pending_line_names.extend(parse_line_name_block(property, block)?);
                index += 1;
            }
            token => {
                let component = match token {
                    CssValueToken::Function(function) if function.name == "repeat" => {
                        GridTemplateComponent::Repeat(parse_grid_track_repeat(property, function)?)
                    }
                    _ => GridTemplateComponent::Single(parse_grid_track(
                        property,
                        &CssValue {
                            text: component_text(token),
                            components: vec![token.clone()],
                        },
                    )?),
                };

                line_names.push(std::mem::take(&mut pending_line_names));
                template_components.push(component);
                index += 1;
            }
        }
    }

    line_names.push(pending_line_names);

    Ok(GridTemplate {
        components: template_components,
        line_names,
    })
}

fn parse_grid_auto_tracks(
    property: &str,
    value: &CssValue,
) -> Result<Vec<GridTrackValue>, CssValueError> {
    let template = parse_grid_tracks(property, value)?;
    if template.line_names.iter().any(|names| !names.is_empty()) {
        return Err(invalid_value(property, text_for_value(value)));
    }

    template
        .components
        .into_iter()
        .map(|component| match component {
            GridTemplateComponent::Single(track) => Ok(track),
            GridTemplateComponent::Repeat(_) => Err(invalid_value(property, text_for_value(value))),
        })
        .collect()
}

fn parse_grid_track(property: &str, value: &CssValue) -> Result<GridTrackValue, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Ident(ident)] if ident == "auto" => Ok(GridTrackValue::Auto),
        [CssValueToken::Ident(ident)] if ident == "min-content" => Ok(GridTrackValue::MinContent),
        [CssValueToken::Ident(ident)] if ident == "max-content" => Ok(GridTrackValue::MaxContent),
        [CssValueToken::Dimension(dimension)] if dimension.unit.eq_ignore_ascii_case("fr") => {
            Ok(GridTrackValue::Fraction(dimension.value))
        }
        [CssValueToken::Dimension(_)] | [CssValueToken::Percentage(_)] => {
            parse_length_percentage(property, value).map(GridTrackValue::LengthPercentage)
        }
        [CssValueToken::Function(function)] if function.name == "fit-content" => {
            parse_length_percentage(property, &function_args_value(function))
                .map(GridTrackValue::FitContent)
        }
        [CssValueToken::Function(function)] if function.name == "minmax" => {
            let args = split_function_args(function);
            if args.len() != 2 {
                return Err(invalid_value(property, text_for_value(value)));
            }
            Ok(GridTrackValue::MinMax(
                parse_grid_track_min(property, &args[0])?,
                parse_grid_track_max(property, &args[1])?,
            ))
        }
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_line_name_block(
    property: &str,
    block: &CssSimpleBlock,
) -> Result<Vec<String>, CssValueError> {
    let names = block
        .value
        .iter()
        .filter_map(|component| match component {
            CssValueToken::Whitespace => None,
            CssValueToken::Ident(name) => Some(Ok(name.clone())),
            _ => Some(Err(invalid_value(
                property,
                &components_to_text(&block.value),
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if names.is_empty() {
        return Err(invalid_value(property, &components_to_text(&block.value)));
    }

    Ok(names)
}

fn parse_grid_track_repeat(
    property: &str,
    function: &CssFunction,
) -> Result<GridTrackRepeat, CssValueError> {
    let args = split_function_args(function);
    if args.len() < 2 {
        return Err(invalid_value(
            property,
            &format!("{}({})", function.name, components_to_text(&function.value)),
        ));
    }

    let count = parse_grid_repetition_count(property, &args[0])?;
    let tracks = parse_grid_tracks(
        property,
        &CssValue {
            text: args[1..]
                .iter()
                .map(|arg| arg.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            components: args[1..]
                .iter()
                .flat_map(|arg| {
                    let mut parts = arg.components.clone();
                    parts.push(CssValueToken::Whitespace);
                    parts
                })
                .collect(),
        },
    )?;

    Ok(GridTrackRepeat {
        count,
        tracks: tracks
            .components
            .into_iter()
            .map(|component| match component {
                GridTemplateComponent::Single(track) => Ok(track),
                GridTemplateComponent::Repeat(_) => Err(invalid_value(
                    property,
                    &components_to_text(&function.value),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?,
        line_names: tracks.line_names,
    })
}

fn parse_grid_repetition_count(
    property: &str,
    value: &CssValue,
) -> Result<GridRepetitionCount, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Ident(ident)] if ident == "auto-fill" => Ok(GridRepetitionCount::AutoFill),
        [CssValueToken::Ident(ident)] if ident == "auto-fit" => Ok(GridRepetitionCount::AutoFit),
        [CssValueToken::Integer(count)] => u16::try_from(*count)
            .map(GridRepetitionCount::Count)
            .map_err(|_| invalid_value(property, text_for_value(value))),
        [CssValueToken::Number(count)] if count.fract() == 0.0 => u16::try_from(*count as i64)
            .map(GridRepetitionCount::Count)
            .map_err(|_| invalid_value(property, text_for_value(value))),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_grid_track_min(
    property: &str,
    value: &CssValue,
) -> Result<GridTrackMinValue, CssValueError> {
    match parse_grid_track(property, value)? {
        GridTrackValue::Auto => Ok(GridTrackMinValue::Auto),
        GridTrackValue::MinContent => Ok(GridTrackMinValue::MinContent),
        GridTrackValue::MaxContent => Ok(GridTrackMinValue::MaxContent),
        GridTrackValue::LengthPercentage(value) => Ok(GridTrackMinValue::LengthPercentage(value)),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_grid_track_max(
    property: &str,
    value: &CssValue,
) -> Result<GridTrackMaxValue, CssValueError> {
    match parse_grid_track(property, value)? {
        GridTrackValue::Auto => Ok(GridTrackMaxValue::Auto),
        GridTrackValue::MinContent => Ok(GridTrackMaxValue::MinContent),
        GridTrackValue::MaxContent => Ok(GridTrackMaxValue::MaxContent),
        GridTrackValue::LengthPercentage(value) => Ok(GridTrackMaxValue::LengthPercentage(value)),
        GridTrackValue::Fraction(value) => Ok(GridTrackMaxValue::Fraction(value)),
        GridTrackValue::FitContent(value) => Ok(GridTrackMaxValue::FitContent(value)),
        GridTrackValue::MinMax(_, _) => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_grid_line_shorthand(
    property: &str,
    value: &CssValue,
) -> Result<Line<GridPlacementValue>, CssValueError> {
    let components = normalized_components(value);
    let slash = components
        .iter()
        .position(|component| matches!(component, CssValueToken::Delimiter(CssDelimiter::Solidus)));

    match slash {
        Some(index) => Ok(Line {
            start: parse_grid_placement(property, &slice_to_value(&components[..index]))?,
            end: parse_grid_placement(property, &slice_to_value(&components[index + 1..]))?,
        }),
        None => Ok(Line {
            start: parse_grid_placement(property, &slice_to_value(&components))?,
            end: GridPlacementValue::Auto,
        }),
    }
}

fn parse_grid_line_side(
    property: &str,
    value: &CssValue,
) -> Result<Line<GridPlacementValue>, CssValueError> {
    let placement = parse_grid_placement(property, value)?;
    Ok(match property {
        "grid-row-start" | "grid-column-start" => Line {
            start: placement,
            end: GridPlacementValue::Auto,
        },
        "grid-row-end" | "grid-column-end" => Line {
            start: GridPlacementValue::Auto,
            end: placement,
        },
        _ => return Err(invalid_value(property, text_for_value(value))),
    })
}

fn parse_grid_placement(
    property: &str,
    value: &CssValue,
) -> Result<GridPlacementValue, CssValueError> {
    let components = normalized_components(value);
    match components.as_slice() {
        [CssValueToken::Ident(ident)] if ident == "auto" => Ok(GridPlacementValue::Auto),
        [CssValueToken::Ident(span), CssValueToken::Integer(number)] if span == "span" => {
            u16::try_from(*number)
                .map(GridPlacementValue::Span)
                .map_err(|_| invalid_value(property, text_for_value(value)))
        }
        [CssValueToken::Ident(span), CssValueToken::Ident(name)] if span == "span" => {
            Ok(GridPlacementValue::NamedSpan(name.clone(), 1))
        }
        [CssValueToken::Ident(span), CssValueToken::Integer(number), CssValueToken::Ident(name)]
            if span == "span" =>
        {
            u16::try_from(*number)
                .map(|count| GridPlacementValue::NamedSpan(name.clone(), count))
                .map_err(|_| invalid_value(property, text_for_value(value)))
        }
        [CssValueToken::Ident(span), CssValueToken::Number(number)]
            if span == "span" && number.fract() == 0.0 =>
        {
            u16::try_from(*number as i64)
                .map(GridPlacementValue::Span)
                .map_err(|_| invalid_value(property, text_for_value(value)))
        }
        [CssValueToken::Ident(span), CssValueToken::Number(number), CssValueToken::Ident(name)]
            if span == "span" && number.fract() == 0.0 =>
        {
            u16::try_from(*number as i64)
                .map(|count| GridPlacementValue::NamedSpan(name.clone(), count))
                .map_err(|_| invalid_value(property, text_for_value(value)))
        }
        [CssValueToken::Ident(name)] => Ok(GridPlacementValue::NamedLine(name.clone(), 1)),
        [CssValueToken::Ident(name), CssValueToken::Integer(index)] => i16::try_from(*index)
            .map(|line_index| GridPlacementValue::NamedLine(name.clone(), line_index))
            .map_err(|_| invalid_value(property, text_for_value(value))),
        [CssValueToken::Integer(index), CssValueToken::Ident(name)] => i16::try_from(*index)
            .map(|line_index| GridPlacementValue::NamedLine(name.clone(), line_index))
            .map_err(|_| invalid_value(property, text_for_value(value))),
        [CssValueToken::Integer(number)] => i16::try_from(*number)
            .map(GridPlacementValue::Line)
            .map_err(|_| invalid_value(property, text_for_value(value))),
        [CssValueToken::Number(number)] if number.fract() == 0.0 => i16::try_from(*number as i64)
            .map(GridPlacementValue::Line)
            .map_err(|_| invalid_value(property, text_for_value(value))),
        _ => Err(invalid_value(property, text_for_value(value))),
    }
}

fn parse_grid_template_areas(
    property: &str,
    value: &CssValue,
) -> Result<Vec<GridTemplateArea>, CssValueError> {
    let rows = normalized_components(value)
        .into_iter()
        .map(|component| match component {
            CssValueToken::String(row) => Ok(row),
            _ => Err(invalid_value(property, text_for_value(value))),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        return Err(invalid_value(property, text_for_value(value)));
    }

    let mut cells = BTreeMap::<String, Vec<(u16, u16)>>::new();
    let mut columns_per_row = None;

    for (row_index, row) in rows.iter().enumerate() {
        let columns = row.split_whitespace().collect::<Vec<_>>();
        if columns.is_empty() {
            return Err(invalid_value(property, text_for_value(value)));
        }

        match columns_per_row {
            Some(expected) if expected != columns.len() => {
                return Err(invalid_value(property, text_for_value(value)));
            }
            None => columns_per_row = Some(columns.len()),
            _ => {}
        }

        for (column_index, name) in columns.into_iter().enumerate() {
            if name == "." {
                continue;
            }

            cells
                .entry(name.to_owned())
                .or_default()
                .push((row_index as u16 + 1, column_index as u16 + 1));
        }
    }

    cells
        .into_iter()
        .map(|(name, cells)| {
            let row_start = cells.iter().map(|(row, _)| *row).min().unwrap();
            let row_end = cells.iter().map(|(row, _)| *row).max().unwrap() + 1;
            let column_start = cells.iter().map(|(_, column)| *column).min().unwrap();
            let column_end = cells.iter().map(|(_, column)| *column).max().unwrap() + 1;

            let expected =
                ((row_end - row_start) as usize) * ((column_end - column_start) as usize);
            if expected != cells.len() {
                return Err(invalid_value(property, text_for_value(value)));
            }

            for row in row_start..row_end {
                for column in column_start..column_end {
                    if !cells.contains(&(row, column)) {
                        return Err(invalid_value(property, text_for_value(value)));
                    }
                }
            }

            Ok(GridTemplateArea {
                name,
                row_start,
                row_end,
                column_start,
                column_end,
            })
        })
        .collect()
}

fn function_args_value(function: &CssFunction) -> CssValue {
    CssValue {
        text: components_to_text(&function.value),
        components: function.value.clone(),
    }
}

fn split_function_args(function: &CssFunction) -> Vec<CssValue> {
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

fn slice_to_value(components: &[&CssValueToken]) -> CssValue {
    let owned = components
        .iter()
        .map(|component| (*component).clone())
        .collect::<Vec<_>>();
    CssValue {
        text: components_to_text(&owned),
        components: owned,
    }
}

fn components_to_text(components: &[CssValueToken]) -> String {
    let mut output = String::new();
    for component in components {
        output.push_str(&component_text(component));
    }
    output.trim().to_owned()
}

fn component_text(component: &CssValueToken) -> String {
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

fn invalid_value(property: &str, value: &str) -> CssValueError {
    CssValueError::UnsupportedValue {
        property: property.to_owned(),
        value: value.to_owned(),
    }
}

fn merge_grid_line(target: &mut Option<Line<GridPlacementValue>>, value: Line<GridPlacementValue>) {
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

fn map_display(display: Display) -> TaffyDisplay {
    match display {
        Display::Block => TaffyDisplay::Block,
        Display::Flex => TaffyDisplay::Flex,
        Display::Grid => TaffyDisplay::Grid,
        Display::None => TaffyDisplay::None,
    }
}

fn map_box_sizing(box_sizing: BoxSizingValue) -> TaffyBoxSizing {
    match box_sizing {
        BoxSizingValue::BorderBox => TaffyBoxSizing::BorderBox,
        BoxSizingValue::ContentBox => TaffyBoxSizing::ContentBox,
    }
}

fn map_flex_direction(direction: FlexDirectionValue) -> TaffyFlexDirection {
    match direction {
        FlexDirectionValue::Row => TaffyFlexDirection::Row,
        FlexDirectionValue::Column => TaffyFlexDirection::Column,
        FlexDirectionValue::RowReverse => TaffyFlexDirection::RowReverse,
        FlexDirectionValue::ColumnReverse => TaffyFlexDirection::ColumnReverse,
    }
}

fn map_flex_wrap(wrap: FlexWrapValue) -> TaffyFlexWrap {
    match wrap {
        FlexWrapValue::NoWrap => TaffyFlexWrap::NoWrap,
        FlexWrapValue::Wrap => TaffyFlexWrap::Wrap,
        FlexWrapValue::WrapReverse => TaffyFlexWrap::WrapReverse,
    }
}

fn map_position(position: PositionValue) -> TaffyPosition {
    match position {
        PositionValue::Relative => TaffyPosition::Relative,
        PositionValue::Absolute => TaffyPosition::Absolute,
    }
}

fn map_overflow(overflow: OverflowValue) -> TaffyOverflow {
    match overflow {
        OverflowValue::Visible => TaffyOverflow::Visible,
        OverflowValue::Clip => TaffyOverflow::Clip,
        OverflowValue::Hidden => TaffyOverflow::Hidden,
        OverflowValue::Scroll => TaffyOverflow::Scroll,
    }
}

fn map_align_items(value: AlignmentValue) -> TaffyAlignItems {
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

fn map_align_content(value: ContentAlignmentValue) -> TaffyAlignContent {
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

fn map_justify_content(value: ContentAlignmentValue) -> TaffyJustifyContent {
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

fn map_size_value(value: SizeValue) -> TaffyDimension {
    match value {
        SizeValue::Auto => TaffyDimension::auto(),
        SizeValue::LengthPercentage(value) => match value {
            LengthPercentage::Px(value) => TaffyDimension::length(value),
            LengthPercentage::Percent(value) => TaffyDimension::percent(value / 100.0),
        },
    }
}

fn map_size_value_auto(value: SizeValue) -> TaffyLengthPercentageAuto {
    match value {
        SizeValue::Auto => TaffyLengthPercentageAuto::AUTO,
        SizeValue::LengthPercentage(value) => map_length_percentage_auto(value),
    }
}

fn map_length_percentage(value: LengthPercentage) -> TaffyLengthPercentage {
    match value {
        LengthPercentage::Px(value) => TaffyLengthPercentage::length(value),
        LengthPercentage::Percent(value) => TaffyLengthPercentage::percent(value / 100.0),
    }
}

fn map_length_percentage_auto(value: LengthPercentage) -> TaffyLengthPercentageAuto {
    match value {
        LengthPercentage::Px(value) => TaffyLengthPercentageAuto::length(value),
        LengthPercentage::Percent(value) => TaffyLengthPercentageAuto::percent(value / 100.0),
    }
}

fn map_box_edges<T, U>(edges: BoxEdges<T>, map: fn(T) -> U) -> TaffyRect<U> {
    TaffyRect {
        left: map(edges.left),
        right: map(edges.right),
        top: map(edges.top),
        bottom: map(edges.bottom),
    }
}

fn map_grid_auto_flow(flow: GridAutoFlow) -> TaffyGridAutoFlow {
    match flow {
        GridAutoFlow::Row => TaffyGridAutoFlow::Row,
        GridAutoFlow::Column => TaffyGridAutoFlow::Column,
        GridAutoFlow::RowDense => TaffyGridAutoFlow::RowDense,
        GridAutoFlow::ColumnDense => TaffyGridAutoFlow::ColumnDense,
    }
}

fn map_grid_line(value: Line<GridPlacementValue>) -> TaffyLine<TaffyGridPlacement> {
    TaffyLine {
        start: map_grid_placement(value.start),
        end: map_grid_placement(value.end),
    }
}

fn map_grid_placement(value: GridPlacementValue) -> TaffyGridPlacement {
    match value {
        GridPlacementValue::Auto => TaffyGridPlacement::Auto,
        GridPlacementValue::Line(line) => TaffyGridPlacement::Line(line.into()),
        GridPlacementValue::NamedLine(name, index) => TaffyGridPlacement::NamedLine(name, index),
        GridPlacementValue::Span(span) => TaffyGridPlacement::Span(span),
        GridPlacementValue::NamedSpan(name, span) => TaffyGridPlacement::NamedSpan(name, span),
    }
}

fn map_grid_template_component(
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

fn map_grid_track_repeat(repetition: &GridTrackRepeat) -> TaffyGridTemplateRepetition<String> {
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

fn map_grid_repetition_count(value: GridRepetitionCount) -> TaffyRepetitionCount {
    match value {
        GridRepetitionCount::AutoFill => TaffyRepetitionCount::AutoFill,
        GridRepetitionCount::AutoFit => TaffyRepetitionCount::AutoFit,
        GridRepetitionCount::Count(count) => TaffyRepetitionCount::Count(count),
    }
}

fn map_grid_template_area(area: &GridTemplateArea) -> TaffyGridTemplateArea<String> {
    TaffyGridTemplateArea {
        name: area.name.clone(),
        row_start: area.row_start,
        row_end: area.row_end,
        column_start: area.column_start,
        column_end: area.column_end,
    }
}

fn map_grid_track_sizing_function(value: GridTrackValue) -> TaffyTrackSizingFunction {
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

fn map_grid_track_min_value(value: GridTrackMinValue) -> TaffyMinTrackSizingFunction {
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

fn map_grid_track_max_value(value: GridTrackMaxValue) -> TaffyMaxTrackSizingFunction {
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

fn map_track_length_percentage(value: LengthPercentage) -> TaffyTrackLengthPercentage {
    match value {
        LengthPercentage::Px(value) => TaffyTrackLengthPercentage::length(value),
        LengthPercentage::Percent(value) => TaffyTrackLengthPercentage::percent(value / 100.0),
    }
}
