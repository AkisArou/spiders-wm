use std::collections::BTreeSet;

use smithay::utils::{Logical, Point, Rectangle, Size};
use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::{Config, ConfigPaths};
use spiders_runtime_js::DefaultLayoutRuntime;
use spiders_scene::ast::ValidatedLayoutTree;
use spiders_scene::pipeline::SceneCache;
use spiders_scene::LayoutSnapshotNode;
use spiders_shared::snapshot::{StateSnapshot, WindowSnapshot};
use spiders_shared::types::LayoutRef;
use spiders_tree::{LayoutRect, ResolvedLayoutNode, WindowId};
use tracing::{debug, warn};

use crate::ipc::state_snapshot_for_model;
use crate::layout::{plan_tiled_slot, plan_tiled_slots};
use crate::model::wm::WmModel;

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
    layout_service: Option<AuthoringLayoutService<DefaultLayoutRuntime>>,
    cache: SceneCache,
}

impl SceneLayoutState {
    pub(crate) fn new(config_paths: Option<ConfigPaths>) -> Self {
        let layout_service = config_paths
            .as_ref()
            .map(spiders_runtime_js::build_authoring_layout_service);

        Self {
            config_paths,
            layout_service,
            cache: SceneCache::new(),
        }
    }

    pub(crate) fn set_config_paths(&mut self, config_paths: Option<ConfigPaths>) {
        self.config_paths = config_paths;
        self.layout_service = self
            .config_paths
            .as_ref()
            .map(spiders_runtime_js::build_authoring_layout_service);
        self.cache.clear();
    }

    pub(crate) fn compute_layout_targets(
        &mut self,
        config: &Config,
        model: &WmModel,
        visible_window_ids: &[WindowId],
    ) -> Option<Vec<LayoutTarget>> {
        let root = self.compute_layout_snapshot(config, model, visible_window_ids)?;
        let targets = scene_targets_from_snapshot(&root);

        if !targets_cover_windows(&targets, visible_window_ids) {
            warn!(
                expected_window_count = visible_window_ids.len(),
                resolved_window_count = targets.len(),
                "scene layout did not cover all visible windows; falling back to bootstrap planner"
            );
            return None;
        }

        Some(targets)
    }

    pub(crate) fn compute_layout_target(
        &mut self,
        config: &Config,
        model: &WmModel,
        visible_window_ids: &[WindowId],
        target_window_id: &WindowId,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        self.compute_layout_targets(config, model, visible_window_ids)?
            .into_iter()
            .find(|target| &target.window_id == target_window_id)
            .map(|target| (target.location, target.size))
    }

    fn compute_layout_snapshot(
        &mut self,
        config: &Config,
        model: &WmModel,
        visible_window_ids: &[WindowId],
    ) -> Option<LayoutSnapshotNode> {
        let layout_service = self.layout_service.as_mut()?;
        let mut snapshot = state_snapshot_for_model(model);
        snapshot.visible_window_ids = visible_window_ids.to_vec();

        let selected_layout = selected_layout_name(config, &snapshot);
        if let Some(workspace_id) = snapshot.current_workspace_id.clone()
            && let Some(workspace) = snapshot
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
        {
            workspace.effective_layout = selected_layout.clone().map(|name| LayoutRef { name });
        }

        let workspace = snapshot.current_workspace()?.clone();
        let evaluation = match layout_service.evaluate_prepared_for_workspace(config, &snapshot, &workspace) {
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
        let resolved_root = match resolve_layout_root(evaluation.layout, &windows) {
            Some(root) => root,
            None => return None,
        };

        let request = match config.build_scene_request_from_state(&snapshot, resolved_root, &evaluation.artifact) {
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
            Ok(response) => Some(response.root),
            Err(error) => {
                warn!(%error, workspace = %workspace.name, "failed to compute scene layout");
                None
            }
        }
    }
}

pub(crate) fn bootstrap_layout_target(
    output_geometry: Rectangle<i32, Logical>,
    window_order: &[WindowId],
    target_window_id: &WindowId,
) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
    let index = window_order
        .iter()
        .position(|window_id| window_id == target_window_id)?;
    let slot = plan_tiled_slot(output_geometry, window_order.len(), index)?;
    Some((slot.location, slot.size))
}

