use spiders_tree::ResolvedLayoutNode;

use crate::css::apply::ApplyCompiledDeclaration;
use crate::css::{CompiledStyleSheet, ComputedStyle, CssValueError, LayoutPseudoElement};

pub fn compute_style(
    sheet: &CompiledStyleSheet,
    node: &ResolvedLayoutNode,
) -> Result<ComputedStyle, CssValueError> {
    let mut style = ComputedStyle::default();

    for rule in crate::css_matching::matching_rules(sheet, node) {
        for declaration in &rule.declarations {
            style.apply(declaration.clone());
        }
    }

    Ok(style)
}

pub fn compute_style_for_pseudo(
    sheet: &CompiledStyleSheet,
    node: &ResolvedLayoutNode,
    pseudo: LayoutPseudoElement,
) -> Result<Option<ComputedStyle>, CssValueError> {
    let mut style = ComputedStyle::default();

    for rule in crate::css_matching::matching_rules_for_pseudo(sheet, node, pseudo) {
        for declaration in &rule.declarations {
            style.apply(declaration.clone());
        }
    }

    if style == ComputedStyle::default() {
        Ok(None)
    } else {
        Ok(Some(style))
    }
}