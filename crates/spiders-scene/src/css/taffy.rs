use taffy::geometry::Size as TaffySize;
use taffy::prelude::Dimension as TaffyDimension;
use taffy::style::Overflow as TaffyOverflow;
use taffy::style::Style as TaffyStyle;

use spiders_tree::ResolvedLayoutNode;
use super::apply::*;
use super::values::ComputedStyle;

#[derive(Debug, Clone, PartialEq)]
pub struct NodeComputedStyle {
    pub node: ResolvedLayoutNode,
    pub computed: ComputedStyle,
    pub taffy_style: TaffyStyle,
    pub children: Vec<NodeComputedStyle>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyledLayoutTree {
    pub root: NodeComputedStyle,
}

pub fn map_computed_style_to_taffy(style: &ComputedStyle) -> TaffyStyle {
    let mut taffy_style = TaffyStyle::default();

    if let Some(display) = style.display {
        taffy_style.display = map_display(display);
    }
    if let Some(box_sizing) = style.box_sizing {
        taffy_style.box_sizing = map_box_sizing(box_sizing);
    }
    if let Some(aspect_ratio) = style.aspect_ratio {
        taffy_style.aspect_ratio = Some(aspect_ratio);
    }
    if let Some(direction) = style.flex_direction {
        taffy_style.flex_direction = map_flex_direction(direction);
    }
    if let Some(wrap) = style.flex_wrap {
        taffy_style.flex_wrap = map_flex_wrap(wrap);
    }
    if let Some(flex_grow) = style.flex_grow {
        taffy_style.flex_grow = flex_grow;
    }
    if let Some(flex_shrink) = style.flex_shrink {
        taffy_style.flex_shrink = flex_shrink;
    }
    if let Some(flex_basis) = style.flex_basis {
        taffy_style.flex_basis = map_size_value(flex_basis);
    }
    if let Some(position) = style.position {
        taffy_style.position = map_position(position);
    }
    if let Some(inset) = style.inset {
        taffy_style.inset = map_box_edges(inset, map_size_value_auto);
    }
    if style.overflow_x.is_some() || style.overflow_y.is_some() {
        taffy_style.overflow = taffy::geometry::Point {
            x: style
                .overflow_x
                .map(map_overflow)
                .unwrap_or(TaffyOverflow::Visible),
            y: style
                .overflow_y
                .map(map_overflow)
                .unwrap_or(TaffyOverflow::Visible),
        };
    }
    if style.width.is_some() || style.height.is_some() {
        taffy_style.size = TaffySize {
            width: style
                .width
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
            height: style
                .height
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
        };
    }
    if style.min_width.is_some() || style.min_height.is_some() {
        taffy_style.min_size = TaffySize {
            width: style
                .min_width
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
            height: style
                .min_height
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
        };
    }
    if style.max_width.is_some() || style.max_height.is_some() {
        taffy_style.max_size = TaffySize {
            width: style
                .max_width
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
            height: style
                .max_height
                .map(map_size_value)
                .unwrap_or_else(TaffyDimension::auto),
        };
    }
    if let Some(gap) = style.gap {
        taffy_style.gap = TaffySize {
            width: map_length_percentage(gap.width),
            height: map_length_percentage(gap.height),
        };
    }
    if let Some(align_items) = style.align_items {
        taffy_style.align_items = Some(map_align_items(align_items));
    }
    if let Some(align_self) = style.align_self {
        taffy_style.align_self = Some(map_align_items(align_self));
    }
    if let Some(justify_items) = style.justify_items {
        taffy_style.justify_items = Some(map_align_items(justify_items));
    }
    if let Some(justify_self) = style.justify_self {
        taffy_style.justify_self = Some(map_align_items(justify_self));
    }
    if let Some(align_content) = style.align_content {
        taffy_style.align_content = Some(map_align_content(align_content));
    }
    if let Some(justify_content) = style.justify_content {
        taffy_style.justify_content = Some(map_justify_content(justify_content));
    }
    if let Some(tracks) = &style.grid_template_rows {
        taffy_style.grid_template_rows = tracks
            .components
            .iter()
            .map(map_grid_template_component)
            .collect();
        taffy_style.grid_template_row_names = tracks.line_names.clone();
    }
    if let Some(tracks) = &style.grid_template_columns {
        taffy_style.grid_template_columns = tracks
            .components
            .iter()
            .map(map_grid_template_component)
            .collect();
        taffy_style.grid_template_column_names = tracks.line_names.clone();
    }
    if let Some(tracks) = &style.grid_auto_rows {
        taffy_style.grid_auto_rows = tracks
            .iter()
            .copied()
            .map(map_grid_track_sizing_function)
            .collect();
    }
    if let Some(tracks) = &style.grid_auto_columns {
        taffy_style.grid_auto_columns = tracks
            .iter()
            .copied()
            .map(map_grid_track_sizing_function)
            .collect();
    }
    if let Some(flow) = style.grid_auto_flow {
        taffy_style.grid_auto_flow = map_grid_auto_flow(flow);
    }
    if let Some(areas) = &style.grid_template_areas {
        taffy_style.grid_template_areas = areas.iter().map(map_grid_template_area).collect();
    }
    if let Some(grid_row) = &style.grid_row {
        taffy_style.grid_row = map_grid_line(grid_row.clone());
    }
    if let Some(grid_column) = &style.grid_column {
        taffy_style.grid_column = map_grid_line(grid_column.clone());
    }
    if let Some(border) = style.border {
        taffy_style.border = map_box_edges(border, map_length_percentage);
    }
    if let Some(padding) = style.padding {
        taffy_style.padding = map_box_edges(padding, map_length_percentage);
    }
    if let Some(margin) = style.margin {
        taffy_style.margin = map_box_edges(margin, map_size_value_auto);
    }

    taffy_style
}
