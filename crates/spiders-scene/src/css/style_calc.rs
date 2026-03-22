use spiders_tree::ResolvedLayoutNode;

use super::apply::ApplyCompiledDeclaration;
use super::compile::CssValueError;
use super::compiled::CompiledStyleSheet;
use super::values::ComputedStyle;

pub fn compute_style(
    sheet: &CompiledStyleSheet,
    node: &ResolvedLayoutNode,
) -> Result<ComputedStyle, CssValueError> {
    let mut style = ComputedStyle::default();

    for rule in super::matching::matching_rules(sheet, node) {
        for declaration in &rule.declarations {
            style.apply(declaration.clone());
        }
    }

    Ok(style)
}
