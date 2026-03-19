use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, SourceLocation, StyleSheetParser,
};
use spiders_shared::wm::{WindowSnapshot, WorkspaceSnapshot};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectStyleSheet {
    pub rules: Vec<EffectStyleRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectStyleRule {
    pub selectors: Vec<EffectSelector>,
    pub declarations: Vec<EffectDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedEffectRule<'a> {
    pub rule_index: usize,
    pub rule: &'a EffectStyleRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSelector {
    pub subject: EffectSelectorSubject,
    pub attributes: Vec<EffectAttributeSelector>,
    pub states: Vec<EffectPseudoState>,
    pub part: Option<EffectPseudoElement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectSelectorSubject {
    Window,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectAttributeSelector {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectPseudoState {
    Focused,
    Floating,
    Fullscreen,
    Urgent,
    Closing,
    EnterFromLeft,
    EnterFromRight,
    ExitToLeft,
    ExitToRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectPseudoElement {
    Titlebar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectDeclaration {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Auto,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowEffects {
    pub appearance: Option<Appearance>,
    pub border_width: Option<String>,
    pub border_color: Option<String>,
    pub opacity: Option<String>,
    pub border_radius: Option<String>,
    pub box_shadow: Option<String>,
    pub backdrop_filter: Option<String>,
    pub animation: Option<String>,
    pub transition: Option<String>,
    pub transform: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceEffects {
    pub opacity: Option<String>,
    pub transform: Option<String>,
    pub animation: Option<String>,
    pub transition: Option<String>,
    pub transition_property: Option<String>,
    pub transition_duration: Option<String>,
    pub transition_timing_function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TitlebarEffects {
    pub background: Option<String>,
    pub color: Option<String>,
    pub height: Option<String>,
    pub padding: Option<String>,
    pub border_bottom_width: Option<String>,
    pub border_bottom_style: Option<String>,
    pub border_bottom_color: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<String>,
    pub font_weight: Option<String>,
    pub letter_spacing: Option<String>,
    pub text_transform: Option<String>,
    pub text_align: Option<String>,
    pub box_shadow: Option<String>,
    pub border_radius: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledEffectDeclaration {
    Appearance(Appearance),
    WindowBorderWidth(String),
    WindowBorderColor(String),
    WindowOpacity(String),
    WindowBorderRadius(String),
    WindowBoxShadow(String),
    WindowBackdropFilter(String),
    WindowAnimation(String),
    WindowTransition(String),
    WindowTransform(String),
    WorkspaceOpacity(String),
    WorkspaceTransform(String),
    WorkspaceAnimation(String),
    WorkspaceTransition(String),
    WorkspaceTransitionProperty(String),
    WorkspaceTransitionDuration(String),
    WorkspaceTransitionTimingFunction(String),
    TitlebarBackground(String),
    TitlebarColor(String),
    TitlebarHeight(String),
    TitlebarPadding(String),
    TitlebarBorderBottomWidth(String),
    TitlebarBorderBottomStyle(String),
    TitlebarBorderBottomColor(String),
    TitlebarFontFamily(String),
    TitlebarFontSize(String),
    TitlebarFontWeight(String),
    TitlebarLetterSpacing(String),
    TitlebarTextTransform(String),
    TitlebarTextAlign(String),
    TitlebarBoxShadow(String),
    TitlebarBorderRadius(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectStyle {
    pub workspace: WorkspaceEffects,
    pub window: WindowEffects,
    pub titlebar: TitlebarEffects,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectTarget<'a> {
    Window(&'a WindowSnapshot),
    Workspace(&'a WorkspaceSnapshot),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EffectsCssParseError {
    #[error("unsupported selector `{selector}`")]
    UnsupportedSelector { selector: String },
    #[error("unsupported pseudo-state `:{name}`")]
    UnsupportedPseudoState { name: String },
    #[error("unsupported pseudo-element `::{name}`")]
    UnsupportedPseudoElement { name: String },
    #[error("unsupported attribute selector `{selector}`")]
    UnsupportedAttributeSelector { selector: String },
    #[error("unsupported property `{property}`")]
    UnsupportedProperty { property: String },
    #[error("invalid CSS syntax at line {line}, column {column}")]
    InvalidSyntax { line: u32, column: u32 },
    #[error("unsupported at-rule `@{name}`")]
    UnsupportedAtRule { name: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EffectsCssValueError {
    #[error("unsupported value `{value}` for `{property}`")]
    UnsupportedValue { property: String, value: String },
    #[error("property `{property}` is not valid for `{target}`")]
    InvalidTarget { property: String, target: String },
}

pub fn parse_effect_stylesheet(input: &str) -> Result<EffectStyleSheet, EffectsCssParseError> {
    let sanitized = strip_ignored_at_rules(input);
    let mut input = ParserInput::new(&sanitized);
    let mut parser = Parser::new(&mut input);
    let mut rule_parser = EffectsCssParser;
    let parser = StyleSheetParser::new(&mut parser, &mut rule_parser);

    let mut rules = Vec::new();
    for rule in parser {
        rules.push(rule.map_err(|(err, _slice)| map_parse_error(err))?);
    }

    Ok(EffectStyleSheet { rules })
}

fn strip_ignored_at_rules(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative) = input[cursor..].find("@keyframes") {
        let start = cursor + relative;
        output.push_str(&input[cursor..start]);

        let Some(block_start_rel) = input[start..].find('{') else {
            cursor = input.len();
            break;
        };
        let mut depth = 0i32;
        let mut end = start + block_start_rel;
        for (idx, ch) in input[end..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end += idx + ch.len_utf8();
                        break;
                    }
                }
                _ => {}
            }
        }
        cursor = end;
    }

    output.push_str(&input[cursor..]);
    output
}

pub fn compile_effect_declaration(
    selector: &EffectSelector,
    declaration: &EffectDeclaration,
) -> Result<CompiledEffectDeclaration, EffectsCssValueError> {
    match selector.part {
        None => match declaration.property.as_str() {
            "appearance" if selector.subject == EffectSelectorSubject::Window => {
                Ok(CompiledEffectDeclaration::Appearance(parse_appearance(
                    &declaration.property,
                    &declaration.value,
                )?))
            }
            "border-width" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowBorderWidth(declaration.value.clone()),
            ),
            "border-color" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowBorderColor(declaration.value.clone()),
            ),
            "opacity" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowOpacity(declaration.value.clone()),
            ),
            "border-radius" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowBorderRadius(declaration.value.clone()),
            ),
            "box-shadow" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowBoxShadow(declaration.value.clone()),
            ),
            "backdrop-filter" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowBackdropFilter(declaration.value.clone()),
            ),
            "animation" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowAnimation(declaration.value.clone()),
            ),
            "transition" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowTransition(declaration.value.clone()),
            ),
            "transform" if selector.subject == EffectSelectorSubject::Window => Ok(
                CompiledEffectDeclaration::WindowTransform(declaration.value.clone()),
            ),
            "opacity" if selector.subject == EffectSelectorSubject::Workspace => Ok(
                CompiledEffectDeclaration::WorkspaceOpacity(declaration.value.clone()),
            ),
            "transform" if selector.subject == EffectSelectorSubject::Workspace => Ok(
                CompiledEffectDeclaration::WorkspaceTransform(declaration.value.clone()),
            ),
            "animation" if selector.subject == EffectSelectorSubject::Workspace => Ok(
                CompiledEffectDeclaration::WorkspaceAnimation(declaration.value.clone()),
            ),
            "transition" if selector.subject == EffectSelectorSubject::Workspace => Ok(
                CompiledEffectDeclaration::WorkspaceTransition(declaration.value.clone()),
            ),
            "transition-property" if selector.subject == EffectSelectorSubject::Workspace => Ok(
                CompiledEffectDeclaration::WorkspaceTransitionProperty(declaration.value.clone()),
            ),
            "transition-duration" if selector.subject == EffectSelectorSubject::Workspace => Ok(
                CompiledEffectDeclaration::WorkspaceTransitionDuration(declaration.value.clone()),
            ),
            "transition-timing-function"
                if selector.subject == EffectSelectorSubject::Workspace =>
            {
                Ok(
                    CompiledEffectDeclaration::WorkspaceTransitionTimingFunction(
                        declaration.value.clone(),
                    ),
                )
            }
            _ => Err(EffectsCssValueError::InvalidTarget {
                property: declaration.property.clone(),
                target: selector_target_name(selector),
            }),
        },
        Some(EffectPseudoElement::Titlebar) => match declaration.property.as_str() {
            "background" => Ok(CompiledEffectDeclaration::TitlebarBackground(
                declaration.value.clone(),
            )),
            "color" => Ok(CompiledEffectDeclaration::TitlebarColor(
                declaration.value.clone(),
            )),
            "height" => Ok(CompiledEffectDeclaration::TitlebarHeight(
                declaration.value.clone(),
            )),
            "padding" => Ok(CompiledEffectDeclaration::TitlebarPadding(
                declaration.value.clone(),
            )),
            "border-bottom-width" => Ok(CompiledEffectDeclaration::TitlebarBorderBottomWidth(
                declaration.value.clone(),
            )),
            "border-bottom-style" => Ok(CompiledEffectDeclaration::TitlebarBorderBottomStyle(
                declaration.value.clone(),
            )),
            "border-bottom-color" => Ok(CompiledEffectDeclaration::TitlebarBorderBottomColor(
                declaration.value.clone(),
            )),
            "font-family" => Ok(CompiledEffectDeclaration::TitlebarFontFamily(
                declaration.value.clone(),
            )),
            "font-size" => Ok(CompiledEffectDeclaration::TitlebarFontSize(
                declaration.value.clone(),
            )),
            "font-weight" => Ok(CompiledEffectDeclaration::TitlebarFontWeight(
                declaration.value.clone(),
            )),
            "letter-spacing" => Ok(CompiledEffectDeclaration::TitlebarLetterSpacing(
                declaration.value.clone(),
            )),
            "text-transform" => Ok(CompiledEffectDeclaration::TitlebarTextTransform(
                declaration.value.clone(),
            )),
            "text-align" => Ok(CompiledEffectDeclaration::TitlebarTextAlign(
                declaration.value.clone(),
            )),
            "box-shadow" => Ok(CompiledEffectDeclaration::TitlebarBoxShadow(
                declaration.value.clone(),
            )),
            "border-radius" => Ok(CompiledEffectDeclaration::TitlebarBorderRadius(
                declaration.value.clone(),
            )),
            _ => Err(EffectsCssValueError::InvalidTarget {
                property: declaration.property.clone(),
                target: selector_target_name(selector),
            }),
        },
    }
}

pub fn matching_effect_rules<'a>(
    sheet: &'a EffectStyleSheet,
    target: EffectTarget<'_>,
    extra_states: &[EffectPseudoState],
) -> Vec<MatchedEffectRule<'a>> {
    sheet
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            rule.selectors
                .iter()
                .any(|selector| effect_selector_matches(selector, target, extra_states))
        })
        .map(|(rule_index, rule)| MatchedEffectRule { rule_index, rule })
        .collect()
}

