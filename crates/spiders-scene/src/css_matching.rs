use spiders_core::ResolvedLayoutNode;

use crate::css::{CompiledStyleRule, CompiledStyleSheet, LayoutPseudoElement};

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

pub fn matching_rules_for_pseudo<'a>(
	sheet: &'a CompiledStyleSheet,
	node: &ResolvedLayoutNode,
	pseudo: LayoutPseudoElement,
) -> Vec<&'a CompiledStyleRule> {
	spiders_css::matching_rules_for_pseudo(sheet, node, pseudo)
}
