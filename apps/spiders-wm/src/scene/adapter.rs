use std::collections::BTreeSet;

use smithay::utils::{Logical, Point, Size};
use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::{Config, ConfigPaths};
use spiders_config::runtime::build_authoring_layout_service;
use spiders_core::focus::{FocusTree, FocusTreeWindowGeometry};
use spiders_core::query::state_snapshot_for_model;
use spiders_core::runtime::prepared_layout::{PreparedStylesheet, PreparedStylesheets};
use spiders_core::snapshot::{StateSnapshot, WindowSnapshot};
use spiders_core::workspace::{fallback_master_stack_layout_tree, flat_workspace_root};
use spiders_core::{LayoutRect, LayoutSpace, ResolvedLayoutNode, SourceLayoutNode, WindowId};
use spiders_runtime_js_native::JavaScriptNativeRuntimeProvider;
use spiders_scene::ast::ValidatedLayoutTree;
use spiders_scene::pipeline::SceneCache;
use spiders_scene::{LayoutSnapshotNode, SceneRequest};
use tracing::{debug, warn};

use spiders_core::wm::WmModel;

const FALLBACK_MASTER_STACK_STYLESHEET: &str = "workspace { display: flex; flex-direction: row; width: 100%; height: 100%; } group { display: flex; flex-direction: column; height: 100%; } #main { width: 60%; } #stack { width: 40%; } window { flex-grow: 1; flex-basis: 0; }";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayoutTarget {
    pub(crate) window_id: WindowId,
    pub(crate) location: Point<i32, Logical>,
    pub(crate) size: Size<i32, Logical>,
    pub(crate) fullscreen: bool,
}

#[derive(Debug, Default)]
pub(crate) struct SceneLayoutState {
    config_paths: Option<ConfigPaths>,
    layout_service: Option<AuthoringLayoutService>,
    cache: SceneCache,
}

impl SceneLayoutState {
    pub(crate) fn new(config_paths: Option<ConfigPaths>) -> Self {
        let js_provider = JavaScriptNativeRuntimeProvider;
        let layout_service = config_paths
            .as_ref()
            .and_then(|paths| build_authoring_layout_service(paths, &[&js_provider]).ok());

        Self { config_paths, layout_service, cache: SceneCache::new() }
    }

    pub(crate) fn set_config_paths(&mut self, config_paths: Option<ConfigPaths>) {
        self.config_paths = config_paths;
        let js_provider = JavaScriptNativeRuntimeProvider;
        self.layout_service = self
            .config_paths
            .as_ref()
            .and_then(|paths| build_authoring_layout_service(paths, &[&js_provider]).ok());
        self.cache.clear();
    }

    pub(crate) fn compute_layout_targets(
        &mut self,
        config: &Config,
        model: &mut WmModel,
        visible_window_ids: &[WindowId],
    ) -> Option<Vec<LayoutTarget>> {
        if let Some(root) = self.compute_authored_layout_snapshot(config, model, visible_window_ids)
        {
            let targets = scene_targets_from_snapshot(&root);

            if targets_cover_windows(&targets, visible_window_ids) {
                return Some(targets);
            }

            warn!(
                expected_window_count = visible_window_ids.len(),
                resolved_window_count = targets.len(),
                "scene layout did not cover all visible windows; retrying with scene fallback layout"
            );
        }

        let root = self.compute_fallback_layout_snapshot(config, model, visible_window_ids)?;
        let targets = scene_targets_from_snapshot(&root);

        if targets_cover_windows(&targets, visible_window_ids) {
            Some(targets)
        } else {
            warn!(
                expected_window_count = visible_window_ids.len(),
                resolved_window_count = targets.len(),
                "scene fallback layout still did not cover all visible windows"
            );
            None
        }
    }

    pub(crate) fn compute_layout_target(
        &mut self,
        config: &Config,
        model: &mut WmModel,
        visible_window_ids: &[WindowId],
        target_window_id: &WindowId,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        self.compute_layout_targets(config, model, visible_window_ids)?
            .into_iter()
            .find(|target| &target.window_id == target_window_id)
            .map(|target| (target.location, target.size))
    }

