use spiders_shared::layout::{
    LayoutRect, LayoutRequest, LayoutResponse, LayoutSnapshotNode, ResolvedLayoutNode,
};
use taffy::prelude::{AvailableSpace, Size as TaffyAvailableSize, TaffyTree};
use taffy::tree::{Layout as TaffyLayout, NodeId as TaffyNodeId};

use crate::css::{
    compute_style, map_computed_style_to_taffy, parse_stylesheet, CssParseError, CssValueError,
    NodeComputedStyle, StyledLayoutTree,
};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LayoutPipelineError {
    #[error(transparent)]
    CssParse(#[from] CssParseError),
    #[error(transparent)]
    CssValue(#[from] CssValueError),
    #[error("taffy layout failed")]
    Taffy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutNode {
    pub node: ResolvedLayoutNode,
    pub computed: crate::css::ComputedStyle,
    pub taffy_style: taffy::style::Style,
    pub geometry: LayoutGeometry,
    pub children: Vec<LaidOutNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutTree {
    pub root: LaidOutNode,
}

impl LaidOutTree {
    pub fn snapshot(&self) -> LayoutSnapshotNode {
        self.root.snapshot()
    }
}

impl LaidOutNode {
    pub fn snapshot(&self) -> LayoutSnapshotNode {
        match &self.node {
            ResolvedLayoutNode::Workspace { meta, .. } => LayoutSnapshotNode::Workspace {
                meta: meta.clone(),
                rect: self.geometry.rect(),
                children: self.children.iter().map(Self::snapshot).collect(),
            },
            ResolvedLayoutNode::Group { meta, .. } => LayoutSnapshotNode::Group {
                meta: meta.clone(),
                rect: self.geometry.rect(),
                children: self.children.iter().map(Self::snapshot).collect(),
            },
            ResolvedLayoutNode::Window {
                meta, window_id, ..
            } => LayoutSnapshotNode::Window {
                meta: meta.clone(),
                rect: self.geometry.rect(),
                window_id: window_id.clone(),
            },
        }
    }
}

impl LayoutGeometry {
    pub fn rect(&self) -> LayoutRect {
        LayoutRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

pub fn build_styled_layout_tree(
    root: &ResolvedLayoutNode,
    stylesheet_source: &str,
) -> Result<StyledLayoutTree, LayoutPipelineError> {
    let sheet = parse_stylesheet(stylesheet_source)?;
    build_styled_layout_tree_from_sheet(root, &sheet).map_err(LayoutPipelineError::from)
}

pub fn compute_layout(
    root: &ResolvedLayoutNode,
    stylesheet_source: &str,
    width: f32,
    height: f32,
) -> Result<LaidOutTree, LayoutPipelineError> {
    let styled = build_styled_layout_tree(root, stylesheet_source)?;
    compute_layout_from_styled(&styled, width, height)
}

pub fn compute_layout_from_request(
    request: &LayoutRequest,
) -> Result<LayoutResponse, LayoutPipelineError> {
    let laid_out = compute_layout(
        &request.root,
        &request.stylesheet,
        request.space.width,
        request.space.height,
    )?;

    Ok(LayoutResponse {
        root: laid_out.snapshot(),
    })
}

pub fn compute_layout_from_styled(
    styled: &StyledLayoutTree,
    width: f32,
    height: f32,
) -> Result<LaidOutTree, LayoutPipelineError> {
    let mut taffy = TaffyTree::new();
    let root_id =
        build_taffy_tree(&mut taffy, &styled.root).map_err(|_| LayoutPipelineError::Taffy)?;
    taffy
        .compute_layout(
            root_id,
            TaffyAvailableSize {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            },
        )
        .map_err(|_| LayoutPipelineError::Taffy)?;

    Ok(LaidOutTree {
        root: collect_layout(&taffy, root_id, &styled.root)
            .map_err(|_| LayoutPipelineError::Taffy)?,
    })
}

pub fn build_styled_layout_tree_from_sheet(
    root: &ResolvedLayoutNode,
    sheet: &crate::css::StyleSheet,
) -> Result<StyledLayoutTree, CssValueError> {
    Ok(StyledLayoutTree {
        root: style_node(root, sheet)?,
    })
}

fn style_node(
    node: &ResolvedLayoutNode,
    sheet: &crate::css::StyleSheet,
) -> Result<NodeComputedStyle, CssValueError> {
    let computed = compute_style(sheet, node)?;
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
        taffy_style,
        children,
    })
}

fn build_taffy_tree(
    taffy: &mut TaffyTree<()>,
    node: &NodeComputedStyle,
) -> Result<TaffyNodeId, taffy::tree::TaffyError> {
    let child_ids = node
        .children
        .iter()
        .map(|child| build_taffy_tree(taffy, child))
        .collect::<Result<Vec<_>, _>>()?;

    if child_ids.is_empty() {
        taffy.new_leaf(node.taffy_style.clone())
    } else {
        taffy.new_with_children(node.taffy_style.clone(), &child_ids)
    }
}

fn collect_layout(
    taffy: &TaffyTree<()>,
    node_id: TaffyNodeId,
    node: &NodeComputedStyle,
) -> Result<LaidOutNode, taffy::tree::TaffyError> {
    let layout = *taffy.layout(node_id)?;
    let child_ids = taffy.children(node_id)?;
    let children = node
        .children
        .iter()
        .zip(child_ids.iter())
        .map(|(child, child_id)| collect_layout(taffy, *child_id, child))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(LaidOutNode {
        node: node.node.clone(),
        computed: node.computed.clone(),
        taffy_style: node.taffy_style.clone(),
        geometry: geometry_from_layout(layout),
        children,
    })
}

fn geometry_from_layout(layout: TaffyLayout) -> LayoutGeometry {
    LayoutGeometry {
        x: layout.location.x,
        y: layout.location.y,
        width: layout.size.width,
        height: layout.size.height,
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::layout::{LayoutNodeMeta, LayoutResponse, LayoutSpace, ResolvedLayoutNode};

    use super::*;
    use crate::css::{Display, FlexDirectionValue, LengthPercentage, SizeValue};

    fn sample_tree() -> ResolvedLayoutNode {
        ResolvedLayoutNode::Workspace {
            meta: LayoutNodeMeta {
                class: vec!["root".into()],
                ..LayoutNodeMeta::default()
            },
            children: vec![ResolvedLayoutNode::Window {
                meta: LayoutNodeMeta {
                    id: Some("main".into()),
                    class: vec!["stack".into()],
                    ..LayoutNodeMeta::default()
                },
                window_id: Some(WindowId::from("win-1")),
            }],
        }
    }

    #[test]
    fn pipeline_builds_computed_styles_for_each_node() {
        let tree = sample_tree();
        let styled = build_styled_layout_tree(
            &tree,
            "workspace { display: flex; flex-direction: row; } #main { width: 60%; }",
        )
        .unwrap();

        assert_eq!(styled.root.computed.display, Some(Display::Flex));
        assert_eq!(
            styled.root.computed.flex_direction,
            Some(FlexDirectionValue::Row)
        );
        assert_eq!(styled.root.children.len(), 1);
        assert_eq!(
            styled.root.children[0].computed.width,
            Some(SizeValue::LengthPercentage(LengthPercentage::Percent(60.0)))
        );
    }

    #[test]
    fn pipeline_surfaces_stylesheet_parse_errors() {
        let tree = sample_tree();
        let error = build_styled_layout_tree(&tree, "slot { display: flex; }").unwrap_err();

        assert_eq!(
            error,
            LayoutPipelineError::CssParse(CssParseError::UnsupportedSelector {
                selector: "slot".into(),
            })
        );
    }

    #[test]
    fn pipeline_computes_basic_layout_geometry() {
        let tree = ResolvedLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![
                ResolvedLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("left".into()),
                        ..LayoutNodeMeta::default()
                    },
                    window_id: Some(WindowId::from("w1")),
                },
                ResolvedLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("right".into()),
                        ..LayoutNodeMeta::default()
                    },
                    window_id: Some(WindowId::from("w2")),
                },
            ],
        };

        let laid_out = compute_layout(
            &tree,
            "workspace { display: flex; flex-direction: row; width: 800px; height: 600px; } #left { width: 200px; } #right { flex-grow: 1; }",
            800.0,
            600.0,
        )
        .unwrap();

        assert_eq!(laid_out.root.geometry.width, 800.0);
        assert_eq!(laid_out.root.geometry.height, 600.0);
        assert_eq!(laid_out.root.children.len(), 2);
        assert_eq!(laid_out.root.children[0].geometry.x, 0.0);
        assert_eq!(laid_out.root.children[0].geometry.width, 200.0);
        assert_eq!(laid_out.root.children[1].geometry.x, 200.0);
        assert_eq!(laid_out.root.children[1].geometry.width, 600.0);
    }

    #[test]
    fn pipeline_handles_gap_padding_and_nested_groups() {
        let tree = ResolvedLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![ResolvedLayoutNode::Group {
                meta: LayoutNodeMeta {
                    id: Some("stack".into()),
                    ..LayoutNodeMeta::default()
                },
                children: vec![
                    ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta {
                            id: Some("a".into()),
                            ..LayoutNodeMeta::default()
                        },
                        window_id: Some(WindowId::from("w1")),
                    },
                    ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta {
                            id: Some("b".into()),
                            ..LayoutNodeMeta::default()
                        },
                        window_id: Some(WindowId::from("w2")),
                    },
                ],
            }],
        };

        let laid_out = compute_layout(
            &tree,
            "workspace { display: flex; width: 500px; height: 300px; padding: 10px; } #stack { display: flex; flex-direction: column; gap: 20px; width: 100%; height: 100%; } #a { height: 80px; } #b { flex-grow: 1; }",
            500.0,
            300.0,
        )
        .unwrap();

        assert_eq!(laid_out.root.children[0].geometry.x, 10.0);
        assert_eq!(laid_out.root.children[0].geometry.y, 10.0);
        assert_eq!(laid_out.root.children[0].geometry.width, 480.0);
        assert_eq!(laid_out.root.children[0].geometry.height, 280.0);
        assert_eq!(laid_out.root.children[0].children[0].geometry.height, 80.0);
        assert_eq!(laid_out.root.children[0].children[1].geometry.y, 100.0);
        assert_eq!(laid_out.root.children[0].children[1].geometry.height, 180.0);
    }

    #[test]
    fn laid_out_tree_converts_to_shared_snapshot_model() {
        let tree = sample_tree();
        let laid_out = compute_layout(
            &tree,
            "workspace { display: flex; width: 400px; height: 300px; } #main { width: 200px; }",
            400.0,
            300.0,
        )
        .unwrap();

        let snapshot = laid_out.snapshot();

        assert_eq!(
            snapshot,
            LayoutSnapshotNode::Workspace {
                meta: LayoutNodeMeta {
                    class: vec!["root".into()],
                    ..LayoutNodeMeta::default()
                },
                rect: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 300.0,
                },
                children: vec![LayoutSnapshotNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("main".into()),
                        class: vec!["stack".into()],
                        ..LayoutNodeMeta::default()
                    },
                    rect: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 200.0,
                        height: 300.0,
                    },
                    window_id: Some(WindowId::from("win-1")),
                }],
            }
        );
    }

    #[test]
    fn pipeline_supports_shared_layout_request_response_types() {
        let request = LayoutRequest {
            workspace_id: WorkspaceId::from("ws-1"),
            output_id: Some(OutputId::from("out-1")),
            layout_name: Some("master-stack".into()),
            root: sample_tree(),
            stylesheet:
                "workspace { display: flex; width: 320px; height: 200px; } #main { width: 100px; }"
                    .into(),
            space: LayoutSpace {
                width: 320.0,
                height: 200.0,
            },
        };

        let response = compute_layout_from_request(&request).unwrap();

        assert_eq!(
            response,
            LayoutResponse {
                root: LayoutSnapshotNode::Workspace {
                    meta: LayoutNodeMeta {
                        class: vec!["root".into()],
                        ..LayoutNodeMeta::default()
                    },
                    rect: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 320.0,
                        height: 200.0,
                    },
                    children: vec![LayoutSnapshotNode::Window {
                        meta: LayoutNodeMeta {
                            id: Some("main".into()),
                            class: vec!["stack".into()],
                            ..LayoutNodeMeta::default()
                        },
                        rect: LayoutRect {
                            x: 0.0,
                            y: 0.0,
                            width: 100.0,
                            height: 200.0,
                        },
                        window_id: Some(WindowId::from("win-1")),
                    }],
                },
            }
        );
    }
}
