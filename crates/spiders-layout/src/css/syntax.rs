use cssparser::{
    AtRuleParser, CowRcStr, Parser, ParserInput, QualifiedRuleParser, StyleSheetParser, Token,
};

use crate::stylo_adapter::{
    parse_selector_list_from_parser, LayoutSelectorImpl, LayoutSelectorParser,
};

use super::compile::{compile_declaration, CompiledDeclaration, CssValueError};
use super::compiled::{CompiledStyleRule, CompiledStyleSheet};
use super::grid::parse_grid_fallback_declarations;
use super::parse_values::{
    CssDelimiter, CssDimension, CssFunction, CssSimpleBlock, CssSimpleBlockKind, CssValue,
    CssValueToken, ParsedDeclaration,
};
use style::parser::ParserContext;
use style::properties::declaration_block::parse_property_declaration_list;
use style::stylesheets::{CssRuleType, Origin, UrlExtraData};
use style_traits::values::ToCss;
use style_traits::ParsingMode;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CssParseError {
    #[error("unsupported at-rule `{name}`")]
    UnsupportedAtRule { name: String },
    #[error("unsupported selector `{selector}`")]
    UnsupportedSelector { selector: String },
    #[error("unsupported property `{property}`")]
    UnsupportedProperty { property: String },
    #[error("invalid CSS near line {line}, column {column}")]
    InvalidSyntax { line: u32, column: u32 },
    #[error(transparent)]
    CssValue(#[from] CssValueError),
}

pub(super) const SUPPORTED_PROPERTIES: &[&str] = &[
    "display",
    "box-sizing",
    "aspect-ratio",
    "flex-direction",
    "flex-wrap",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "position",
    "inset",
    "top",
    "right",
    "bottom",
    "left",
    "overflow",
    "overflow-x",
    "overflow-y",
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "align-items",
    "align-self",
    "justify-items",
    "justify-self",
    "align-content",
    "justify-content",
    "gap",
    "row-gap",
    "column-gap",
    "grid-template-rows",
    "grid-template-columns",
    "grid-auto-rows",
    "grid-auto-columns",
    "grid-auto-flow",
    "grid-template-areas",
    "grid-row",
    "grid-column",
    "grid-row-start",
    "grid-row-end",
    "grid-column-start",
    "grid-column-end",
    "border-width",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
];

#[derive(Default)]
struct LayoutCssRuleParser;

pub fn parse_stylesheet(input: &str) -> Result<CompiledStyleSheet, CssParseError> {
    let mut input_buf = ParserInput::new(input);
    let mut parser_input = Parser::new(&mut input_buf);
    let mut parser = LayoutCssRuleParser;
    let mut rules = Vec::new();

    for item in StyleSheetParser::new(&mut parser_input, &mut parser) {
        match item {
            Ok(rule) => rules.push(rule),
            Err((err, _slice)) => {
                let location = err.location;
                return Err(match err.kind {
                    cssparser::ParseErrorKind::Custom(error) => error,
                    _ => CssParseError::InvalidSyntax {
                        line: location.line,
                        column: location.column,
                    },
                });
            }
        }
    }

    Ok(CompiledStyleSheet { rules })
}

impl<'i> AtRuleParser<'i> for LayoutCssRuleParser {
    type Prelude = ();
    type AtRule = CompiledStyleRule;
    type Error = CssParseError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        Err(input.new_custom_error(CssParseError::UnsupportedAtRule {
            name: name.to_string(),
        }))
    }
}

impl<'i> QualifiedRuleParser<'i> for LayoutCssRuleParser {
    type Prelude = selectors::parser::SelectorList<LayoutSelectorImpl>;
    type QualifiedRule = CompiledStyleRule;
    type Error = CssParseError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        let start = input.state();
        let parser = LayoutSelectorParser;
        let parsed = parse_selector_list_from_parser(&parser, input).map_err(|_| {
            let selector = input.slice_from(start.position()).trim().to_string();
            input.new_custom_error(CssParseError::UnsupportedSelector { selector })
        })?;

        let selector = input.slice_from(start.position()).trim().to_string();
        let trimmed = selector.trim_start();
        let slot_type_selected = trimmed == "slot"
            || trimmed
                .strip_prefix("slot")
                .and_then(|rest| rest.chars().next())
                .is_some_and(|ch| {
                    matches!(ch, ' ' | '.' | '#' | '[' | ':' | '>' | '+' | '~' | ',')
                });
        if slot_type_selected {
            return Err(input.new_custom_error(CssParseError::UnsupportedSelector { selector }));
        }

        Ok(parsed)
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &cssparser::ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, cssparser::ParseError<'i, Self::Error>> {
        let url_data = UrlExtraData(url::Url::parse("about:blank").unwrap().into());
        let context = ParserContext::new(
            Origin::Author,
            &url_data,
            Some(CssRuleType::Style),
            ParsingMode::DEFAULT,
            style::context::QuirksMode::NoQuirks,
            Default::default(),
            None,
            None,
        );