    fn compute_authored_layout_snapshot(
        &mut self,
        config: &Config,
        model: &mut WmModel,
        visible_window_ids: &[WindowId],
    ) -> Option<LayoutSnapshotNode> {
        let layout_service = self.layout_service.as_mut()?;
        let snapshot = scene_input_snapshot(config, model, visible_window_ids);

        let workspace = snapshot.current_workspace()?.clone();
        let evaluation = match layout_service
            .evaluate_prepared_for_workspace(config, &snapshot, &workspace)
        {
            Ok(Some(evaluation)) => evaluation,
            Ok(None) => {
                debug!(workspace = %workspace.name, "no prepared layout available for current workspace");
                return None;
            }
            Err(error) => {
                warn!(%error, workspace = %workspace.name, "failed to evaluate prepared layout");
                return None;
            }
        };

        let windows = visible_window_snapshots(&snapshot, visible_window_ids);
        let resolved_root = resolve_layout_root(evaluation.layout, &windows, visible_window_ids);
        let request = match config.build_scene_request_from_state(
            &snapshot,
            resolved_root,
            &evaluation.artifact,
        ) {
            Ok(Some(request)) => request,
            Ok(None) => {
                warn!(workspace = %workspace.name, "scene request was not produced for current workspace");
                return None;
            }
            Err(error) => {
                warn!(%error, workspace = %workspace.name, "failed to build scene request from current state");
                return None;
            }
        };

        match self.cache.compute_layout_from_request(&request) {
            Ok(response) => {
                model.set_focus_tree_value(Some(focus_tree_from_snapshot(&response.root)));
                Some(response.root)
            }
            Err(error) => {
                warn!(%error, workspace = %workspace.name, "failed to compute scene layout");
                None
            }
        }
    }

    fn compute_fallback_layout_snapshot(
        &mut self,
        config: &Config,
        model: &mut WmModel,
        visible_window_ids: &[WindowId],
    ) -> Option<LayoutSnapshotNode> {
        let snapshot = scene_input_snapshot(config, model, visible_window_ids);
        let workspace = snapshot.current_workspace()?.clone();
        let output =
            workspace.output_id.as_ref().and_then(|output_id| snapshot.output_by_id(output_id));
        let windows = visible_window_snapshots(&snapshot, visible_window_ids);
        let resolved_root =
            resolve_layout_root(fallback_master_stack_layout_tree(), &windows, visible_window_ids);
        let request = SceneRequest {
            workspace_id: workspace.id,
            output_id: output.map(|output| output.id.clone()),
            layout_name: workspace.effective_layout.as_ref().map(|layout| layout.name.clone()),
            root: resolved_root,
            stylesheets: PreparedStylesheets {
                global: None,
                layout: Some(PreparedStylesheet {
                    path: "fallback://wm-master-stack.css".into(),
                    source: FALLBACK_MASTER_STACK_STYLESHEET.into(),
                }),
            },
            space: LayoutSpace {
                width: output.map(|output| output.logical_width as f32).unwrap_or_default(),
                height: output.map(|output| output.logical_height as f32).unwrap_or_default(),
            },
        };

        match self.cache.compute_layout_from_request(&request) {
            Ok(response) => {
                model.set_focus_tree_value(Some(focus_tree_from_snapshot(&response.root)));
                Some(response.root)
            }
            Err(error) => {
                warn!(%error, workspace = %workspace.name, "failed to compute fallback scene layout");
                None
            }
        }
    }
}

fn scene_input_snapshot(
    _config: &Config,
    model: &WmModel,
    visible_window_ids: &[WindowId],
) -> StateSnapshot {
    let mut snapshot = state_snapshot_for_model(model);
    snapshot.visible_window_ids = visible_window_ids.to_vec();

    snapshot
}

fn visible_window_snapshots(
    state: &StateSnapshot,
    visible_window_ids: &[WindowId],
) -> Vec<WindowSnapshot> {
    visible_window_ids
        .iter()
        .filter_map(|window_id| {
            state.windows.iter().find(|window| &window.id == window_id).cloned()
        })
        .collect()
}

fn resolve_layout_root(
    source_layout: SourceLayoutNode,
    windows: &[WindowSnapshot],
    visible_window_ids: &[WindowId],
) -> ResolvedLayoutNode {
    let validated = match ValidatedLayoutTree::new(source_layout) {
        Ok(validated) => validated,
        Err(error) => {
            warn!(%error, window_count = windows.len(), "scene layout validation failed");
            return flat_workspace_root(visible_window_ids.iter().cloned());
        }
    };

    match validated.resolve(windows) {
        Ok(resolved) => resolved.root,
        Err(error) => {
            warn!(%error, window_count = windows.len(), "scene layout resolve failed");
            flat_workspace_root(visible_window_ids.iter().cloned())
        }
    }
}