pub fn compute_effect_style(
    sheet: &EffectStyleSheet,
    target: EffectTarget<'_>,
    extra_states: &[EffectPseudoState],
) -> Result<EffectStyle, EffectsCssValueError> {
    let mut style = EffectStyle::default();

    for rule in matching_effect_rules(sheet, target, extra_states) {
        for selector in &rule.rule.selectors {
            if !effect_selector_matches(selector, target, extra_states) {
                continue;
            }

            for declaration in &rule.rule.declarations {
                style.apply(compile_effect_declaration(selector, declaration)?);
            }
        }
    }

    Ok(style)
}

pub fn effect_selector_matches(
    selector: &EffectSelector,
    target: EffectTarget<'_>,
    extra_states: &[EffectPseudoState],
) -> bool {
    match target {
        EffectTarget::Window(window) => match_window_selector(selector, window, extra_states),
        EffectTarget::Workspace(workspace) => {
            match_workspace_selector(selector, workspace, extra_states)
        }
    }
}

impl EffectStyle {
    pub fn apply(&mut self, declaration: CompiledEffectDeclaration) {
        match declaration {
            CompiledEffectDeclaration::Appearance(value) => self.window.appearance = Some(value),
            CompiledEffectDeclaration::WindowBorderWidth(value) => {
                self.window.border_width = Some(value)
            }
            CompiledEffectDeclaration::WindowBorderColor(value) => {
                self.window.border_color = Some(value)
            }
            CompiledEffectDeclaration::WindowOpacity(value) => self.window.opacity = Some(value),
            CompiledEffectDeclaration::WindowBorderRadius(value) => {
                self.window.border_radius = Some(value)
            }
            CompiledEffectDeclaration::WindowBoxShadow(value) => {
                self.window.box_shadow = Some(value)
            }
            CompiledEffectDeclaration::WindowBackdropFilter(value) => {
                self.window.backdrop_filter = Some(value)
            }
            CompiledEffectDeclaration::WindowAnimation(value) => {
                self.window.animation = Some(value)
            }
            CompiledEffectDeclaration::WindowTransition(value) => {
                self.window.transition = Some(value)
            }
            CompiledEffectDeclaration::WindowTransform(value) => {
                self.window.transform = Some(value)
            }
            CompiledEffectDeclaration::WorkspaceOpacity(value) => {
                self.workspace.opacity = Some(value)
            }
            CompiledEffectDeclaration::WorkspaceTransform(value) => {
                self.workspace.transform = Some(value)
            }
            CompiledEffectDeclaration::WorkspaceAnimation(value) => {
                self.workspace.animation = Some(value)
            }
            CompiledEffectDeclaration::WorkspaceTransition(value) => {
                self.workspace.transition = Some(value)
            }
            CompiledEffectDeclaration::WorkspaceTransitionProperty(value) => {
                self.workspace.transition_property = Some(value)
            }
            CompiledEffectDeclaration::WorkspaceTransitionDuration(value) => {
                self.workspace.transition_duration = Some(value)
            }
            CompiledEffectDeclaration::WorkspaceTransitionTimingFunction(value) => {
                self.workspace.transition_timing_function = Some(value)
            }
            CompiledEffectDeclaration::TitlebarBackground(value) => {
                self.titlebar.background = Some(value)
            }
            CompiledEffectDeclaration::TitlebarColor(value) => self.titlebar.color = Some(value),
            CompiledEffectDeclaration::TitlebarHeight(value) => self.titlebar.height = Some(value),
            CompiledEffectDeclaration::TitlebarPadding(value) => {
                self.titlebar.padding = Some(value)
            }
            CompiledEffectDeclaration::TitlebarBorderBottomWidth(value) => {
                self.titlebar.border_bottom_width = Some(value)
            }
            CompiledEffectDeclaration::TitlebarBorderBottomStyle(value) => {
                self.titlebar.border_bottom_style = Some(value)
            }
            CompiledEffectDeclaration::TitlebarBorderBottomColor(value) => {
                self.titlebar.border_bottom_color = Some(value)
            }
            CompiledEffectDeclaration::TitlebarFontFamily(value) => {
                self.titlebar.font_family = Some(value)
            }
            CompiledEffectDeclaration::TitlebarFontSize(value) => {
                self.titlebar.font_size = Some(value)
            }
            CompiledEffectDeclaration::TitlebarFontWeight(value) => {
                self.titlebar.font_weight = Some(value)
            }
            CompiledEffectDeclaration::TitlebarLetterSpacing(value) => {
                self.titlebar.letter_spacing = Some(value)
            }
            CompiledEffectDeclaration::TitlebarTextTransform(value) => {
                self.titlebar.text_transform = Some(value)
            }
            CompiledEffectDeclaration::TitlebarTextAlign(value) => {
                self.titlebar.text_align = Some(value)
            }
            CompiledEffectDeclaration::TitlebarBoxShadow(value) => {
                self.titlebar.box_shadow = Some(value)
            }
            CompiledEffectDeclaration::TitlebarBorderRadius(value) => {
                self.titlebar.border_radius = Some(value)
            }
        }
    }
}

