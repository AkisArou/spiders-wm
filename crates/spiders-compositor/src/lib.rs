use spiders_layout::pipeline::{compute_layout, LaidOutTree, LayoutPipelineError};
use spiders_shared::layout::{LayoutSnapshotNode, ResolvedLayoutNode};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CompositorLayoutError {
    #[error(transparent)]
    Pipeline(#[from] LayoutPipelineError),
}

pub trait LayoutEngine {
    fn layout_workspace(
        &self,
        root: &ResolvedLayoutNode,
        stylesheet_source: &str,
        width: f32,
        height: f32,
    ) -> Result<LayoutSnapshotNode, CompositorLayoutError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutService;

impl LayoutEngine for LayoutService {
    fn layout_workspace(
        &self,
        root: &ResolvedLayoutNode,
        stylesheet_source: &str,
        width: f32,
        height: f32,
    ) -> Result<LayoutSnapshotNode, CompositorLayoutError> {
        let laid_out: LaidOutTree = compute_layout(root, stylesheet_source, width, height)?;
        Ok(laid_out.snapshot())
    }
}

pub fn crate_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::WindowId;
    use spiders_shared::layout::{LayoutNodeMeta, LayoutRect, LayoutSnapshotNode};

    use super::*;

    #[test]
    fn layout_service_exposes_shared_snapshot_boundary() {
        let service = LayoutService;
        let root = ResolvedLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![ResolvedLayoutNode::Window {
                meta: LayoutNodeMeta {
                    id: Some("main".into()),
                    ..LayoutNodeMeta::default()
                },
                window_id: Some(WindowId::from("w1")),
            }],
        };

        let snapshot = service
            .layout_workspace(
                &root,
                "workspace { display: flex; width: 300px; height: 200px; } #main { width: 120px; }",
                300.0,
                200.0,
            )
            .unwrap();

        assert_eq!(
            snapshot,
            LayoutSnapshotNode::Workspace {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 300.0,
                    height: 200.0,
                },
                children: vec![LayoutSnapshotNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("main".into()),
                        ..LayoutNodeMeta::default()
                    },
                    rect: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 120.0,
                        height: 200.0,
                    },
                    window_id: Some(WindowId::from("w1")),
                }],
            }
        );
    }
}