        let block_start = input.state();
        let block = parse_property_declaration_list(&context, input, &[]);
        let mut declarations = Vec::new();
        for declaration in block.normal_declaration_iter() {
            let property = declaration.id().to_css_string();
            if !SUPPORTED_PROPERTIES.contains(&property.as_str()) {
                return Err(input.new_custom_error(CssParseError::UnsupportedProperty { property }));
            }

            if let Some(compiled) = compile_stylo_declaration(declaration)
                .map_err(|error| input.new_custom_error(error))?
            {
                declarations.push(compiled);
                continue;
            }

            let mut value = String::new();
            declaration.to_css(&mut value).map_err(|_| {
                input.new_custom_error(CssParseError::InvalidSyntax { line: 1, column: 1 })
            })?;

            let parsed = ParsedDeclaration {
                property,
                value: CssValue {
                    text: value.clone(),
                    components: parse_value_tokens(&value)
                        .map_err(|error| input.new_custom_error(error))?,
                },
            };
            let compiled = compile_declaration(&parsed)
                .map_err(|error| input.new_custom_error(CssParseError::CssValue(error)))?;
            declarations.push(compiled);
        }

        if declarations.is_empty() {
            let raw_block = input.slice_from(block_start.position()).trim().to_string();
            if needs_grid_fallback(&raw_block) {
                declarations = parse_grid_fallback_declarations(&raw_block)
                    .map_err(|error| input.new_custom_error(error))?;
            }
        }

        Ok(CompiledStyleRule {
            selectors: prelude,
            declarations,
        })
    }
}

pub(super) fn compile_stylo_declaration(
    declaration: &style::properties::PropertyDeclaration,
) -> Result<Option<CompiledDeclaration>, CssParseError> {
    use style::properties::PropertyDeclaration::*;

    fn from_value(
        property: &str,
        value: &impl ToCss,
    ) -> Result<CompiledDeclaration, CssParseError> {
        let mut text = String::new();
        value
            .to_css(&mut style_traits::CssWriter::new(&mut text))
            .map_err(|_| CssParseError::InvalidSyntax { line: 1, column: 1 })?;
        let parsed = ParsedDeclaration {
            property: property.into(),
            value: CssValue {
                text: text.clone(),
                components: parse_value_tokens(&text)?,
            },
        };
        compile_declaration(&parsed).map_err(CssParseError::CssValue)
    }

    match declaration {
        Display(value) => from_value("display", value).map(Some),
        BoxSizing(value) => from_value("box-sizing", value).map(Some),
        AspectRatio(value) => from_value("aspect-ratio", value).map(Some),
        FlexDirection(value) => from_value("flex-direction", value).map(Some),
        FlexWrap(value) => from_value("flex-wrap", value).map(Some),
        FlexGrow(value) => from_value("flex-grow", value).map(Some),
        FlexShrink(value) => from_value("flex-shrink", value).map(Some),
        FlexBasis(value) => from_value("flex-basis", value).map(Some),
        Position(value) => from_value("position", value).map(Some),
        Top(value) => from_value("top", value).map(Some),
        Right(value) => from_value("right", value).map(Some),
        Bottom(value) => from_value("bottom", value).map(Some),
        Left(value) => from_value("left", value).map(Some),
        OverflowX(value) => from_value("overflow-x", value).map(Some),
        OverflowY(value) => from_value("overflow-y", value).map(Some),
        Width(value) => from_value("width", value).map(Some),
        Height(value) => from_value("height", value).map(Some),
        MinWidth(value) => from_value("min-width", value).map(Some),
        MinHeight(value) => from_value("min-height", value).map(Some),
        MaxWidth(value) => from_value("max-width", value).map(Some),
        MaxHeight(value) => from_value("max-height", value).map(Some),
        AlignItems(value) => from_value("align-items", value).map(Some),
        AlignSelf(value) => from_value("align-self", value).map(Some),
        JustifyItems(value) => from_value("justify-items", value).map(Some),
        JustifySelf(value) => from_value("justify-self", value).map(Some),
        AlignContent(value) => from_value("align-content", value).map(Some),
        JustifyContent(value) => from_value("justify-content", value).map(Some),
        RowGap(value) => from_value("row-gap", value).map(Some),
        ColumnGap(value) => from_value("column-gap", value).map(Some),
        GridAutoFlow(value) => from_value("grid-auto-flow", value).map(Some),
        GridTemplateAreas(value) => from_value("grid-template-areas", value).map(Some),
        GridTemplateRows(value) => from_value("grid-template-rows", value).map(Some),
        GridTemplateColumns(value) => from_value("grid-template-columns", value).map(Some),
        GridAutoRows(value) => from_value("grid-auto-rows", value).map(Some),
        GridAutoColumns(value) => from_value("grid-auto-columns", value).map(Some),
        GridRowStart(value) => from_value("grid-row-start", value).map(Some),
        GridRowEnd(value) => from_value("grid-row-end", value).map(Some),
        GridColumnStart(value) => from_value("grid-column-start", value).map(Some),
        GridColumnEnd(value) => from_value("grid-column-end", value).map(Some),
        PaddingTop(value) => from_value("padding-top", value).map(Some),
        PaddingRight(value) => from_value("padding-right", value).map(Some),
        PaddingBottom(value) => from_value("padding-bottom", value).map(Some),
        PaddingLeft(value) => from_value("padding-left", value).map(Some),
        MarginTop(value) => from_value("margin-top", value).map(Some),
        MarginRight(value) => from_value("margin-right", value).map(Some),
        MarginBottom(value) => from_value("margin-bottom", value).map(Some),
        MarginLeft(value) => from_value("margin-left", value).map(Some),
        BorderTopWidth(value) => from_value("border-top-width", value).map(Some),
        BorderRightWidth(value) => from_value("border-right-width", value).map(Some),
        BorderBottomWidth(value) => from_value("border-bottom-width", value).map(Some),
        BorderLeftWidth(value) => from_value("border-left-width", value).map(Some),
        _ => Ok(None),
    }
}