struct EffectsCssParser;

impl<'i> AtRuleParser<'i> for EffectsCssParser {
    type Prelude = ();
    type AtRule = EffectStyleRule;
    type Error = EffectsCssParseError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        Err(
            input.new_custom_error(EffectsCssParseError::UnsupportedAtRule {
                name: name.to_string(),
            }),
        )
    }
}

impl<'i> QualifiedRuleParser<'i> for EffectsCssParser {
    type Prelude = Vec<EffectSelector>;
    type QualifiedRule = EffectStyleRule;
    type Error = EffectsCssParseError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let start = input.position();
        input.parse_until_before(cssparser::Delimiter::CurlyBracketBlock, |input| {
            while !input.is_exhausted() {
                input.next_including_whitespace_and_comments()?;
            }
            Ok(())
        })?;
        let selector_text = input.slice_from(start).trim();
        parse_effect_selectors(selector_text).map_err(|error| input.new_custom_error(error))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        let mut declarations = Vec::new();
        let mut declaration_parser = EffectsDeclarationParser;
        let body_parser = RuleBodyParser::new(input, &mut declaration_parser);

        for item in body_parser {
            let item = match item {
                Ok(item) => item,
                Err((err, _slice)) => return Err(input.new_custom_error(map_parse_error(err))),
            };

            match item {
                EffectsRuleBodyItem::Declaration(declaration) => declarations.push(declaration),
            }
        }

