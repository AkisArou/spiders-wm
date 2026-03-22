use selectors::parser::SelectorList;

use super::stylo_adapter::{LayoutPseudoElement, LayoutSelectorImpl};

use super::compile::CompiledDeclaration;

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledStyleSheet {
    pub rules: Vec<CompiledStyleRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledStyleRule {
    pub selectors: SelectorList<LayoutSelectorImpl>,
    pub target_pseudo: Option<LayoutPseudoElement>,
    pub pseudo_base_selectors: Option<SelectorList<LayoutSelectorImpl>>,
    pub declarations: Vec<CompiledDeclaration>,
}
