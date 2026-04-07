use spiders_core::ResolvedLayoutNode;

use crate::css::{CompiledStyleRule, CompiledStyleSheet};

pub fn matching_rules<'a>(
    sheet: &'a CompiledStyleSheet,
    node: &ResolvedLayoutNode,
) -> Vec<&'a CompiledStyleRule> {
    spiders_css::matching_rules(sheet, node)
}

#[cfg(test)]
pub fn selector_matches(
    selector: &selectors::parser::SelectorList<spiders_css::LayoutSelectorImpl>,
    node: &ResolvedLayoutNode,
) -> bool {
    spiders_css::selector_matches(selector, node)
}