        Ok(EffectStyleRule {
            selectors: prelude,
            declarations,
        })
    }
}

struct EffectsDeclarationParser;

enum EffectsRuleBodyItem {
    Declaration(EffectDeclaration),
}

impl<'i> RuleBodyItemParser<'i, EffectsRuleBodyItem, EffectsCssParseError>
    for EffectsDeclarationParser
{
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

impl<'i> DeclarationParser<'i> for EffectsDeclarationParser {
    type Declaration = EffectsRuleBodyItem;
    type Error = EffectsCssParseError;

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _declaration_start: &ParserState,
    ) -> Result<Self::Declaration, ParseError<'i, Self::Error>> {
        let property = name.to_string();
        validate_effect_property(&property).map_err(|error| input.new_custom_error(error))?;

        let start = input.position();
        while !input.is_exhausted() {
            input.next_including_whitespace_and_comments()?;
        }
        let value = input.slice_from(start).trim().to_owned();

        Ok(EffectsRuleBodyItem::Declaration(EffectDeclaration {
            property,
            value,
        }))
    }
}

impl<'i> QualifiedRuleParser<'i> for EffectsDeclarationParser {
    type Prelude = ();
    type QualifiedRule = EffectsRuleBodyItem;
    type Error = EffectsCssParseError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let start = input.position();
        let selector = input.slice_from(start).trim().to_owned();
        Err(input.new_custom_error(EffectsCssParseError::UnsupportedSelector { selector }))
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

impl<'i> AtRuleParser<'i> for EffectsDeclarationParser {
    type Prelude = ();
    type AtRule = EffectsRuleBodyItem;
    type Error = EffectsCssParseError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        Err(
            input.new_custom_error(EffectsCssParseError::UnsupportedAtRule {
                name: name.to_string(),
            }),
        )
    }
}

