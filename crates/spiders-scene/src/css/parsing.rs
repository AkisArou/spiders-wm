use cssparser::{
    AtRuleParser, CowRcStr, Parser, ParserInput, QualifiedRuleParser, StyleSheetParser,
};

use super::stylo_adapter::{
    LayoutPseudoElement, LayoutSelectorImpl, LayoutSelectorParser, parse_selector_list_from_parser,
};

use super::compile::{CssValueError, compile_declaration, compile_declaration_from_value};
use super::compiled::{
    CompiledKeyframeStep, CompiledKeyframesRule, CompiledStyleRule, CompiledStyleSheet,
};
use super::grid::parse_grid_fallback_declarations;
use super::parse_values::{CssValue, ParsedDeclaration};
use super::tokenizer::parse_value_tokens;
use style::parser::ParserContext;
use style::properties::declaration_block::parse_property_declaration_list;
use style::stylesheets::{CssRuleType, Origin, UrlExtraData};
use style_traits::ParsingMode;
use style_traits::values::ToCss;

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
    let (sanitized, keyframes) = extract_keyframes_and_strip(input)?;
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

    Ok(CompiledStyleSheet { rules, keyframes })
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
        let _ = parse_property_declaration_list(&context, input, &[]);
        let raw_block = input.slice_from(block_start.position()).trim().to_string();
        let declarations = compile_declarations_from_raw_block(&raw_block)
            .map_err(|error| input.new_custom_error(error))?;

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

fn extract_keyframes_and_strip(
    input: &str,
) -> Result<(String, Vec<CompiledKeyframesRule>), CssParseError> {
    let mut result = String::with_capacity(input.len());
    let mut keyframes = Vec::new();
    let mut index = 0;

    while let Some(relative) = input[index..].find("@keyframes") {
        let start = index + relative;
        result.push_str(&input[index..start]);

        let open_brace_offset = input[start..]
            .find('{')
            .ok_or(CssParseError::InvalidSyntax { line: 1, column: 1 })?;
        let open_brace = start + open_brace_offset;
        let end = matching_brace_end(input, open_brace)
            .ok_or(CssParseError::InvalidSyntax { line: 1, column: 1 })?;
        let name = input[start + "@keyframes".len()..open_brace]
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        if name.is_empty() {
            return Err(CssParseError::InvalidSyntax { line: 1, column: 1 });
        }
        let body = &input[open_brace + 1..end - 1];
        keyframes.push(parse_keyframes_rule(name, body)?);
        index = end;
    }

    result.push_str(&input[index..]);

    Ok((result, keyframes))
}

fn compile_declarations_from_raw_block(
    raw_block: &str,
) -> Result<Vec<super::compile::CompiledDeclaration>, CssParseError> {
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

    let mut input_buf = ParserInput::new(raw_block);
    let mut parser = Parser::new(&mut input_buf);
    let block = parse_property_declaration_list(&context, &mut parser, &[]);
    let mut declarations = Vec::new();
    for declaration in block.normal_declaration_iter() {
        let property = declaration.id().to_css_string();
        if !SUPPORTED_PROPERTIES.contains(&property.as_str()) {
            if is_ignored_background_expansion(&property) {
                continue;
            }
            return Err(CssParseError::UnsupportedProperty { property });
        }

        if let Some(compiled) = super::stylo_compile::compile_stylo_declaration(declaration)? {
            declarations.push(compiled);
            continue;
        }

        let mut value = String::new();
        declaration
            .to_css(&mut value)
            .map_err(|_| CssParseError::InvalidSyntax { line: 1, column: 1 })?;

        let parsed = ParsedDeclaration {
            property,
            value: CssValue {
                text: value.clone(),
                components: parse_value_tokens(&value)?,
            },
        };
        let compiled = compile_declaration(&parsed).map_err(CssParseError::CssValue)?;
        declarations.push(compiled);
    }

    if raw_block.contains("appearance:")
        && !declarations.iter().any(|declaration| {
            matches!(
                declaration,
                super::compile::CompiledDeclaration::Appearance(_)
            )
        })
    {
        let fallback = parse_grid_fallback_declarations(raw_block)?;
        declarations.extend(fallback.into_iter().filter(|declaration| {
            matches!(
                declaration,
                super::compile::CompiledDeclaration::Appearance(_)
            )
        }));
    }

    if raw_block.contains("background:")
        && !declarations.iter().any(|declaration| {
            matches!(
                declaration,
                super::compile::CompiledDeclaration::Background(_)
            )
        })
    {
        let fallback = parse_grid_fallback_declarations(raw_block)?;
        declarations.extend(fallback.into_iter().filter(|declaration| {
            matches!(
                declaration,
                super::compile::CompiledDeclaration::Background(_)
            )
        }));
    }

    if raw_block.contains("border-color:")
        && !declarations.iter().any(|declaration| {
            matches!(
                declaration,
                super::compile::CompiledDeclaration::BorderColor(_)
            )
        })
    {
        append_raw_property_fallbacks(raw_block, &mut declarations, &["border-color"])?;
    }

    append_raw_property_fallbacks(
        raw_block,
        &mut declarations,
        &["border-radius", "box-shadow"],
    )?;

    if declarations.is_empty() && needs_grid_fallback(raw_block) {
        declarations = parse_grid_fallback_declarations(raw_block)?;
    }

    Ok(declarations)
}

fn parse_keyframes_rule(name: String, body: &str) -> Result<CompiledKeyframesRule, CssParseError> {
    let mut steps = Vec::new();
    let mut index = 0;

    while let Some(relative) = body[index..].find('{') {
        let block_start = index + relative;
        let selector_text = body[index..block_start].trim();
        let block_end = matching_brace_end(body, block_start)
            .ok_or(CssParseError::InvalidSyntax { line: 1, column: 1 })?;
        let declarations =
            compile_declarations_from_raw_block(&body[block_start + 1..block_end - 1])?;

        for selector in selector_text
            .split(',')
            .map(str::trim)
            .filter(|selector| !selector.is_empty())
        {
            steps.push(CompiledKeyframeStep {
                offset: parse_keyframe_offset(selector)?,
                declarations: declarations.clone(),
            });
        }

        index = block_end;
    }

    steps.sort_by(|left, right| {
        left.offset
            .partial_cmp(&right.offset)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(CompiledKeyframesRule { name, steps })
}

fn parse_keyframe_offset(selector: &str) -> Result<f32, CssParseError> {
    match selector {
        "from" => Ok(0.0),
        "to" => Ok(1.0),
        _ => selector
            .strip_suffix('%')
            .and_then(|value| value.trim().parse::<f32>().ok())
            .map(|value| (value / 100.0).clamp(0.0, 1.0))
            .ok_or(CssParseError::UnsupportedSelector {
                selector: selector.to_string(),
            }),
    }
}

fn matching_brace_end(input: &str, open_brace: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (offset, ch) in input[open_brace..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_brace + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
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
        let already_present =
            declarations
                .iter()
                .any(|declaration| match (property_name, declaration) {
                    ("border-radius", super::compile::CompiledDeclaration::BorderRadius(_)) => true,
                    ("border-color", super::compile::CompiledDeclaration::BorderColor(_)) => true,
                    ("box-shadow", super::compile::CompiledDeclaration::BoxShadow(_)) => true,
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
        .find_map(|(name, value)| (name.trim() == property).then(|| value.trim().to_string()))
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
