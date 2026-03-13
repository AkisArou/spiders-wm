use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, SourceLocation, StyleSheetParser,
};
use spiders_shared::layout::ResolvedLayoutNode;
use taffy::geometry::{Rect as TaffyRect, Size as TaffySize};
use taffy::prelude::{
    Dimension as TaffyDimension, Display as TaffyDisplay, FlexDirection as TaffyFlexDirection,
    TaffyAuto,
};
use taffy::style::{
    LengthPercentage as TaffyLengthPercentage, LengthPercentageAuto as TaffyLengthPercentageAuto,
    Style as TaffyStyle,
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssParserStrategy {
    CustomSubset,
    CssParser,
    LightningCss,
    Raffia,
    SwcCss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedRule<'a> {
    pub rule_index: usize,
    pub rule: &'a StyleRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Type(NodeSelector),
    Id(String),
    Class(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSelector {
    Workspace,
    Group,
    Window,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Flex,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexDirectionValue {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthPercentage {
    Px(f32),
    Percent(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeValue {
    Auto,
    LengthPercentage(LengthPercentage),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxEdges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComputedStyle {
    pub display: Option<Display>,
    pub flex_direction: Option<FlexDirectionValue>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<SizeValue>,
    pub width: Option<SizeValue>,
    pub height: Option<SizeValue>,
    pub min_width: Option<SizeValue>,
    pub min_height: Option<SizeValue>,
    pub max_width: Option<SizeValue>,
    pub max_height: Option<SizeValue>,
    pub gap: Option<LengthPercentage>,
    pub padding: Option<BoxEdges<LengthPercentage>>,
    pub margin: Option<BoxEdges<LengthPercentage>>,
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
    FlexDirection(FlexDirectionValue),
    FlexGrow(f32),
    FlexShrink(f32),
    FlexBasis(SizeValue),
    Width(SizeValue),
    Height(SizeValue),
    MinWidth(SizeValue),
    MinHeight(SizeValue),
    MaxWidth(SizeValue),
    MaxHeight(SizeValue),
    Gap(LengthPercentage),
    Padding(BoxEdges<LengthPercentage>),
    Margin(BoxEdges<LengthPercentage>),
}

pub fn matching_rules<'a>(
    sheet: &'a StyleSheet,
    node: &spiders_shared::layout::ResolvedLayoutNode,
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
    if let Some(direction) = style.flex_direction {
        taffy_style.flex_direction = map_flex_direction(direction);
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
    if style.width.is_some() || style.height.is_some() {
        taffy_style.size = TaffySize {
            width: style
                .width
                .map(map_size_value)
                .unwrap_or(TaffyDimension::AUTO),
            height: style
                .height
                .map(map_size_value)
                .unwrap_or(TaffyDimension::AUTO),
        };
    }
    if style.min_width.is_some() || style.min_height.is_some() {
        taffy_style.min_size = TaffySize {
            width: style
                .min_width
                .map(map_size_value)
                .unwrap_or(TaffyDimension::AUTO),
            height: style
                .min_height
                .map(map_size_value)
                .unwrap_or(TaffyDimension::AUTO),
        };
    }
    if style.max_width.is_some() || style.max_height.is_some() {
        taffy_style.max_size = TaffySize {
            width: style
                .max_width
                .map(map_size_value)
                .unwrap_or(TaffyDimension::AUTO),
            height: style
                .max_height
                .map(map_size_value)
                .unwrap_or(TaffyDimension::AUTO),
        };
    }
    if let Some(gap) = style.gap {
        let gap = map_length_percentage(gap);
        taffy_style.gap = TaffySize {
            width: gap,
            height: gap,
        };
    }
    if let Some(padding) = style.padding {
        taffy_style.padding = map_box_edges(padding, map_length_percentage);
    }
    if let Some(margin) = style.margin {
        taffy_style.margin = map_box_edges(margin, map_length_percentage_auto);
    }

    taffy_style
}

fn map_display(display: Display) -> TaffyDisplay {
    match display {
        Display::Flex => TaffyDisplay::Flex,
    }
}

fn map_flex_direction(direction: FlexDirectionValue) -> TaffyFlexDirection {
    match direction {
        FlexDirectionValue::Row => TaffyFlexDirection::Row,
        FlexDirectionValue::Column => TaffyFlexDirection::Column,
    }
}

fn map_size_value(value: SizeValue) -> TaffyDimension {
    match value {
        SizeValue::Auto => TaffyDimension::AUTO,
        SizeValue::LengthPercentage(value) => map_length_percentage(value).into(),
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CssParseError {
    #[error("unsupported at-rule `{name}`")]
    UnsupportedAtRule { name: String },
    #[error("unsupported selector `{selector}`")]
    UnsupportedSelector { selector: String },
    #[error("unsupported property `{property}`")]
    UnsupportedProperty { property: String },
    #[error("invalid CSS near line {line}, column {column}")]
    InvalidSyntax { line: u32, column: u32 },
}

#[derive(Debug, Error, PartialEq)]
pub enum CssValueError {
    #[error("unsupported value `{value}` for property `{property}`")]
    UnsupportedValue { property: String, value: String },
}

pub fn parse_stylesheet(input: &str) -> Result<StyleSheet, CssParseError> {
    let mut input = ParserInput::new(input);
    let mut parser = Parser::new(&mut input);
    let mut rule_parser = LayoutCssParser;
    let parser = StyleSheetParser::new(&mut parser, &mut rule_parser);

    let mut rules = Vec::new();
    for rule in parser {
        rules.push(rule.map_err(|(err, _slice)| map_parse_error(err))?);
    }

    Ok(StyleSheet { rules })
}

struct LayoutCssParser;

impl<'i> AtRuleParser<'i> for LayoutCssParser {
    type Prelude = ();
    type AtRule = StyleRule;
    type Error = CssParseError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        Err(input.new_custom_error(CssParseError::UnsupportedAtRule {
            name: name.to_string(),
        }))
    }
}

impl<'i> QualifiedRuleParser<'i> for LayoutCssParser {
    type Prelude = Vec<Selector>;
    type QualifiedRule = StyleRule;
    type Error = CssParseError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let start = input.state();
        while !input.is_exhausted() {
            input.next_including_whitespace_and_comments()?;
        }
        let selector_text = input.slice_from(start.position()).trim();
        parse_selectors(selector_text).map_err(|err| input.new_custom_error(err))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        let mut declarations = Vec::new();
        let mut declaration_parser = LayoutDeclarationParser;
        let body_parser = RuleBodyParser::new(input, &mut declaration_parser);

        for item in body_parser {
            let item = match item {
                Ok(item) => item,
                Err((err, _slice)) => return Err(input.new_custom_error(map_parse_error(err))),
            };

            match item {
                RuleBodyItem::Declaration(declaration) => declarations.push(declaration),
            }
        }

        Ok(StyleRule {
            selectors: prelude,
            declarations,
        })
    }
}

struct LayoutDeclarationParser;

enum RuleBodyItem {
    Declaration(Declaration),
}

impl<'i> RuleBodyItemParser<'i, RuleBodyItem, CssParseError> for LayoutDeclarationParser {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

impl<'i> DeclarationParser<'i> for LayoutDeclarationParser {
    type Declaration = RuleBodyItem;
    type Error = CssParseError;

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _declaration_start: &ParserState,
    ) -> Result<Self::Declaration, ParseError<'i, Self::Error>> {
        let property = name.to_string();
        validate_property(&property).map_err(|err| input.new_custom_error(err))?;

        let start = input.position();
        while !input.is_exhausted() {
            input.next_including_whitespace_and_comments()?;
        }
        let value = input.slice_from(start).trim().to_owned();

        Ok(RuleBodyItem::Declaration(Declaration { property, value }))
    }
}

fn parse_selectors(input: &str) -> Result<Vec<Selector>, CssParseError> {
    input
        .split(',')
        .map(|selector| parse_selector(selector.trim()))
        .collect()
}

fn parse_selector(input: &str) -> Result<Selector, CssParseError> {
    if input.is_empty() {
        return Err(CssParseError::UnsupportedSelector {
            selector: input.to_owned(),
        });
    }

    if let Some(id) = input.strip_prefix('#') {
        return Ok(Selector::Id(id.to_owned()));
    }

    if let Some(class) = input.strip_prefix('.') {
        return Ok(Selector::Class(class.to_owned()));
    }

    let selector = match input {
        "workspace" => NodeSelector::Workspace,
        "group" => NodeSelector::Group,
        "window" => NodeSelector::Window,
        _ => {
            return Err(CssParseError::UnsupportedSelector {
                selector: input.to_owned(),
            })
        }
    };

    Ok(Selector::Type(selector))
}

fn validate_property(property: &str) -> Result<(), CssParseError> {
    match property {
        "display" | "flex-direction" | "flex-grow" | "flex-shrink" | "flex-basis" | "width"
        | "height" | "min-width" | "min-height" | "max-width" | "max-height" | "gap"
        | "padding" | "margin" => Ok(()),
        _ => Err(CssParseError::UnsupportedProperty {
            property: property.to_owned(),
        }),
    }
}

pub fn compile_declaration(
    declaration: &Declaration,
) -> Result<CompiledDeclaration, CssValueError> {
    let property = declaration.property.as_str();
    let value = declaration.value.as_str();

    match property {
        "display" => Ok(CompiledDeclaration::Display(parse_display(
            property, value,
        )?)),
        "flex-direction" => Ok(CompiledDeclaration::FlexDirection(parse_flex_direction(
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
        "gap" => Ok(CompiledDeclaration::Gap(parse_length_percentage(
            property, value,
        )?)),
        "padding" => Ok(CompiledDeclaration::Padding(parse_box_edges(
            property, value,
        )?)),
        "margin" => Ok(CompiledDeclaration::Margin(parse_box_edges(
            property, value,
        )?)),
        _ => Err(CssValueError::UnsupportedValue {
            property: declaration.property.clone(),
            value: declaration.value.clone(),
        }),
    }
}

impl ComputedStyle {
    fn apply(&mut self, declaration: CompiledDeclaration) {
        match declaration {
            CompiledDeclaration::Display(value) => self.display = Some(value),
            CompiledDeclaration::FlexDirection(value) => self.flex_direction = Some(value),
            CompiledDeclaration::FlexGrow(value) => self.flex_grow = Some(value),
            CompiledDeclaration::FlexShrink(value) => self.flex_shrink = Some(value),
            CompiledDeclaration::FlexBasis(value) => self.flex_basis = Some(value),
            CompiledDeclaration::Width(value) => self.width = Some(value),
            CompiledDeclaration::Height(value) => self.height = Some(value),
            CompiledDeclaration::MinWidth(value) => self.min_width = Some(value),
            CompiledDeclaration::MinHeight(value) => self.min_height = Some(value),
            CompiledDeclaration::MaxWidth(value) => self.max_width = Some(value),
            CompiledDeclaration::MaxHeight(value) => self.max_height = Some(value),
            CompiledDeclaration::Gap(value) => self.gap = Some(value),
            CompiledDeclaration::Padding(value) => self.padding = Some(value),
            CompiledDeclaration::Margin(value) => self.margin = Some(value),
        }
    }
}

fn parse_display(property: &str, value: &str) -> Result<Display, CssValueError> {
    match value {
        "flex" => Ok(Display::Flex),
        _ => Err(invalid_value(property, value)),
    }
}

fn parse_flex_direction(property: &str, value: &str) -> Result<FlexDirectionValue, CssValueError> {
    match value {
        "row" => Ok(FlexDirectionValue::Row),
        "column" => Ok(FlexDirectionValue::Column),
        _ => Err(invalid_value(property, value)),
    }
}

fn parse_number(property: &str, value: &str) -> Result<f32, CssValueError> {
    value.parse().map_err(|_| invalid_value(property, value))
}

fn parse_size_value(property: &str, value: &str) -> Result<SizeValue, CssValueError> {
    if value == "auto" {
        return Ok(SizeValue::Auto);
    }

    Ok(SizeValue::LengthPercentage(parse_length_percentage(
        property, value,
    )?))
}

fn parse_length_percentage(property: &str, value: &str) -> Result<LengthPercentage, CssValueError> {
    if let Some(px) = value.strip_suffix("px") {
        return px
            .parse()
            .map(LengthPercentage::Px)
            .map_err(|_| invalid_value(property, value));
    }

    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .parse()
            .map(LengthPercentage::Percent)
            .map_err(|_| invalid_value(property, value));
    }

    Err(invalid_value(property, value))
}

fn parse_box_edges(
    property: &str,
    value: &str,
) -> Result<BoxEdges<LengthPercentage>, CssValueError> {
    let values: Result<Vec<_>, _> = value
        .split_whitespace()
        .map(|part| parse_length_percentage(property, part))
        .collect();
    let values = values?;

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
        _ => Err(invalid_value(property, value)),
    }
}

fn invalid_value(property: &str, value: &str) -> CssValueError {
    CssValueError::UnsupportedValue {
        property: property.to_owned(),
        value: value.to_owned(),
    }
}

fn map_parse_error(err: ParseError<'_, CssParseError>) -> CssParseError {
    match err.kind {
        cssparser::ParseErrorKind::Custom(error) => error,
        cssparser::ParseErrorKind::Basic(_) => location_error(err.location),
    }
}

fn location_error(location: SourceLocation) -> CssParseError {
    CssParseError::InvalidSyntax {
        line: location.line,
        column: location.column,
    }
}

impl<'i> QualifiedRuleParser<'i> for LayoutDeclarationParser {
    type Prelude = ();
    type QualifiedRule = RuleBodyItem;
    type Error = CssParseError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let start = input.position();
        let selector = input.slice_from(start).trim().to_owned();
        Err(input.new_custom_error(CssParseError::UnsupportedSelector { selector }))
    }

    fn parse_block<'t>(
        &mut self,
        _prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        Err(input.new_custom_error(location_error(input.current_source_location())))
    }
}

impl<'i> AtRuleParser<'i> for LayoutDeclarationParser {
    type Prelude = ();
    type AtRule = RuleBodyItem;
    type Error = CssParseError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        Err(input.new_custom_error(CssParseError::UnsupportedAtRule {
            name: name.to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::WindowId;
    use spiders_shared::layout::{LayoutNodeMeta, ResolvedLayoutNode};

    use super::*;

    fn runtime_window_with_meta(meta: LayoutNodeMeta) -> ResolvedLayoutNode {
        ResolvedLayoutNode::Window {
            meta,
            window_id: Some(WindowId::from("win-1")),
        }
    }

    #[test]
    fn parses_basic_rule_with_multiple_selectors() {
        let sheet =
            parse_stylesheet("workspace, .stack { display: flex; flex-direction: row; gap: 8px; }")
                .unwrap();

        assert_eq!(
            sheet,
            StyleSheet {
                rules: vec![StyleRule {
                    selectors: vec![
                        Selector::Type(NodeSelector::Workspace),
                        Selector::Class("stack".into()),
                    ],
                    declarations: vec![
                        Declaration {
                            property: "display".into(),
                            value: "flex".into(),
                        },
                        Declaration {
                            property: "flex-direction".into(),
                            value: "row".into(),
                        },
                        Declaration {
                            property: "gap".into(),
                            value: "8px".into(),
                        },
                    ],
                }],
            }
        );
    }

    #[test]
    fn parses_id_selector() {
        let sheet = parse_stylesheet("#main { width: 50%; }").unwrap();

        assert_eq!(sheet.rules[0].selectors, vec![Selector::Id("main".into())]);
    }

    #[test]
    fn rejects_unsupported_selector() {
        let error = parse_stylesheet("slot { display: flex; }").unwrap_err();

        assert_eq!(
            error,
            CssParseError::UnsupportedSelector {
                selector: "slot".into(),
            }
        );
    }

    #[test]
    fn rejects_unsupported_property() {
        let error = parse_stylesheet("window { color: red; }").unwrap_err();

        assert_eq!(
            error,
            CssParseError::UnsupportedProperty {
                property: "color".into(),
            }
        );
    }

    #[test]
    fn rejects_at_rules_for_v1() {
        let error = parse_stylesheet("@media screen { window { width: 100%; } }").unwrap_err();

        assert_eq!(
            error,
            CssParseError::UnsupportedAtRule {
                name: "media".into(),
            }
        );
    }

    #[test]
    fn matches_type_id_and_class_selectors_against_runtime_nodes() {
        let node = runtime_window_with_meta(LayoutNodeMeta {
            id: Some("main".into()),
            class: vec!["stack".into(), "focused".into()],
            ..LayoutNodeMeta::default()
        });

        assert!(selector_matches(
            &Selector::Type(NodeSelector::Window),
            &node
        ));
        assert!(selector_matches(&Selector::Id("main".into()), &node));
        assert!(selector_matches(&Selector::Class("stack".into()), &node));
        assert!(!selector_matches(
            &Selector::Type(NodeSelector::Group),
            &node
        ));
        assert!(!selector_matches(&Selector::Class("missing".into()), &node));
    }

    #[test]
    fn collects_rules_matching_any_selector_in_rule() {
        let sheet = parse_stylesheet(
            "group { gap: 8px; } #main, .stack { width: 50%; } window { height: 100%; }",
        )
        .unwrap();
        let node = runtime_window_with_meta(LayoutNodeMeta {
            id: Some("main".into()),
            class: vec!["stack".into()],
            ..LayoutNodeMeta::default()
        });

        let matches = matching_rules(&sheet, &node);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].rule_index, 1);
        assert_eq!(matches[1].rule_index, 2);
        assert_eq!(matches[0].rule.declarations[0].property, "width");
        assert_eq!(matches[1].rule.declarations[0].property, "height");
    }

    #[test]
    fn compiles_typed_declaration_values() {
        let declaration = Declaration {
            property: "padding".into(),
            value: "8px 16px".into(),
        };

        let compiled = compile_declaration(&declaration).unwrap();

        assert_eq!(
            compiled,
            CompiledDeclaration::Padding(BoxEdges {
                top: LengthPercentage::Px(8.0),
                right: LengthPercentage::Px(16.0),
                bottom: LengthPercentage::Px(8.0),
                left: LengthPercentage::Px(16.0),
            })
        );
    }

    #[test]
    fn later_matching_rules_override_earlier_declarations() {
        let sheet = parse_stylesheet(
            "window { width: 40%; gap: 8px; } .stack { width: 60%; } #main { gap: 12px; }",
        )
        .unwrap();
        let node = runtime_window_with_meta(LayoutNodeMeta {
            id: Some("main".into()),
            class: vec!["stack".into()],
            ..LayoutNodeMeta::default()
        });

        let style = compute_style(&sheet, &node).unwrap();

        assert_eq!(
            style.width,
            Some(SizeValue::LengthPercentage(LengthPercentage::Percent(60.0)))
        );
        assert_eq!(style.gap, Some(LengthPercentage::Px(12.0)));
    }

    #[test]
    fn invalid_supported_property_value_fails_during_compilation() {
        let declaration = Declaration {
            property: "display".into(),
            value: "block".into(),
        };

        let error = compile_declaration(&declaration).unwrap_err();

        assert_eq!(
            error,
            CssValueError::UnsupportedValue {
                property: "display".into(),
                value: "block".into(),
            }
        );
    }

    #[test]
    fn maps_computed_style_into_taffy_style() {
        let style = ComputedStyle {
            display: Some(Display::Flex),
            flex_direction: Some(FlexDirectionValue::Column),
            flex_grow: Some(2.0),
            width: Some(SizeValue::LengthPercentage(LengthPercentage::Percent(50.0))),
            gap: Some(LengthPercentage::Px(12.0)),
            padding: Some(BoxEdges {
                top: LengthPercentage::Px(4.0),
                right: LengthPercentage::Px(8.0),
                bottom: LengthPercentage::Px(4.0),
                left: LengthPercentage::Px(8.0),
            }),
            ..ComputedStyle::default()
        };

        let mapped = map_computed_style_to_taffy(&style);

        assert_eq!(mapped.display, TaffyDisplay::Flex);
        assert_eq!(mapped.flex_direction, TaffyFlexDirection::Column);
        assert_eq!(mapped.flex_grow, 2.0);
        assert_eq!(mapped.size.width, TaffyDimension::percent(0.5));
        assert_eq!(mapped.gap.width, TaffyLengthPercentage::length(12.0));
        assert_eq!(mapped.padding.left, TaffyLengthPercentage::length(8.0));
    }
}