fn parse_effect_selectors(input: &str) -> Result<Vec<EffectSelector>, EffectsCssParseError> {
    input
        .split(',')
        .map(|selector| parse_effect_selector(selector.trim()))
        .collect()
}

fn parse_effect_selector(input: &str) -> Result<EffectSelector, EffectsCssParseError> {
    if input.is_empty() {
        return Err(EffectsCssParseError::UnsupportedSelector {
            selector: input.to_owned(),
        });
    }

    let (base, part) = split_selector_pseudo_element(input)?;

    let subject_end = base.find(['[', ':']).unwrap_or(base.len());
    let subject_name = base[..subject_end].trim();
    let mut rest = &base[subject_end..];

    let subject = match subject_name {
        "window" => EffectSelectorSubject::Window,
        "workspace" => EffectSelectorSubject::Workspace,
        _ => {
            return Err(EffectsCssParseError::UnsupportedSelector {
                selector: input.to_owned(),
            });
        }
    };

    let mut attributes = Vec::new();
    let mut states = Vec::new();

    while !rest.is_empty() {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('[') {
            let end =
                trimmed
                    .find(']')
                    .ok_or_else(|| EffectsCssParseError::UnsupportedSelector {
                        selector: input.to_owned(),
                    })?;
            let raw = &trimmed[..=end];
            attributes.push(parse_attribute_selector(raw)?);
            rest = &trimmed[end + 1..];
            continue;
        }

        if let Some(pseudo) = trimmed.strip_prefix(':') {
            let pseudo_end = pseudo.find(['[', ':']).unwrap_or(pseudo.len());
            let name = &pseudo[..pseudo_end];
            states.push(parse_pseudo_state(name)?);
            rest = &pseudo[pseudo_end..];
            continue;
        }

        return Err(EffectsCssParseError::UnsupportedSelector {
            selector: input.to_owned(),
        });
    }

    if part.is_some() && subject != EffectSelectorSubject::Window {
        return Err(EffectsCssParseError::UnsupportedSelector {
            selector: input.to_owned(),
        });
    }

    Ok(EffectSelector {
        subject,
        attributes,
        states,
        part,
    })
}

fn parse_attribute_selector(input: &str) -> Result<EffectAttributeSelector, EffectsCssParseError> {
    let Some(inner) = input
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Err(EffectsCssParseError::UnsupportedAttributeSelector {
            selector: input.to_owned(),
        });
    };
    let Some((name, value)) = inner.split_once('=') else {
        return Err(EffectsCssParseError::UnsupportedAttributeSelector {
            selector: input.to_owned(),
        });
    };
    let value = value.trim();
    let Some(value) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(EffectsCssParseError::UnsupportedAttributeSelector {
            selector: input.to_owned(),
        });
    };

    match name.trim() {
        "app_id" | "title" => Ok(EffectAttributeSelector {
            name: name.trim().to_owned(),
            value: value.to_owned(),
        }),
        _ => Err(EffectsCssParseError::UnsupportedAttributeSelector {
            selector: input.to_owned(),
        }),
    }
}

