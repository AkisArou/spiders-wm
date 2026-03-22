use cssparser::{AtRuleParser, CowRcStr, Parser, ParserInput, QualifiedRuleParser, StyleSheetParser};

use super::stylo_adapter::{
    parse_selector_list_from_parser, LayoutPseudoElement, LayoutSelectorImpl, LayoutSelectorParser,
};

use super::compile::{compile_declaration, compile_declaration_from_value, CssValueError};
use super::compiled::{CompiledStyleRule, CompiledStyleSheet};
use super::grid::parse_grid_fallback_declarations;
use super::parse_values::{CssValue, ParsedDeclaration};
use super::tokenizer::parse_value_tokens;
use style::parser::ParserContext;
use style::properties::declaration_block::parse_property_declaration_list;
use style::stylesheets::{CssRuleType, Origin, UrlExtraData};
use style_traits::values::ToCss;
use style_traits::ParsingMode;

struct ParsedSelectorPrelude {
    selectors: selectors::parser::SelectorList<LayoutSelectorImpl>,
    source: String,
}

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
    "appearance",
    "background",
    "background-color",
    "color",
    "opacity",
    "border-color",
    "border-style",
    "border-radius",
    "box-shadow",
    "backdrop-filter",
    "transform",
    "text-align",
    "text-transform",
    "font-family",
    "font-size",
    "font-weight",
    "letter-spacing",
    "animation",
    "animation-name",
    "animation-duration",
    "animation-timing-function",
    "animation-delay",
    "animation-iteration-count",
    "animation-direction",
    "animation-fill-mode",
    "animation-play-state",
    "transition",
    "transition-property",
    "transition-duration",
    "transition-timing-function",
    "transition-delay",
    "transition-behavior",
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
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "border-top-style",
    "border-right-style",
    "border-bottom-style",
    "border-left-style",
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
    let sanitized = strip_ignored_at_rules(input);
    let mut input_buf = ParserInput::new(&sanitized);
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
    type Prelude = ParsedSelectorPrelude;
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

        Ok(ParsedSelectorPrelude {
            selectors: parsed,
            source: selector,
        })
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
                if is_ignored_background_expansion(&property) {
                    continue;
                }
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

        let raw_block = input.slice_from(block_start.position()).trim().to_string();
        if raw_block.contains("appearance:")
            && !declarations
                .iter()
                .any(|declaration| matches!(declaration, super::compile::CompiledDeclaration::Appearance(_)))
        {
            let fallback = parse_grid_fallback_declarations(&raw_block)
                .map_err(|error| input.new_custom_error(error))?;
            declarations.extend(
                fallback.into_iter().filter(|declaration| {
                    matches!(declaration, super::compile::CompiledDeclaration::Appearance(_))
                }),
            );
        }

        if raw_block.contains("background:")
            && !declarations
                .iter()
                .any(|declaration| matches!(declaration, super::compile::CompiledDeclaration::Background(_)))
        {
            let fallback = parse_grid_fallback_declarations(&raw_block)
                .map_err(|error| input.new_custom_error(error))?;
            declarations.extend(
                fallback.into_iter().filter(|declaration| {
                    matches!(declaration, super::compile::CompiledDeclaration::Background(_))
                }),
            );
        }

        if raw_block.contains("border-color:")
            && !declarations
                .iter()
                .any(|declaration| matches!(declaration, super::compile::CompiledDeclaration::BorderColor(_)))
        {
            append_raw_property_fallbacks(&raw_block, &mut declarations, &["border-color"])
                .map_err(|error| input.new_custom_error(error))?;
        }

        append_raw_property_fallbacks(&raw_block, &mut declarations, &["border-radius", "box-shadow", "animation", "transition"])
            .map_err(|error| input.new_custom_error(error))?;

        if declarations.is_empty() {
            if needs_grid_fallback(&raw_block) {
                declarations = parse_grid_fallback_declarations(&raw_block)
                    .map_err(|error| input.new_custom_error(error))?;
            }
        }

        let target_pseudo = selector_target_pseudo(&prelude.source);
        let pseudo_base_selectors = selector_base_selectors(&target_pseudo, &prelude.source)
            .map_err(|error| input.new_custom_error(error))?;

        Ok(CompiledStyleRule {
            selectors: prelude.selectors,
            target_pseudo,
            pseudo_base_selectors,
            declarations,
        })
    }
}