fn scene_targets_from_snapshot(root: &LayoutSnapshotNode) -> Vec<LayoutTarget> {
    root.window_nodes()
        .into_iter()
        .filter_map(|node| match node {
            LayoutSnapshotNode::Window { rect, window_id: Some(window_id), .. } => {
                Some(LayoutTarget {
                    window_id: window_id.clone(),
                    location: rect_location(*rect),
                    size: rect_size(*rect),
                    fullscreen: false,
                })
            }
            _ => None,
        })
        .collect()
}

fn focus_tree_from_snapshot(root: &LayoutSnapshotNode) -> FocusTree {
    let mut windows = Vec::new();
    collect_focus_tree_window_geometries(root, &mut windows);
    FocusTree::from_window_geometries(&windows)
}

fn collect_focus_tree_window_geometries(
    node: &LayoutSnapshotNode,
    out: &mut Vec<FocusTreeWindowGeometry>,
) {
    match node {
        LayoutSnapshotNode::Workspace { children, .. }
        | LayoutSnapshotNode::Group { children, .. } => {
            for child in children {
                collect_focus_tree_window_geometries(child, out);
            }
        }
        LayoutSnapshotNode::Window { rect, window_id: Some(window_id), .. } => {
            out.push(FocusTreeWindowGeometry {
                window_id: window_id.clone(),
                geometry: spiders_core::wm::WindowGeometry {
                    x: rect.x.round() as i32,
                    y: rect.y.round() as i32,
                    width: rect.width.round() as i32,
                    height: rect.height.round() as i32,
                },
            })
        }
        LayoutSnapshotNode::Window { window_id: None, .. } => {}
    }
}

fn rect_location(rect: LayoutRect) -> Point<i32, Logical> {
    Point::from((rect.x.round() as i32, rect.y.round() as i32))
}

fn rect_size(rect: LayoutRect) -> Size<i32, Logical> {
    Size::from(((rect.width.round() as i32).max(1), (rect.height.round() as i32).max(1)))
}

fn targets_cover_windows(targets: &[LayoutTarget], visible_window_ids: &[WindowId]) -> bool {
    let target_ids = targets.iter().map(|target| target.window_id.clone()).collect::<BTreeSet<_>>();
    let visible_ids = visible_window_ids.iter().cloned().collect::<BTreeSet<_>>();

    target_ids == visible_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::window_id;
    use spiders_core::{LayoutNodeMeta, SourceLayoutNode};

    #[test]
    fn scene_targets_from_snapshot_extracts_window_geometry() {
        let root = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 100.0, height: 50.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 10.0, y: 20.0, width: 33.6, height: 40.2 },
                styles: None,
                window_id: Some(window_id(7)),
            }],
        };

        assert_eq!(
            scene_targets_from_snapshot(&root),
            vec![LayoutTarget {
                window_id: window_id(7),
                location: Point::from((10, 20)),
                size: Size::from((34, 40)),
                fullscreen: false,
            }]
        );
    }

    #[test]
    fn targets_cover_windows_requires_exact_window_set() {
        let targets = vec![LayoutTarget {
            window_id: window_id(1),
            location: Point::from((0, 0)),
            size: Size::from((10, 10)),
            fullscreen: false,
        }];

        assert!(targets_cover_windows(&targets, &[window_id(1)]));
        assert!(!targets_cover_windows(&targets, &[window_id(1), window_id(2)]));
    }

    #[test]
    fn resolve_layout_root_falls_back_to_flat_workspace_when_invalid() {
        let root = resolve_layout_root(
            SourceLayoutNode::Window { meta: LayoutNodeMeta::default(), window_match: None },
            &[],
            &[window_id(1), window_id(2)],
        );

        let ResolvedLayoutNode::Workspace { children, .. } = root else {
            panic!("expected workspace root");
        };

        assert_eq!(children.len(), 2);
        assert!(matches!(
            &children[0],
            ResolvedLayoutNode::Window { window_id: Some(id), .. } if id == &window_id(1)
        ));
        assert!(matches!(
            &children[1],
            ResolvedLayoutNode::Window { window_id: Some(id), .. } if id == &window_id(2)
        ));
    }
}
