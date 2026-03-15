use swc_common::{
    input::StringInput, sync::Lrc, FileName, FilePathMapping, SourceMap, SourceMapper, Span,
    Spanned,
};
use swc_css_ast::Token;
use swc_css_ast::{
    AtRuleName, AttributeSelectorMatcherValue, AttributeSelectorValue, ComplexSelector,
    ComplexSelectorChildren as SwcComplexSelectorChildren, ComponentValue, DeclarationName,
    DelimiterValue, Dimension, ListOfComponentValues, QualifiedRule, QualifiedRulePrelude, Rule,
    SelectorList, SubclassSelector, TypeSelector,
};
use swc_css_codegen::{
    writer::basic::{BasicCssWriter, BasicCssWriterConfig},
    CodeGenerator, CodegenConfig, Emit,
};
use swc_css_parser::{error::Error as SwcCssError, parse_string_input, parser::ParserConfig};
use thiserror::Error;

use super::domain::{
    AttributeSelector, CssDelimiter, CssDimension, CssFunction, CssSimpleBlock, CssSimpleBlockKind,
    CssValue, CssValueToken, Declaration, NodeSelector, Selector, StyleRule, StyleSheet,
};

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

pub fn parse_stylesheet(input: &str) -> Result<StyleSheet, CssParseError> {
    let source_map = Lrc::new(SourceMap::new(FilePathMapping::empty()));
    let source_file = source_map.new_source_file(
        FileName::Custom("layout.css".into()).into(),
        input.to_owned(),
    );
    let mut errors = Vec::new();
    let stylesheet = parse_string_input::<swc_css_ast::Stylesheet>(
        StringInput::from(&*source_file),
        None,
        ParserConfig::default(),
        &mut errors,
    )
    .map_err(|err| map_swc_parse_error(&source_map, err))?;

    if let Some(err) = errors.into_iter().next() {
        return Err(map_swc_parse_error(&source_map, err));
    }

    Ok(StyleSheet {
        rules: stylesheet
            .rules
            .iter()
            .map(|rule| convert_rule(&source_map, rule))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn convert_rule(source_map: &SourceMap, rule: &Rule) -> Result<StyleRule, CssParseError> {
    match rule {
        Rule::QualifiedRule(rule) => convert_qualified_rule(source_map, rule),
        Rule::AtRule(at_rule) => Err(CssParseError::UnsupportedAtRule {
            name: at_rule_name(&at_rule.name),
        }),
        Rule::ListOfComponentValues(values) => Err(CssParseError::UnsupportedSelector {
            selector: snippet_or_fallback(source_map, values.span, "<invalid rule>"),
        }),
    }
}

fn convert_qualified_rule(
    source_map: &SourceMap,
    rule: &QualifiedRule,
) -> Result<StyleRule, CssParseError> {
    let QualifiedRulePrelude::SelectorList(selector_list) = &rule.prelude else {
        return Err(CssParseError::UnsupportedSelector {
            selector: snippet_or_fallback(source_map, rule.prelude.span(), "<selector>"),
        });
    };

    Ok(StyleRule {
        selectors: convert_selector_list(source_map, selector_list)?,
        declarations: convert_declarations(source_map, &rule.block.value)?,
    })
}

fn convert_selector_list(
    source_map: &SourceMap,
    selector_list: &SelectorList,
) -> Result<Vec<Selector>, CssParseError> {
    selector_list
        .children
        .iter()
        .map(|selector| convert_complex_selector(source_map, selector))
        .collect()
}

fn convert_complex_selector(
    source_map: &SourceMap,
    selector: &ComplexSelector,
) -> Result<Selector, CssParseError> {
    let [SwcComplexSelectorChildren::CompoundSelector(compound)] = selector.children.as_slice()
    else {
        return Err(CssParseError::UnsupportedSelector {
            selector: snippet_or_fallback(source_map, selector.span, "<selector>"),
        });
    };

    convert_compound_selector(source_map, compound)
}

fn convert_compound_selector(
    source_map: &SourceMap,
    selector: &swc_css_ast::CompoundSelector,
) -> Result<Selector, CssParseError> {
    if selector.nesting_selector.is_some() || selector.subclass_selectors.len() > 1 {
        return Err(CssParseError::UnsupportedSelector {
            selector: snippet_or_fallback(source_map, selector.span, "<selector>"),
        });
    }

    match (
        &selector.type_selector,
        selector.subclass_selectors.as_slice(),
    ) {
        (Some(type_selector), []) => convert_type_selector(source_map, type_selector),
        (None, [SubclassSelector::Id(id)]) => Ok(Selector::Id(id.text.value.to_string())),
        (None, [SubclassSelector::Class(class)]) => {
            Ok(Selector::Class(class.text.value.to_string()))
        }
        (Some(type_selector), [SubclassSelector::Attribute(attribute)]) => {
            convert_attribute_selector(source_map, type_selector, attribute)
        }
        _ => Err(CssParseError::UnsupportedSelector {
            selector: snippet_or_fallback(source_map, selector.span, "<selector>"),
        }),
    }
}

fn convert_type_selector(
    source_map: &SourceMap,
    type_selector: &TypeSelector,
) -> Result<Selector, CssParseError> {
    let name = match type_selector {
        TypeSelector::TagName(tag) if tag.name.prefix.is_none() => tag.name.value.value.as_ref(),
        _ => {
            return Err(CssParseError::UnsupportedSelector {
                selector: snippet_or_fallback(source_map, type_selector.span(), "<selector>"),
            })
        }
    };

    let selector = match name {
        "workspace" => NodeSelector::Workspace,
        "group" => NodeSelector::Group,
        "window" => NodeSelector::Window,
        _ => {
            return Err(CssParseError::UnsupportedSelector {
                selector: name.to_owned(),
            })
        }
    };

    Ok(Selector::Type(selector))
}

fn convert_attribute_selector(
    source_map: &SourceMap,
    type_selector: &TypeSelector,
    attribute: &swc_css_ast::AttributeSelector,
) -> Result<Selector, CssParseError> {
    match convert_type_selector(source_map, type_selector)? {
        Selector::Type(NodeSelector::Workspace | NodeSelector::Group | NodeSelector::Window) => {}
        _ => unreachable!(),
    }

    if attribute.name.prefix.is_some()
        || attribute.modifier.is_some()
        || !matches!(
            attribute.matcher.as_ref().map(|matcher| matcher.value),
            Some(AttributeSelectorMatcherValue::Equals)
        )
    {
        return Err(CssParseError::UnsupportedSelector {
            selector: snippet_or_fallback(source_map, attribute.span, "<selector>"),
        });
    }

    let Some(value) = &attribute.value else {
        return Err(CssParseError::UnsupportedSelector {
            selector: snippet_or_fallback(source_map, attribute.span, "<selector>"),
        });
    };

    let value = match value {
        AttributeSelectorValue::Str(value) => value.value.to_string(),
        AttributeSelectorValue::Ident(value) => value.value.to_string(),
    };

    Ok(Selector::Attribute(AttributeSelector {
        name: attribute.name.value.value.to_string(),
        value,
    }))
}

fn convert_declarations(
    source_map: &SourceMap,
    values: &[ComponentValue],
) -> Result<Vec<Declaration>, CssParseError> {
    values
        .iter()
        .filter_map(|value| match value {
            ComponentValue::Declaration(declaration) => {
                Some(convert_declaration(source_map, declaration))
            }
            ComponentValue::AtRule(at_rule) => Some(Err(CssParseError::UnsupportedAtRule {
                name: at_rule_name(&at_rule.name),
            })),
            ComponentValue::QualifiedRule(rule) => Some(Err(CssParseError::UnsupportedSelector {
                selector: snippet_or_fallback(source_map, rule.span, "<selector>"),
            })),
            ComponentValue::ListOfComponentValues(values) => {
                Some(Err(CssParseError::InvalidSyntax {
                    line: lookup_line_column(source_map, values.span).0,
                    column: lookup_line_column(source_map, values.span).1,
                }))
            }
            _ => None,
        })
        .collect()
}

fn convert_declaration(
    source_map: &SourceMap,
    declaration: &swc_css_ast::Declaration,
) -> Result<Declaration, CssParseError> {
    let property = declaration_name(&declaration.name);
    validate_property(&property)?;

    Ok(Declaration {
        property,
        value: CssValue {
            text: serialize_component_values(source_map, &declaration.value, declaration.span)?,
            components: lower_component_values(source_map, &declaration.value),
        },
    })
}

fn validate_property(property: &str) -> Result<(), CssParseError> {
    match property {
        "display"
        | "box-sizing"
        | "aspect-ratio"
        | "flex-direction"
        | "flex-wrap"
        | "flex-grow"
        | "flex-shrink"
        | "flex-basis"
        | "position"
        | "inset"
        | "top"
        | "right"
        | "bottom"
        | "left"
        | "overflow"
        | "overflow-x"
        | "overflow-y"
        | "width"
        | "height"
        | "min-width"
        | "min-height"
        | "max-width"
        | "max-height"
        | "align-items"
        | "align-self"
        | "justify-items"
        | "justify-self"
        | "align-content"
        | "justify-content"
        | "gap"
        | "row-gap"
        | "column-gap"
        | "grid-template-rows"
        | "grid-template-columns"
        | "grid-auto-rows"
        | "grid-auto-columns"
        | "grid-auto-flow"
        | "grid-template-areas"
        | "grid-row"
        | "grid-column"
        | "grid-row-start"
        | "grid-row-end"
        | "grid-column-start"
        | "grid-column-end"
        | "border-width"
        | "padding"
        | "margin" => Ok(()),
        _ => Err(CssParseError::UnsupportedProperty {
            property: property.to_owned(),
        }),
    }
}

fn lower_component_values(source_map: &SourceMap, values: &[ComponentValue]) -> Vec<CssValueToken> {
    let mut lowered = Vec::new();

    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            let previous = &values[index - 1];
            let between = Span::new(previous.span().hi(), value.span().lo());
            if let Ok(snippet) = source_map.span_to_snippet(between) {
                if snippet.chars().any(char::is_whitespace) {
                    lowered.push(CssValueToken::Whitespace);
                }
            }
        }

        lowered.push(lower_component_value(source_map, value));
    }

    lowered
}

fn lower_component_value(source_map: &SourceMap, value: &ComponentValue) -> CssValueToken {
    match value {
        ComponentValue::Ident(ident) => CssValueToken::Ident(ident.value.to_string()),
        ComponentValue::Str(value) => CssValueToken::String(value.value.to_string()),
        ComponentValue::Integer(integer) => CssValueToken::Integer(integer.value),
        ComponentValue::Number(number) => CssValueToken::Number(number.value as f32),
        ComponentValue::Percentage(percent) => {
            CssValueToken::Percentage(percent.value.value as f32)
        }
        ComponentValue::LengthPercentage(length_percentage) => match &**length_percentage {
            swc_css_ast::LengthPercentage::Length(length) => {
                CssValueToken::Dimension(CssDimension {
                    value: length.value.value as f32,
                    unit: length.unit.value.to_string(),
                })
            }
            swc_css_ast::LengthPercentage::Percentage(percent) => {
                CssValueToken::Percentage(percent.value.value as f32)
            }
        },
        ComponentValue::Dimension(dimension) => lower_dimension(dimension),
        ComponentValue::Delimiter(delimiter) => CssValueToken::Delimiter(match delimiter.value {
            DelimiterValue::Comma => CssDelimiter::Comma,
            DelimiterValue::Solidus => CssDelimiter::Solidus,
            DelimiterValue::Semicolon => CssDelimiter::Semicolon,
        }),
        ComponentValue::Function(function) => CssValueToken::Function(CssFunction {
            name: function.name.as_str().to_owned(),
            value: lower_component_values(source_map, &function.value),
        }),
        ComponentValue::SimpleBlock(block) => CssValueToken::SimpleBlock(CssSimpleBlock {
            kind: match block.name.token {
                Token::LBracket => CssSimpleBlockKind::Bracket,
                Token::LParen => CssSimpleBlockKind::Parenthesis,
                Token::LBrace => CssSimpleBlockKind::Brace,
                _ => {
                    return CssValueToken::Unknown(snippet_or_fallback(
                        source_map,
                        value.span(),
                        "<block>",
                    ))
                }
            },
            value: lower_component_values(source_map, &block.value),
        }),
        ComponentValue::PreservedToken(_) => CssValueToken::Whitespace,
        _ => CssValueToken::Unknown(snippet_or_fallback(source_map, value.span(), "<value>")),
    }
}

fn lower_dimension(dimension: &Dimension) -> CssValueToken {
    match dimension {
        Dimension::Length(length) => CssValueToken::Dimension(CssDimension {
            value: length.value.value as f32,
            unit: length.unit.value.to_string(),
        }),
        Dimension::Flex(flex) => CssValueToken::Dimension(CssDimension {
            value: flex.value.value as f32,
            unit: flex.unit.value.to_string(),
        }),
        Dimension::UnknownDimension(other) => CssValueToken::Dimension(CssDimension {
            value: other.value.value as f32,
            unit: other.unit.value.to_string(),
        }),
        _ => CssValueToken::Unknown("<dimension>".into()),
    }
}

fn declaration_name(name: &DeclarationName) -> String {
    match name {
        DeclarationName::Ident(name) => name.value.to_string(),
        DeclarationName::DashedIdent(name) => name.value.to_string(),
    }
}

fn at_rule_name(name: &AtRuleName) -> String {
    match name {
        AtRuleName::Ident(name) => name.value.to_string(),
        AtRuleName::DashedIdent(name) => name.value.to_string(),
    }
}

fn serialize_component_values(
    source_map: &SourceMap,
    values: &[ComponentValue],
    span: Span,
) -> Result<String, CssParseError> {
    if values.is_empty() {
        return Ok(String::new());
    }

    if let Some(snippet) = snippet(source_map, inner_span(values, span)) {
        return Ok(snippet);
    }

    let wrapper = ListOfComponentValues {
        span,
        children: values.to_vec(),
    };
    let mut output = String::new();
    let writer = BasicCssWriter::new(&mut output, None, BasicCssWriterConfig::default());
    let mut generator = CodeGenerator::new(writer, CodegenConfig { minify: true });
    generator
        .emit(&wrapper)
        .map_err(|_| invalid_syntax_at_span(source_map, span))?;
    Ok(output.trim().to_owned())
}

fn inner_span(values: &[ComponentValue], fallback: Span) -> Span {
    match (values.first(), values.last()) {
        (Some(first), Some(last)) => Span::new(first.span().lo(), last.span().hi()),
        _ => fallback,
    }
}

fn snippet_or_fallback(source_map: &SourceMap, span: Span, fallback: &str) -> String {
    snippet(source_map, span)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

fn snippet(source_map: &SourceMap, span: Span) -> Option<String> {
    source_map
        .span_to_snippet(span)
        .ok()
        .map(|value: String| value.trim().to_owned())
}

fn map_swc_parse_error(source_map: &SourceMap, err: SwcCssError) -> CssParseError {
    let (span, _kind) = *err.into_inner();
    invalid_syntax_at_span(source_map, span)
}

fn invalid_syntax_at_span(source_map: &SourceMap, span: Span) -> CssParseError {
    let (line, column) = lookup_line_column(source_map, span);
    CssParseError::InvalidSyntax { line, column }
}

fn lookup_line_column(source_map: &SourceMap, span: Span) -> (u32, u32) {
    let loc = source_map.lookup_char_pos(span.lo());
    (loc.line as u32, loc.col_display as u32 + 1)
}
