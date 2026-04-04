use spiders_core::ResolvedLayoutNode;

use crate::{
    CompiledStyleRule, CompiledStyleSheet, LayoutDomTree, LayoutPseudoElement,
    LayoutSelectorImpl, selector_matches_element,
};

pub fn matching_rules<'a>(
    sheet: &'a CompiledStyleSheet,
    node: &ResolvedLayoutNode,
) -> Vec<&'a CompiledStyleRule> {
    sheet
        .rules
        .iter()
        .filter(|rule| rule.target_pseudo.is_none())
        .filter(|rule| selector_matches(&rule.selectors, node))
        .collect()
}

pub fn selector_matches(
    selector: &selectors::parser::SelectorList<LayoutSelectorImpl>,
    node: &ResolvedLayoutNode,
) -> bool {
    let tree = LayoutDomTree::from_resolved_root(node);
    selector_matches_element(selector, tree.root_element())
}

pub fn matching_rules_for_pseudo<'a>(
    sheet: &'a CompiledStyleSheet,
    node: &ResolvedLayoutNode,
    pseudo: LayoutPseudoElement,
) -> Vec<&'a CompiledStyleRule> {
    sheet
        .rules
        .iter()
        .filter(|rule| rule.target_pseudo.as_ref() == Some(&pseudo))
        .filter(|rule| {
            rule.pseudo_base_selectors
                .as_ref()
                .is_some_and(|selector| selector_matches(selector, node))
        })
        .collect()
}