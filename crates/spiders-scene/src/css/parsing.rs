use cssparser::{AtRuleParser, CowRcStr, Parser, ParserInput, QualifiedRuleParser, StyleSheetParser};

use super::stylo_adapter::{
    parse_selector_list_from_parser, LayoutSelectorImpl, LayoutSelectorParser,
};

use super::compile::{compile_declaration, CssValueError};
use super::compiled::{CompiledStyleRule, CompiledStyleSheet};
use super::grid::parse_grid_fallback_declarations;
use super::parse_values::{CssValue, ParsedDeclaration};
use super::tokenizer::parse_value_tokens;
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

            if let Some(compiled) = super::stylo_compile::compile_stylo_declaration(declaration)
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

fn needs_grid_fallback(raw_block: &str) -> bool {
    raw_block.contains("grid-template-")
        || raw_block.contains("grid-row:")
        || raw_block.contains("grid-column:")
    || raw_block.contains("grid-row-start:")
    || raw_block.contains("grid-row-end:")
    || raw_block.contains("grid-column-start:")
    || raw_block.contains("grid-column-end:")
}
