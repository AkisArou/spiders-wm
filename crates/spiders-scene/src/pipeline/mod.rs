use crate::css::{parse_stylesheet, CssParseError, CssValueError, StyledLayoutTree};
pub use self::layout_calc::{LaidOutNode, LaidOutTree};
use spiders_tree::ResolvedLayoutNode;
use crate::scene::{SceneRequest, SceneResponse};

mod layout_calc;
mod style_tree;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LayoutPipelineError {
    #[error(transparent)]
    CssParse(#[from] CssParseError),
    #[error(transparent)]
    CssValue(#[from] CssValueError),
    #[error(transparent)]
    LayoutCalc(#[from] self::layout_calc::LayoutCalcError),
}

pub fn build_styled_layout_tree(
    root: &ResolvedLayoutNode,
    stylesheet_source: &str,
) -> Result<StyledLayoutTree, LayoutPipelineError> {
    let sheet = parse_stylesheet(stylesheet_source)?;
    self::style_tree::build_styled_layout_tree_from_sheet(root, &sheet)
        .map_err(LayoutPipelineError::from)
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

pub fn compute_layout_from_styled(
    styled: &StyledLayoutTree,
    width: f32,
    height: f32,
) -> Result<LaidOutTree, LayoutPipelineError> {
    self::layout_calc::compute_layout_from_styled(styled, width, height)
        .map_err(LayoutPipelineError::from)
}

pub fn compute_layout_from_request(
    request: &SceneRequest,
) -> Result<SceneResponse, LayoutPipelineError> {
    let stylesheet = request.stylesheets.combined_source();
    let laid_out = compute_layout(
        &request.root,
        &stylesheet,
        request.space.width,
        request.space.height,
    )?;

    Ok(SceneResponse {
        root: laid_out.snapshot(),
    })
}

pub fn build_styled_layout_tree_from_sheet(
    root: &ResolvedLayoutNode,
    sheet: &crate::css::CompiledStyleSheet,
) -> Result<StyledLayoutTree, CssValueError> {
    self::style_tree::build_styled_layout_tree_from_sheet(root, sheet)
}

#[cfg(test)]
mod tests {
    use spiders_tree::{OutputId, WindowId, WorkspaceId};

    use super::*;
    use crate::css::{Display, FlexDirectionValue, LengthPercentage, SizeValue};
    use spiders_tree::{LayoutNodeMeta, LayoutRect, LayoutSpace, ResolvedLayoutNode};
    use crate::scene::{LayoutSnapshotNode, SceneNodeStyle, SceneResponse};

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
        assert_eq!(laid_out.root.children[0].children[1].geometry.y, 110.0);
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
                styles: Some(SceneNodeStyle {
                    layout: crate::css::ComputedStyle {
                        display: Some(Display::Flex),
                        width: Some(SizeValue::LengthPercentage(LengthPercentage::Px(400.0))),
                        height: Some(SizeValue::LengthPercentage(LengthPercentage::Px(300.0))),
                        ..crate::css::ComputedStyle::default()
                    },
                }),
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
                    styles: Some(SceneNodeStyle {
                        layout: crate::css::ComputedStyle {
                            width: Some(SizeValue::LengthPercentage(LengthPercentage::Px(200.0))),
                            ..crate::css::ComputedStyle::default()
                        },
                    }),
                    window_id: Some(WindowId::from("win-1")),
                }],
            }
        );
    }

    #[test]
    fn pipeline_supports_shared_layout_request_response_types() {
        let request = SceneRequest {
            workspace_id: WorkspaceId::from("ws-1"),
            output_id: Some(OutputId::from("out-1")),
            layout_name: Some("master-stack".into()),
            root: sample_tree(),
            stylesheets: spiders_shared::runtime::PreparedStylesheets {
                layout: Some(spiders_shared::runtime::PreparedStylesheet {
                    path: "layouts/master-stack/index.css".into(),
                    source:
                        "workspace { display: flex; width: 320px; height: 200px; } #main { width: 100px; }"
                            .into(),
                }),
            },
            space: LayoutSpace {
                width: 320.0,
                height: 200.0,
            },
        };

        let response = compute_layout_from_request(&request).unwrap();

        assert_eq!(
            response,
            SceneResponse {
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
                    styles: Some(SceneNodeStyle {
                        layout: crate::css::ComputedStyle {
                            display: Some(Display::Flex),
                            width: Some(SizeValue::LengthPercentage(LengthPercentage::Px(320.0))),
                            height: Some(SizeValue::LengthPercentage(LengthPercentage::Px(200.0))),
                            ..crate::css::ComputedStyle::default()
                        },
                    }),
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
                        styles: Some(SceneNodeStyle {
                            layout: crate::css::ComputedStyle {
                                width: Some(SizeValue::LengthPercentage(LengthPercentage::Px(100.0))),
                                ..crate::css::ComputedStyle::default()
                            },
                        }),
                        window_id: Some(WindowId::from("win-1")),
                    }],
                },
            }
        );
    }

    #[test]
    fn pipeline_sizes_workspace_root_to_request_space() {
        let tree = ResolvedLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![ResolvedLayoutNode::Group {
                meta: LayoutNodeMeta {
                    id: Some("frame".into()),
                    ..LayoutNodeMeta::default()
                },
                children: vec![ResolvedLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("master".into()),
                        class: vec!["master-slot".into()],
                        ..LayoutNodeMeta::default()
                    },
                    window_id: Some(WindowId::from("win-1")),
                }],
            }],
        };

        let laid_out = compute_layout(
            &tree,
            "#frame { display: flex; padding: 4px; width: 100%; height: 100%; } .master-slot { flex-basis: 0px; flex-grow: 1; min-width: 0px; }",
            1280.0,
            720.0,
        )
        .unwrap();

        assert_eq!(laid_out.root.geometry.width, 1280.0);
        assert_eq!(laid_out.root.geometry.height, 720.0);
        assert_eq!(laid_out.root.children[0].geometry.width, 1280.0);
        assert_eq!(laid_out.root.children[0].geometry.height, 720.0);
        assert_eq!(laid_out.root.children[0].children[0].geometry.width, 1272.0);
        assert_eq!(laid_out.root.children[0].children[0].geometry.height, 712.0);
    }
}
