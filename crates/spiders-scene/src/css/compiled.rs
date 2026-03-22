use selectors::parser::SelectorList;

use super::stylo_adapter::LayoutSelectorImpl;

use super::compile::CompiledDeclaration;

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledStyleSheet {
    pub rules: Vec<CompiledStyleRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledStyleRule {
    pub selectors: SelectorList<LayoutSelectorImpl>,
    pub declarations: Vec<CompiledDeclaration>,
}