pub(crate) fn bootstrap_layout_targets(
    output_geometry: Rectangle<i32, Logical>,
    window_order: &[WindowId],
    fullscreen_window_id: Option<&WindowId>,
) -> Vec<LayoutTarget> {
    if let Some(fullscreen_window_id) = fullscreen_window_id {
        return window_order
            .iter()
            .filter(|window_id| *window_id == fullscreen_window_id)
            .map(|window_id| LayoutTarget {
                window_id: window_id.clone(),
                location: output_geometry.loc,
                size: output_geometry.size,
                fullscreen: true,
            })
            .collect();
    }

    let slots = plan_tiled_slots(output_geometry, window_order.len());
    window_order
        .iter()
        .cloned()
        .zip(slots)
        .map(|(window_id, slot)| LayoutTarget {
            window_id,
            location: slot.location,
            size: slot.size,
            fullscreen: false,
        })
        .collect()
}

fn selected_layout_name(config: &Config, state: &StateSnapshot) -> Option<String> {
    let workspace = state.current_workspace()?;

    if let Some(output_id) = workspace.output_id.as_ref()
        && let Some(output) = state.output_by_id(output_id)
        && let Some(layout_name) = config.layout_selection.per_monitor.get(&output.name)
    {
        return Some(layout_name.clone());
    }

    if let Some(index) = state
        .workspace_names
        .iter()
        .position(|workspace_name| workspace_name == &workspace.name)
        && let Some(layout_name) = config.layout_selection.per_workspace.get(index)
    {
        return Some(layout_name.clone());
    }

    config.layout_selection.default.clone()
}

fn visible_window_snapshots(
    state: &StateSnapshot,
    visible_window_ids: &[WindowId],
) -> Vec<WindowSnapshot> {
    visible_window_ids
        .iter()
        .filter_map(|window_id| {
            state
                .windows
                .iter()
                .find(|window| &window.id == window_id)
                .cloned()
        })
        .collect()
}

fn resolve_layout_root(
    source_layout: spiders_tree::SourceLayoutNode,
    windows: &[WindowSnapshot],
) -> Option<ResolvedLayoutNode> {
    let validated = match ValidatedLayoutTree::new(source_layout) {
        Ok(validated) => validated,
        Err(error) => {
            warn!(%error, window_count = windows.len(), "scene layout validation failed");
            return None;
        }
    };

    match validated.resolve(windows) {
        Ok(resolved) => Some(resolved.root),
        Err(error) => {
            warn!(%error, window_count = windows.len(), "scene layout resolve failed");
            None
        }
    }
}

fn scene_targets_from_snapshot(root: &LayoutSnapshotNode) -> Vec<LayoutTarget> {
    root.window_nodes()
        .into_iter()
        .filter_map(|node| match node {
            LayoutSnapshotNode::Window {
                rect,
                window_id: Some(window_id),
                ..
            } => Some(LayoutTarget {
                window_id: window_id.clone(),
                location: rect_location(*rect),
                size: rect_size(*rect),
                fullscreen: false,
            }),
            _ => None,
        })
        .collect()
}

fn rect_location(rect: LayoutRect) -> Point<i32, Logical> {
    Point::from((rect.x.round() as i32, rect.y.round() as i32))
}

fn rect_size(rect: LayoutRect) -> Size<i32, Logical> {
    Size::from(((rect.width.round() as i32).max(1), (rect.height.round() as i32).max(1)))
}

fn targets_cover_windows(targets: &[LayoutTarget], visible_window_ids: &[WindowId]) -> bool {
    let target_ids = targets
        .iter()
        .map(|target| target.window_id.clone())
        .collect::<BTreeSet<_>>();
    let visible_ids = visible_window_ids.iter().cloned().collect::<BTreeSet<_>>();

    target_ids == visible_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::window_id;
    use spiders_tree::LayoutNodeMeta;

    #[test]
    fn bootstrap_layout_target_resolves_slot_for_window() {
        let output = Rectangle::new((10, 20).into(), (100, 50).into());
        let windows = vec![window_id(1), window_id(2)];

        assert_eq!(
            bootstrap_layout_target(output, &windows, &window_id(2)),
            Some((Point::from((70, 20)), Size::from((40, 50))))
        );
    }

    #[test]
    fn bootstrap_layout_targets_returns_fullscreen_target_when_requested() {
        let output = Rectangle::new((10, 20).into(), (100, 50).into());
        let windows = vec![window_id(1), window_id(2)];

        assert_eq!(
            bootstrap_layout_targets(output, &windows, Some(&window_id(2))),
            vec![LayoutTarget {
                window_id: window_id(2),
                location: Point::from((10, 20)),
                size: Size::from((100, 50)),
                fullscreen: true,
            }]
        );
    }

    #[test]
    fn scene_targets_from_snapshot_extracts_window_geometry() {
        let root = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
            },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect {
                    x: 10.0,
                    y: 20.0,
                    width: 33.6,
                    height: 40.2,
                },
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
}