use crate::css::{
    CssValueError, LayoutPseudoElement, NodeComputedStyle, StyledLayoutTree, compute_style,
    compute_style_for_pseudo, map_computed_style_to_taffy,
};
use spiders_core::ResolvedLayoutNode;

pub fn build_styled_layout_tree_from_sheet(
    root: &ResolvedLayoutNode,
    sheet: &crate::css::CompiledStyleSheet,
) -> Result<StyledLayoutTree, CssValueError> {
    Ok(StyledLayoutTree { root: style_node(root, sheet, None)? })
}

fn style_node(
    node: &ResolvedLayoutNode,
    sheet: &crate::css::CompiledStyleSheet,
    titlebar_root_fallback: Option<&crate::css::ComputedStyle>,
) -> Result<NodeComputedStyle, CssValueError> {
    let mut computed = compute_style(sheet, node)?;
    if let Some(fallback) = titlebar_root_fallback {
        merge_computed_style_defaults(&mut computed, fallback);
    }
    let titlebar = match node {
        ResolvedLayoutNode::Window { .. } => {
            compute_style_for_pseudo(sheet, node, LayoutPseudoElement::Titlebar)?
        }
        _ => None,
    };
    let taffy_style = map_computed_style_to_taffy(&computed);
    let children = match node {
        ResolvedLayoutNode::Workspace { children, .. }
        | ResolvedLayoutNode::Group { children, .. }
        | ResolvedLayoutNode::Window { children, .. }
        | ResolvedLayoutNode::Content { children, .. } => children
            .iter()
            .map(|child| {
                let child_titlebar_fallback = match (node, child, titlebar.as_ref()) {
                    (
                        ResolvedLayoutNode::Window { .. },
                        ResolvedLayoutNode::Content { meta, .. },
                        Some(titlebar_style),
                    ) if meta.name.as_deref() == Some("titlebar") => Some(titlebar_style),
                    _ => None,
                };

                style_node(child, sheet, child_titlebar_fallback)
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(NodeComputedStyle { node: node.clone(), computed, titlebar, taffy_style, children })
}

fn merge_computed_style_defaults(
    style: &mut crate::css::ComputedStyle,
    fallback: &crate::css::ComputedStyle,
) {
    macro_rules! inherit {
        ($($field:ident),+ $(,)?) => {
            $(
                if style.$field.is_none() {
                    style.$field = fallback.$field.clone();
                }
            )+
        };
    }

    inherit!(
        display,
        box_sizing,
        aspect_ratio,
        appearance,
        background,
        color,
        opacity,
        border_color,
        border_side_colors,
        border_style,
        border_radius,
        box_shadow,
        backdrop_filter,
        transform,
        text_align,
        text_transform,
        font_family,
        font_size,
        font_weight,
        letter_spacing,
        animation_name,
        animation_duration,
        animation_timing_function,
        animation_delay,
        animation_iteration_count,
        animation_direction,
        animation_fill_mode,
        animation_play_state,
        transition_property,
        transition_duration,
        transition_timing_function,
        transition_delay,
        flex_direction,
        flex_wrap,
        flex_grow,
        flex_shrink,
        flex_basis,
        position,
        inset,
        overflow_x,
        overflow_y,
        width,
        height,
        min_width,
        min_height,
        max_width,
        max_height,
        align_items,
        align_self,
        justify_items,
        justify_self,
        align_content,
        justify_content,
        gap,
        grid_template_rows,
        grid_template_columns,
        grid_auto_rows,
        grid_auto_columns,
        grid_auto_flow,
        grid_template_areas,
        grid_row,
        grid_column,
        border,
        padding,
        margin,
    );
}
