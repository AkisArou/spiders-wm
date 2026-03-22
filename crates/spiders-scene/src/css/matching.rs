use spiders_tree::ResolvedLayoutNode;
use super::stylo_adapter::{selector_matches_element, LayoutDomTree, LayoutSelectorImpl};

pub fn matching_rules<'a>(
    sheet: &'a super::compiled::CompiledStyleSheet,
    node: &ResolvedLayoutNode,
) -> Vec<&'a super::compiled::CompiledStyleRule> {
    sheet
        .rules
        .iter()
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