fn split_selector_pseudo_element(
    input: &str,
) -> Result<(&str, Option<EffectPseudoElement>), EffectsCssParseError> {
    let mut in_attribute = false;

    for (index, ch) in input.char_indices() {
        match ch {
            '[' => in_attribute = true,
            ']' => in_attribute = false,
            ':' if !in_attribute && input[index..].starts_with("::") => {
                let base = input[..index].trim();
                let pseudo = input[index + 2..].trim();
                return Ok((base, Some(parse_pseudo_element(pseudo)?)));
            }
            _ => {}
        }
    }

    Ok((input, None))
}

fn parse_pseudo_state(name: &str) -> Result<EffectPseudoState, EffectsCssParseError> {
    match name {
        "focused" => Ok(EffectPseudoState::Focused),
        "floating" => Ok(EffectPseudoState::Floating),
        "fullscreen" => Ok(EffectPseudoState::Fullscreen),
        "urgent" => Ok(EffectPseudoState::Urgent),
        "closing" => Ok(EffectPseudoState::Closing),
        "enter-from-left" => Ok(EffectPseudoState::EnterFromLeft),
        "enter-from-right" => Ok(EffectPseudoState::EnterFromRight),
        "exit-to-left" => Ok(EffectPseudoState::ExitToLeft),
        "exit-to-right" => Ok(EffectPseudoState::ExitToRight),
        _ => Err(EffectsCssParseError::UnsupportedPseudoState {
            name: name.to_owned(),
        }),
    }
}

fn parse_pseudo_element(name: &str) -> Result<EffectPseudoElement, EffectsCssParseError> {
    match name {
        "titlebar" => Ok(EffectPseudoElement::Titlebar),
        _ => Err(EffectsCssParseError::UnsupportedPseudoElement {
            name: name.to_owned(),
        }),
    }
}

fn validate_effect_property(property: &str) -> Result<(), EffectsCssParseError> {
    match property {
        "appearance"
        | "border-width"
        | "border-color"
        | "opacity"
        | "border-radius"
        | "backdrop-filter"
        | "animation"
        | "transition"
        | "transform"
        | "transition-property"
        | "transition-duration"
        | "transition-timing-function"
        | "background"
        | "color"
        | "height"
        | "padding"
        | "border-bottom-width"
        | "border-bottom-style"
        | "border-bottom-color"
        | "font-family"
        | "font-size"
        | "font-weight"
        | "letter-spacing"
        | "text-transform"
        | "text-align"
        | "box-shadow" => Ok(()),
        _ => Err(EffectsCssParseError::UnsupportedProperty {
            property: property.to_owned(),
        }),
    }
}

fn parse_appearance(property: &str, value: &str) -> Result<Appearance, EffectsCssValueError> {
    match value {
        "auto" => Ok(Appearance::Auto),
        "none" => Ok(Appearance::None),
        _ => Err(EffectsCssValueError::UnsupportedValue {
            property: property.to_owned(),
            value: value.to_owned(),
        }),
    }
}

fn selector_target_name(selector: &EffectSelector) -> String {
    match selector.part {
        None => match selector.subject {
            EffectSelectorSubject::Window => "window".into(),
            EffectSelectorSubject::Workspace => "workspace".into(),
        },
        Some(EffectPseudoElement::Titlebar) => "window::titlebar".into(),
    }
}

fn match_window_selector(
    selector: &EffectSelector,
    window: &WindowSnapshot,
    extra_states: &[EffectPseudoState],
) -> bool {
    if selector.subject != EffectSelectorSubject::Window {
        return false;
    }

    selector
        .attributes
        .iter()
        .all(|attribute| match_window_attribute(attribute, window))
        && selector
            .states
            .iter()
            .all(|state| window_state_matches(*state, window, extra_states))
}

fn match_workspace_selector(
    selector: &EffectSelector,
    workspace: &WorkspaceSnapshot,
    extra_states: &[EffectPseudoState],
) -> bool {
    if selector.subject != EffectSelectorSubject::Workspace {
        return false;
    }

    selector.attributes.is_empty()
        && selector
            .states
            .iter()
            .all(|state| workspace_state_matches(*state, workspace, extra_states))
}