fn needs_grid_fallback(raw_block: &str) -> bool {
    raw_block.contains("grid-template-")
        || raw_block.contains("grid-row:")
        || raw_block.contains("grid-column:")
}

pub(super) fn parse_value_tokens(input: &str) -> Result<Vec<CssValueToken>, CssParseError> {
    let mut input_buf = ParserInput::new(input);
    let mut parser = Parser::new(&mut input_buf);
    parse_component_values(&mut parser).map_err(|err| CssParseError::InvalidSyntax {
        line: err.location.line,
        column: err.location.column,
    })
}

pub(super) fn parse_component_values<'i, 't>(
    parser: &mut Parser<'i, 't>,
) -> Result<Vec<CssValueToken>, cssparser::ParseError<'i, CssParseError>> {
    let mut components = Vec::new();
    while let Ok(token) = parser.next_including_whitespace_and_comments() {
        let component = match token.clone() {
            Token::Ident(value) => CssValueToken::Ident(value.to_string()),
            Token::QuotedString(value) => CssValueToken::String(value.to_string()),
            Token::Number {
                value, int_value, ..
            } => match int_value {
                Some(int) if int as f32 == value => CssValueToken::Integer(int as i64),
                _ => CssValueToken::Number(value),
            },
            Token::Percentage {
                unit_value,
                int_value,
                ..
            } => {
                let percent = match int_value {
                    Some(int) => int as f32,
                    None => (unit_value * 100.0 * 1_000_000.0).round() / 1_000_000.0,
                };
                CssValueToken::Percentage(percent)
            }
            Token::Dimension {
                value,
                unit,
                int_value,
                ..
            } => {
                let value = match int_value {
                    Some(int) if int as f32 == value => int as f32,
                    _ => value,
                };
                CssValueToken::Dimension(CssDimension {
                    value,
                    unit: unit.to_string(),
                })
            }
            Token::WhiteSpace(_) | Token::Comment(_) => CssValueToken::Whitespace,
            Token::Comma => CssValueToken::Delimiter(CssDelimiter::Comma),
            Token::Semicolon => CssValueToken::Delimiter(CssDelimiter::Semicolon),
            Token::Delim('/') => CssValueToken::Delimiter(CssDelimiter::Solidus),
            Token::Delim(ch) => CssValueToken::Unknown(ch.to_string()),
            Token::Function(name) => {
                let value = parser.parse_nested_block(parse_component_values)?;
                CssValueToken::Function(CssFunction {
                    name: name.to_string(),
                    value,
                })
            }
            Token::ParenthesisBlock => {
                let value = parser.parse_nested_block(parse_component_values)?;
                CssValueToken::SimpleBlock(CssSimpleBlock {
                    kind: CssSimpleBlockKind::Parenthesis,
                    value,
                })
            }
            Token::SquareBracketBlock => {
                let value = parser.parse_nested_block(parse_component_values)?;
                CssValueToken::SimpleBlock(CssSimpleBlock {
                    kind: CssSimpleBlockKind::Bracket,
                    value,
                })
            }
            Token::CurlyBracketBlock => {
                let value = parser.parse_nested_block(parse_component_values)?;
                CssValueToken::SimpleBlock(CssSimpleBlock {
                    kind: CssSimpleBlockKind::Brace,
                    value,
                })
            }
            other => CssValueToken::Unknown(format!("{other:?}")),
        };
        components.push(component);
    }
    Ok(components)
}