fn strip_ignored_at_rules(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut index = 0;

    while let Some(relative) = input[index..].find("@keyframes") {
        let start = index + relative;
        result.push_str(&input[index..start]);

        let Some(open_brace_offset) = input[start..].find('{') else {
            break;
        };
        let mut depth = 0i32;
        let mut end = start + open_brace_offset;
        for (offset, ch) in input[end..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end += offset + ch.len_utf8();
                        break;
                    }
                }
                _ => {}
            }
        }
        index = end;
    }

    result.push_str(&input[index..]);
    result
}

fn selector_target_pseudo(selector: &str) -> Option<LayoutPseudoElement> {
    if selector.contains("::titlebar") {
        Some(LayoutPseudoElement::Titlebar)
    } else {
        None
    }
}

fn selector_base_selectors(
    target_pseudo: &Option<LayoutPseudoElement>,
    selector: &str,
) -> Result<Option<selectors::parser::SelectorList<LayoutSelectorImpl>>, CssParseError> {
    let Some(LayoutPseudoElement::Titlebar) = target_pseudo else {
        return Ok(None);
    };

    let stripped = selector.replace("::titlebar", "");
    let error_selector = stripped.clone();
    let mut input = ParserInput::new(&stripped);
    let mut parser_input = Parser::new(&mut input);
    parse_selector_list_from_parser(&LayoutSelectorParser, &mut parser_input)
        .map(Some)
        .map_err(|_| CssParseError::UnsupportedSelector {
            selector: error_selector,
        })
}

fn is_ignored_background_expansion(property: &str) -> bool {
    matches!(
        property,
        "background-position-x"
            | "background-position-y"
            | "background-repeat"
            | "background-attachment"
            | "background-image"
            | "background-size"
            | "background-origin"
            | "background-clip"
            | "border-top-left-radius"
            | "border-top-right-radius"
            | "border-bottom-right-radius"
            | "border-bottom-left-radius"
    )
}

fn append_raw_property_fallbacks(
    raw_block: &str,
    declarations: &mut Vec<super::compile::CompiledDeclaration>,
    properties: &[&str],
) -> Result<(), CssParseError> {
    for property in properties {
        if !raw_block.contains(&format!("{property}:")) {
            continue;
        }

        let property_name = *property;
        let already_present = declarations.iter().any(|declaration| match (property_name, declaration) {
            ("border-radius", super::compile::CompiledDeclaration::BorderRadius(_)) => true,
            ("border-color", super::compile::CompiledDeclaration::BorderColor(_)) => true,
            ("box-shadow", super::compile::CompiledDeclaration::BoxShadow(_)) => true,
            ("animation", super::compile::CompiledDeclaration::Animation(_)) => true,
            ("transition", super::compile::CompiledDeclaration::Transition(_)) => true,
            _ => false,
        });
        if already_present {
            continue;
        }

        if let Some(value) = extract_raw_property_value(raw_block, property_name) {
            let compiled = compile_declaration_from_value(
                property_name,
                &CssValue {
                    text: value.clone(),
                    components: parse_value_tokens(&value)?,
                },
            )
            .map_err(CssParseError::CssValue)?;
            declarations.push(compiled);
        }
    }

    Ok(())
}

fn extract_raw_property_value(raw_block: &str, property: &str) -> Option<String> {
    raw_block
        .split(';')
        .filter_map(|declaration| declaration.split_once(':'))
        .find_map(|(name, value)| {
            (name.trim() == property).then(|| value.trim().to_string())
        })
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