fn match_window_attribute(attribute: &EffectAttributeSelector, window: &WindowSnapshot) -> bool {
    match attribute.name.as_str() {
        "app_id" => window.app_id.as_deref() == Some(attribute.value.as_str()),
        "title" => window.title.as_deref() == Some(attribute.value.as_str()),
        _ => false,
    }
}

fn window_state_matches(
    state: EffectPseudoState,
    window: &WindowSnapshot,
    extra_states: &[EffectPseudoState],
) -> bool {
    match state {
        EffectPseudoState::Focused => window.focused,
        EffectPseudoState::Floating => window.is_floating(),
        EffectPseudoState::Fullscreen => window.is_fullscreen(),
        EffectPseudoState::Urgent => window.urgent,
        EffectPseudoState::Closing => extra_states.contains(&EffectPseudoState::Closing),
        EffectPseudoState::EnterFromLeft
        | EffectPseudoState::EnterFromRight
        | EffectPseudoState::ExitToLeft
        | EffectPseudoState::ExitToRight => extra_states.contains(&state),
    }
}

fn workspace_state_matches(
    state: EffectPseudoState,
    workspace: &WorkspaceSnapshot,
    extra_states: &[EffectPseudoState],
) -> bool {
    match state {
        EffectPseudoState::Focused => workspace.focused,
        EffectPseudoState::EnterFromLeft
        | EffectPseudoState::EnterFromRight
        | EffectPseudoState::ExitToLeft
        | EffectPseudoState::ExitToRight => extra_states.contains(&state),
        EffectPseudoState::Floating
        | EffectPseudoState::Fullscreen
        | EffectPseudoState::Urgent
        | EffectPseudoState::Closing => false,
    }
}

fn map_parse_error(err: ParseError<'_, EffectsCssParseError>) -> EffectsCssParseError {
    match err.kind {
        cssparser::ParseErrorKind::Custom(error) => error,
        cssparser::ParseErrorKind::Basic(_) => location_error(err.location),
    }
}

