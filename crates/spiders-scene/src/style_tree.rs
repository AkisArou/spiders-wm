use crate::css::{
    CssValueError, LayoutPseudoElement, NodeComputedStyle, StyledLayoutTree, compute_style,
    compute_style_for_pseudo, map_computed_style_to_taffy,
};
use spiders_core::ResolvedLayoutNode;

pub fn build_styled_layout_tree_from_sheet(
    root: &ResolvedLayoutNode,
    sheet: &crate::css::CompiledStyleSheet,
) -> Result<StyledLayoutTree, CssValueError> {
    Ok(StyledLayoutTree {
        root: style_node(root, sheet)?,
    })
}

fn style_node(
    node: &ResolvedLayoutNode,
    sheet: &crate::css::CompiledStyleSheet,
) -> Result<NodeComputedStyle, CssValueError> {
    let computed = compute_style(sheet, node)?;
    let titlebar = match node {
        ResolvedLayoutNode::Window { .. } => {
            compute_style_for_pseudo(sheet, node, LayoutPseudoElement::Titlebar)?
        }
        _ => None,
    };
    let taffy_style = map_computed_style_to_taffy(&computed);
    let children = match node {
        ResolvedLayoutNode::Workspace { children, .. }
        | ResolvedLayoutNode::Group { children, .. } => children
            .iter()
            .map(|child| style_node(child, sheet))
            .collect::<Result<Vec<_>, _>>()?,
        ResolvedLayoutNode::Window { .. } => Vec::new(),
    };

    Ok(NodeComputedStyle {
        node: node.clone(),
        computed,
        titlebar,
        taffy_style,
        children,
    })
}