fn location_error(location: SourceLocation) -> EffectsCssParseError {
    EffectsCssParseError::InvalidSyntax {
        line: location.line,
        column: location.column,
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{WindowId, WorkspaceId};
    use spiders_shared::wm::{LayoutRef, ShellKind};

    use super::*;

    fn window_snapshot() -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId::from("win-1"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("foot".into()),
            title: Some("shell".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: spiders_shared::wm::WindowMode::Tiled,
            focused: true,
            urgent: false,
            output_id: None,
            workspace_id: Some(WorkspaceId::from("ws-1")),
            workspaces: vec!["1".into()],
        }
    }

    fn workspace_snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: None,
            active_workspaces: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "master-stack".into(),
            }),
        }
    }

    #[test]
    fn parses_window_appearance_and_titlebar_rules() {
        let sheet = parse_effect_stylesheet(
            r#"
                window:focused { appearance: none; }
                window::titlebar, window:floating::titlebar {
                    background: #222;
                    color: #eee;
                    height: 24px;
                }
            "#,
        )
        .unwrap();

        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(
            sheet.rules[0].selectors[0].subject,
            EffectSelectorSubject::Window
        );
        assert_eq!(
            sheet.rules[0].selectors[0].states,
            vec![EffectPseudoState::Focused]
        );
        assert_eq!(
            sheet.rules[1].selectors[0].part,
            Some(EffectPseudoElement::Titlebar)
        );
        assert_eq!(
            sheet.rules[1].selectors[1].states,
            vec![EffectPseudoState::Floating]
        );
    }

    #[test]
    fn parses_window_attribute_selector_for_effects() {
        let sheet = parse_effect_stylesheet(r#"window[app_id="foot"]::titlebar { color: white; }"#)
            .unwrap();

        assert_eq!(
            sheet.rules[0].selectors[0].attributes,
            vec![EffectAttributeSelector {
                name: "app_id".into(),
                value: "foot".into(),
            }]
        );
    }

    #[test]
    fn rejects_dom_style_titlebar_descendant_selector() {
        let error = parse_effect_stylesheet("window .titlebar { background: red; }").unwrap_err();

        assert_eq!(
            error,
            EffectsCssParseError::UnsupportedSelector {
                selector: "window .titlebar".into(),
            }
        );
    }

    #[test]
    fn rejects_workspace_titlebar_pseudo_element() {
        let error =
            parse_effect_stylesheet("workspace::titlebar { background: red; }").unwrap_err();

        assert_eq!(
            error,
            EffectsCssParseError::UnsupportedSelector {
                selector: "workspace::titlebar".into(),
            }
        );
    }

    #[test]
    fn compiles_window_appearance() {
        let selector = EffectSelector {
            subject: EffectSelectorSubject::Window,
            attributes: Vec::new(),
            states: Vec::new(),
            part: None,
        };
        let declaration = EffectDeclaration {
            property: "appearance".into(),
            value: "none".into(),
        };

        assert_eq!(
            compile_effect_declaration(&selector, &declaration).unwrap(),
            CompiledEffectDeclaration::Appearance(Appearance::None)
        );
    }

    #[test]
    fn compiles_titlebar_style_declarations() {
        let selector = EffectSelector {
            subject: EffectSelectorSubject::Window,
            attributes: Vec::new(),
            states: vec![EffectPseudoState::Focused],
            part: Some(EffectPseudoElement::Titlebar),
        };

        let declaration = EffectDeclaration {
            property: "background".into(),
            value: "#285577".into(),
        };

        assert_eq!(
            compile_effect_declaration(&selector, &declaration).unwrap(),
            CompiledEffectDeclaration::TitlebarBackground("#285577".into())
        );
    }

    #[test]
    fn rejects_titlebar_only_property_on_window_root() {
        let selector = EffectSelector {
            subject: EffectSelectorSubject::Window,
            attributes: Vec::new(),
            states: Vec::new(),
            part: None,
        };
        let declaration = EffectDeclaration {
            property: "background".into(),
            value: "#111".into(),
        };

        assert_eq!(
            compile_effect_declaration(&selector, &declaration).unwrap_err(),
            EffectsCssValueError::InvalidTarget {
                property: "background".into(),
                target: "window".into(),
            }
        );
    }

    #[test]
    fn applies_compiled_effects_into_effect_style() {
        let mut style = EffectStyle::default();
        style.apply(CompiledEffectDeclaration::Appearance(Appearance::None));
        style.apply(CompiledEffectDeclaration::TitlebarHeight("24px".into()));

        assert_eq!(style.window.appearance, Some(Appearance::None));
        assert_eq!(style.titlebar.height.as_deref(), Some("24px"));
    }

    #[test]
    fn matches_window_selector_against_snapshot_state() {
        let selector = EffectSelector {
            subject: EffectSelectorSubject::Window,
            attributes: vec![EffectAttributeSelector {
                name: "app_id".into(),
                value: "foot".into(),
            }],
            states: vec![EffectPseudoState::Focused],
            part: Some(EffectPseudoElement::Titlebar),
        };

        assert!(effect_selector_matches(
            &selector,
            EffectTarget::Window(&window_snapshot()),
            &[]
        ));
    }

    #[test]
    fn matches_workspace_transition_state_from_extra_context() {
        let selector = EffectSelector {
            subject: EffectSelectorSubject::Workspace,
            attributes: Vec::new(),
            states: vec![EffectPseudoState::EnterFromRight],
            part: None,
        };

        assert!(effect_selector_matches(
            &selector,
            EffectTarget::Workspace(&workspace_snapshot()),
            &[EffectPseudoState::EnterFromRight]
        ));
    }

    #[test]
    fn computes_merged_effect_style_for_window_and_titlebar() {
        let sheet = parse_effect_stylesheet(
            r#"
                window { appearance: auto; }
                window[app_id="foot"] { appearance: none; }
                window::titlebar { background: #111; color: #aaa; }
                window:focused::titlebar { background: #285577; }
            "#,
        )
        .unwrap();

        let style =
            compute_effect_style(&sheet, EffectTarget::Window(&window_snapshot()), &[]).unwrap();

        assert_eq!(style.window.appearance, Some(Appearance::None));
        assert_eq!(style.titlebar.background.as_deref(), Some("#285577"));
        assert_eq!(style.titlebar.color.as_deref(), Some("#aaa"));
    }

    #[test]
    fn collects_matching_effect_rules_in_order() {
        let sheet = parse_effect_stylesheet(
            r#"
                window { appearance: auto; }
                window[title="shell"]::titlebar { height: 24px; }
                workspace:focused { appearance: none; }
            "#,
        )
        .unwrap();

        let matches = matching_effect_rules(&sheet, EffectTarget::Window(&window_snapshot()), &[]);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].rule_index, 0);
        assert_eq!(matches[1].rule_index, 1);
    }
}
