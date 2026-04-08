use std::collections::BTreeSet;

use smithay::utils::{Logical, Point, Size};
use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::{Config, ConfigPaths};
use spiders_config::runtime::build_authoring_layout_service;
use spiders_core::OutputId;
use spiders_core::focus::{FocusTree, FocusTreeWindowGeometry};
use spiders_core::query::state_snapshot_for_model;
use spiders_core::runtime::prepared_layout::{PreparedStylesheet, PreparedStylesheets};
use spiders_core::snapshot::{StateSnapshot, WindowSnapshot};
use spiders_core::workspace::{fallback_master_stack_layout_tree, flat_workspace_root};
use spiders_core::{LayoutRect, LayoutSpace, ResolvedLayoutNode, SourceLayoutNode, WindowId};
use spiders_css::AppearanceValue;
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
    pub(crate) output_id: Option<OutputId>,
    pub(crate) location: Point<i32, Logical>,
    pub(crate) size: Size<i32, Logical>,
    pub(crate) fullscreen: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WindowAppearancePlan {
    pub(crate) appearance: AppearanceValue,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayoutComputation {
    pub(crate) primary_root: LayoutSnapshotNode,
    pub(crate) roots_by_output: std::collections::BTreeMap<OutputId, LayoutSnapshotNode>,
    pub(crate) targets: Vec<LayoutTarget>,
}

#[derive(Debug, Default)]
pub(crate) struct SceneLayoutState {
    config_paths: Option<ConfigPaths>,
    layout_service: Option<AuthoringLayoutService>,
    cache: SceneCache,
}

impl SceneLayoutState {
    pub(crate) fn new(config_paths: Option<ConfigPaths>) -> Self {
        let js_provider = JavaScriptNativeRuntimeProvider::default();
        let layout_service = config_paths
            .as_ref()
            .and_then(|paths| build_authoring_layout_service(paths, &[&js_provider]).ok());

        Self { config_paths, layout_service, cache: SceneCache::new() }
    }

    pub(crate) fn set_config_paths(&mut self, config_paths: Option<ConfigPaths>) {
        self.config_paths = config_paths;
        let js_provider = JavaScriptNativeRuntimeProvider::default();
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
        self.compute_layout_snapshot_and_targets(config, model, visible_window_ids)
            .map(|layout| layout.targets)
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

    pub(crate) fn compute_layout_snapshot_and_targets(
        &mut self,
        config: &Config,
        model: &mut WmModel,
        visible_window_ids: &[WindowId],
    ) -> Option<LayoutComputation> {
        if let Some(layout) =
            self.compute_authored_layout_snapshot_and_targets(config, model, visible_window_ids)
        {
            if targets_cover_windows(&layout.targets, visible_window_ids) {
                return Some(layout);
            }
        }

        let root = self.compute_fallback_layout_snapshot(config, model, visible_window_ids)?;
        let snapshot = scene_input_snapshot(config, model, visible_window_ids);
        let targets = scene_targets_from_snapshot(&root, &snapshot);
        targets_cover_windows(&targets, visible_window_ids).then_some(LayoutComputation {
            primary_root: root,
            roots_by_output: std::collections::BTreeMap::new(),
            targets,
        })
    }

    pub(crate) fn compute_window_appearance_plan(
        &mut self,
        config: &Config,
        model: &WmModel,
        visible_window_ids: &[WindowId],
        target_window_id: &WindowId,
    ) -> Option<WindowAppearancePlan> {
        let mut model = model.clone();
        let layout =
            self.compute_layout_snapshot_and_targets(config, &mut model, visible_window_ids)?;
        let snapshot = state_snapshot_for_model(&model);
        let target_output_id = snapshot
            .windows
            .iter()
            .find(|window| &window.id == target_window_id)
            .and_then(|window| window.output_id.as_ref());

        Self::appearance_from_roots(
            Some(&layout.primary_root),
            &layout.roots_by_output,
            target_output_id,
            target_window_id,
        )
    }

    pub(crate) fn appearance_from_roots(
        primary_root: Option<&LayoutSnapshotNode>,
        roots_by_output: &std::collections::BTreeMap<OutputId, LayoutSnapshotNode>,
        output_id: Option<&OutputId>,
        target_window_id: &WindowId,
    ) -> Option<WindowAppearancePlan> {
        let node = primary_root.and_then(|root| root.find_by_window_id(target_window_id)).or_else(
            || {
                output_id
                    .and_then(|output_id| roots_by_output.get(output_id))
                    .and_then(|root| root.find_by_window_id(target_window_id))
            },
        )?;
        let appearance = node
            .styles()
            .and_then(|styles| styles.layout.appearance)
            .unwrap_or(AppearanceValue::Auto);

        Some(WindowAppearancePlan { appearance })
    }
    fn compute_authored_layout_snapshot_and_targets(
        &mut self,
        config: &Config,
        model: &mut WmModel,
        visible_window_ids: &[WindowId],
    ) -> Option<LayoutComputation> {
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
        let resolved_root =
            resolve_layout_root(evaluation.layout.clone(), &windows, visible_window_ids);
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
                let mut roots_for_focus = vec![response.root.clone()];
                let mut target_sets = vec![scene_targets_from_snapshot(&response.root, &snapshot)];
                let mut roots_by_output = std::collections::BTreeMap::new();

                let visible_outputs = snapshot
                    .windows
                    .iter()
                    .filter(|window| visible_window_ids.iter().any(|id| id == &window.id))
                    .filter_map(|window| window.output_id.clone())
                    .collect::<BTreeSet<_>>();

                for output_id in visible_outputs {
                    if workspace.output_id.as_ref() == Some(&output_id) {
                        continue;
                    }
                    let Some(output_snapshot_state) =
                        snapshot.filtered_for_output(visible_window_ids, &output_id)
                    else {
                        continue;
                    };
                    if output_snapshot_state.visible_window_ids.is_empty() {
                        continue;
                    }

                    let output_windows = visible_window_snapshots(
                        &output_snapshot_state,
                        &output_snapshot_state.visible_window_ids,
                    );
                    let resolved_root = resolve_layout_root(
                        evaluation.layout.clone(),
                        &output_windows,
                        &output_snapshot_state.visible_window_ids,
                    );
                    let request = match config.build_scene_request_for_output_from_state(
                        &output_snapshot_state,
                        &output_id,
                        resolved_root,
                        &evaluation.artifact,
                    ) {
                        Ok(Some(request)) => request,
                        Ok(None) => continue,
                        Err(error) => {
                            warn!(%error, output = %output_id, "failed to build scene request for output state");
                            continue;
                        }
                    };

                    if let Ok(response) = self.cache.compute_layout_from_request(&request) {
                        roots_for_focus.push(response.root.clone());
                        roots_by_output.insert(output_id.clone(), response.root.clone());
                        target_sets.push(scene_targets_from_snapshot(
                            &response.root,
                            &output_snapshot_state,
                        ));
                    }
                }

                model.set_focus_tree_value(Some(focus_tree_from_roots(&roots_for_focus)));

                Some(LayoutComputation {
                    primary_root: response.root,
                    roots_by_output,
                    targets: merge_layout_targets(target_sets),
                })
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
        let output = workspace
            .output_id
            .as_ref()
            .and_then(|output_id| snapshot.output_by_id(output_id))
            .or_else(|| snapshot.current_output());
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

fn focus_tree_from_roots(roots: &[LayoutSnapshotNode]) -> FocusTree {
    let mut windows = Vec::new();
    for root in roots {
        collect_focus_tree_window_geometries(root, &mut windows);
    }
    FocusTree::from_window_geometries(&windows)
}

fn scene_input_snapshot(
    _config: &Config,
    model: &WmModel,
    visible_window_ids: &[WindowId],
) -> StateSnapshot {
    let mut snapshot = state_snapshot_for_model(model);
    snapshot.visible_window_ids = visible_window_ids.to_vec();
    for window in &mut snapshot.windows {
        if visible_window_ids.iter().any(|window_id| window_id == &window.id) {
            window.mapped = true;
        }
    }

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

pub(crate) fn scene_targets_from_snapshot(
    root: &LayoutSnapshotNode,
    snapshot: &StateSnapshot,
) -> Vec<LayoutTarget> {
    root.window_nodes()
        .into_iter()
        .filter_map(|node| match node {
            LayoutSnapshotNode::Window { rect, window_id: Some(window_id), .. } => {
                let output_id = snapshot
                    .windows
                    .iter()
                    .find(|window| window.id == *window_id)
                    .and_then(|window| window.output_id.clone());
                Some(LayoutTarget {
                    window_id: window_id.clone(),
                    output_id,
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
        LayoutSnapshotNode::Content { children, .. } => {
            for child in children {
                collect_focus_tree_window_geometries(child, out);
            }
        }
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

fn merge_layout_targets(
    target_sets: impl IntoIterator<Item = Vec<LayoutTarget>>,
) -> Vec<LayoutTarget> {
    let mut targets = target_sets.into_iter().flatten().collect::<Vec<_>>();
    targets.sort_by(|left, right| left.window_id.cmp(&right.window_id));
    targets.dedup_by(|left, right| left.window_id == right.window_id);
    targets
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
                children: vec![],
            }],
        };

        assert_eq!(
            scene_targets_from_snapshot(
                &root,
                &StateSnapshot {
                    focused_window_id: None,
                    current_output_id: None,
                    current_workspace_id: None,
                    outputs: vec![],
                    workspaces: vec![],
                    windows: vec![WindowSnapshot {
                        id: window_id(7),
                        shell: spiders_core::types::ShellKind::Unknown,
                        app_id: None,
                        title: None,
                        class: None,
                        instance: None,
                        role: None,
                        window_type: None,
                        mapped: true,
                        mode: spiders_core::types::WindowMode::Tiled,
                        focused: false,
                        urgent: false,
                        closing: false,
                        output_id: Some(OutputId::from("out-1")),
                        workspace_id: None,
                        workspaces: vec![],
                    }],
                    visible_window_ids: vec![window_id(7)],
                    workspace_names: vec![],
                },
            ),
            vec![LayoutTarget {
                window_id: window_id(7),
                output_id: Some(OutputId::from("out-1")),
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
            output_id: None,
            location: Point::from((0, 0)),
            size: Size::from((10, 10)),
            fullscreen: false,
        }];

        assert!(targets_cover_windows(&targets, &[window_id(1)]));
        assert!(!targets_cover_windows(&targets, &[window_id(1), window_id(2)]));
    }

    #[test]
    fn scene_targets_from_snapshot_preserves_window_output_affinity() {
        let root = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
                styles: None,
                window_id: Some(window_id(1)),
                children: vec![],
            }],
        };
        let snapshot = StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-a")),
            current_workspace_id: None,
            outputs: vec![],
            workspaces: vec![],
            windows: vec![WindowSnapshot {
                id: window_id(1),
                shell: spiders_core::types::ShellKind::Unknown,
                app_id: None,
                title: None,
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                mode: spiders_core::types::WindowMode::Tiled,
                focused: false,
                urgent: false,
                closing: false,
                output_id: Some(OutputId::from("out-a")),
                workspace_id: None,
                workspaces: vec![],
            }],
            visible_window_ids: vec![window_id(1)],
            workspace_names: vec![],
        };

        let targets = scene_targets_from_snapshot(&root, &snapshot);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].output_id, Some(OutputId::from("out-a")));
    }

    #[test]
    fn merge_layout_targets_prefers_one_target_per_window() {
        let merged = merge_layout_targets([
            vec![LayoutTarget {
                window_id: window_id(1),
                output_id: Some(OutputId::from("out-a")),
                location: Point::from((0, 0)),
                size: Size::from((50, 50)),
                fullscreen: false,
            }],
            vec![LayoutTarget {
                window_id: window_id(2),
                output_id: Some(OutputId::from("out-b")),
                location: Point::from((50, 0)),
                size: Size::from((50, 50)),
                fullscreen: false,
            }],
        ]);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].window_id, window_id(1));
        assert_eq!(merged[1].window_id, window_id(2));
    }

    #[test]
    fn focus_tree_from_roots_includes_windows_from_all_roots() {
        let left = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 0.0, y: 0.0, width: 50.0, height: 50.0 },
                styles: None,
                window_id: Some(window_id(1)),
                children: vec![],
            }],
        };
        let right = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 100.0, y: 0.0, width: 100.0, height: 100.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 100.0, y: 0.0, width: 50.0, height: 50.0 },
                styles: None,
                window_id: Some(window_id(2)),
                children: vec![],
            }],
        };

        let tree = focus_tree_from_roots(&[left, right]);

        assert!(tree.contains_window(&window_id(1)));
        assert!(tree.contains_window(&window_id(2)));
    }

    #[test]
    fn merge_layout_targets_deduplicates_same_window_across_outputs() {
        let merged = merge_layout_targets([
            vec![LayoutTarget {
                window_id: window_id(1),
                output_id: Some(OutputId::from("out-a")),
                location: Point::from((0, 0)),
                size: Size::from((50, 50)),
                fullscreen: false,
            }],
            vec![LayoutTarget {
                window_id: window_id(1),
                output_id: Some(OutputId::from("out-b")),
                location: Point::from((100, 0)),
                size: Size::from((50, 50)),
                fullscreen: false,
            }],
        ]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].window_id, window_id(1));
    }

    #[test]
    fn appearance_from_roots_prefers_primary_root_when_window_exists_there() {
        let primary_root = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 0.0, y: 0.0, width: 50.0, height: 50.0 },
                styles: None,
                window_id: Some(window_id(3)),
                children: vec![],
            }],
        };
        let output_root = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 100.0, y: 0.0, width: 100.0, height: 100.0 },
            styles: None,
            children: vec![],
        };
        let mut roots_by_output = std::collections::BTreeMap::new();
        roots_by_output.insert(OutputId::from("out-b"), output_root);

        let appearance = SceneLayoutState::appearance_from_roots(
            Some(&primary_root),
            &roots_by_output,
            Some(&OutputId::from("out-b")),
            &window_id(3),
        )
        .expect("appearance should resolve from primary root first");

        assert_eq!(appearance.appearance, AppearanceValue::Auto);
    }

    #[test]
    fn appearance_from_roots_falls_back_to_matching_output_root() {
        let primary_root = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            styles: None,
            children: vec![],
        };
        let output_root = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 0.0, y: 0.0, width: 50.0, height: 50.0 },
                styles: None,
                window_id: Some(window_id(9)),
                children: vec![],
            }],
        };
        let mut roots_by_output = std::collections::BTreeMap::new();
        roots_by_output.insert(OutputId::from("out-b"), output_root);

        let appearance = SceneLayoutState::appearance_from_roots(
            Some(&primary_root),
            &roots_by_output,
            Some(&OutputId::from("out-b")),
            &window_id(9),
        )
        .expect("appearance should be resolved from per-output root");

        assert_eq!(appearance.appearance, AppearanceValue::Auto);
    }

    #[test]
    fn scene_input_snapshot_marks_requested_visible_windows_mapped() {
        let mut model = WmModel::default();
        model.upsert_workspace(spiders_core::WorkspaceId::from("1"), "1".into());
        model.set_current_workspace(spiders_core::WorkspaceId::from("1"));
        model.upsert_output(spiders_core::OutputId::from("winit"), "winit", 1280, 800, None);
        model.attach_workspace_to_output(
            spiders_core::WorkspaceId::from("1"),
            spiders_core::OutputId::from("winit"),
        );
        model.set_current_output(spiders_core::OutputId::from("winit"));
        model.insert_window(
            window_id(1),
            Some(spiders_core::WorkspaceId::from("1")),
            Some(spiders_core::OutputId::from("winit")),
        );

        let snapshot = scene_input_snapshot(&Config::default(), &model, &[window_id(1)]);

        assert_eq!(snapshot.visible_window_ids, vec![window_id(1)]);
        assert!(
            snapshot
                .windows
                .iter()
                .find(|window| window.id == window_id(1))
                .is_some_and(|window| window.mapped)
        );
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

    #[test]
    fn multi_output_bridge_pieces_work_together_end_to_end() {
        let config = Config {
            layouts: vec![spiders_config::model::LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };
        let snapshot = StateSnapshot {
            focused_window_id: Some(window_id(1)),
            current_output_id: Some(OutputId::from("out-a")),
            current_workspace_id: Some(spiders_core::WorkspaceId::from("ws-a")),
            outputs: vec![
                spiders_core::snapshot::OutputSnapshot {
                    id: OutputId::from("out-a"),
                    name: "HDMI-A-1".into(),
                    logical_x: 0,
                    logical_y: 0,
                    logical_width: 1920,
                    logical_height: 1080,
                    scale: 1,
                    transform: spiders_core::types::OutputTransform::Normal,
                    enabled: true,
                    current_workspace_id: Some(spiders_core::WorkspaceId::from("ws-a")),
                },
                spiders_core::snapshot::OutputSnapshot {
                    id: OutputId::from("out-b"),
                    name: "DP-1".into(),
                    logical_x: 1920,
                    logical_y: 0,
                    logical_width: 1280,
                    logical_height: 720,
                    scale: 1,
                    transform: spiders_core::types::OutputTransform::Normal,
                    enabled: true,
                    current_workspace_id: Some(spiders_core::WorkspaceId::from("ws-b")),
                },
            ],
            workspaces: vec![
                spiders_core::snapshot::WorkspaceSnapshot {
                    id: spiders_core::WorkspaceId::from("ws-a"),
                    name: "1".into(),
                    output_id: Some(OutputId::from("out-a")),
                    active_workspaces: vec!["1".into()],
                    focused: true,
                    visible: true,
                    effective_layout: Some(spiders_core::types::LayoutRef {
                        name: "master-stack".into(),
                    }),
                },
                spiders_core::snapshot::WorkspaceSnapshot {
                    id: spiders_core::WorkspaceId::from("ws-b"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-b")),
                    active_workspaces: vec!["2".into()],
                    focused: false,
                    visible: true,
                    effective_layout: Some(spiders_core::types::LayoutRef {
                        name: "master-stack".into(),
                    }),
                },
            ],
            windows: vec![
                WindowSnapshot {
                    id: window_id(1),
                    shell: spiders_core::types::ShellKind::Unknown,
                    app_id: None,
                    title: None,
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    mode: spiders_core::types::WindowMode::Tiled,
                    focused: true,
                    urgent: false,
                    closing: false,
                    output_id: Some(OutputId::from("out-a")),
                    workspace_id: Some(spiders_core::WorkspaceId::from("ws-a")),
                    workspaces: vec!["1".into()],
                },
                WindowSnapshot {
                    id: window_id(2),
                    shell: spiders_core::types::ShellKind::Unknown,
                    app_id: None,
                    title: None,
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    mode: spiders_core::types::WindowMode::Tiled,
                    focused: false,
                    urgent: false,
                    closing: false,
                    output_id: Some(OutputId::from("out-b")),
                    workspace_id: Some(spiders_core::WorkspaceId::from("ws-b")),
                    workspaces: vec!["2".into()],
                },
            ],
            visible_window_ids: vec![window_id(1), window_id(2)],
            workspace_names: vec!["1".into(), "2".into()],
        };

        let filtered = snapshot
            .filtered_for_output(&snapshot.visible_window_ids, &OutputId::from("out-b"))
            .expect("filtered state should exist");
        let request = config
            .build_scene_request_for_output_from_state(
                &filtered,
                &OutputId::from("out-b"),
                ResolvedLayoutNode::Workspace {
                    meta: LayoutNodeMeta::default(),
                    children: vec![ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta::default(),
                        window_id: Some(window_id(2)),
                        children: vec![],
                    }],
                },
                &spiders_core::runtime::prepared_layout::PreparedLayout {
                    selected: spiders_core::runtime::prepared_layout::SelectedLayout {
                        name: "master-stack".into(),
                        directory: "layouts/master-stack".into(),
                        module: "layouts/master-stack.js".into(),
                    },
                    runtime_payload: serde_json::Value::Null,
                    stylesheets: Default::default(),
                },
            )
            .unwrap()
            .expect("request should be produced");

        let root_a = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 0.0, y: 0.0, width: 960.0, height: 1080.0 },
                styles: None,
                window_id: Some(window_id(1)),
                children: vec![],
            }],
        };
        let root_b = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 1920.0, y: 0.0, width: 1280.0, height: 720.0 },
            styles: None,
            children: vec![LayoutSnapshotNode::Window {
                meta: LayoutNodeMeta::default(),
                rect: LayoutRect { x: 1920.0, y: 0.0, width: 1280.0, height: 720.0 },
                styles: None,
                window_id: Some(window_id(2)),
                children: vec![],
            }],
        };

        let targets = merge_layout_targets([
            scene_targets_from_snapshot(&root_a, &snapshot),
            scene_targets_from_snapshot(&root_b, &filtered),
        ]);
        let tree = focus_tree_from_roots(&[root_a.clone(), root_b.clone()]);
        let mut roots_by_output = std::collections::BTreeMap::new();
        roots_by_output.insert(OutputId::from("out-b"), root_b);
        let appearance = SceneLayoutState::appearance_from_roots(
            Some(&root_a),
            &roots_by_output,
            Some(&OutputId::from("out-b")),
            &window_id(2),
        )
        .expect("appearance should resolve for secondary output window");

        assert_eq!(request.output_id, Some(OutputId::from("out-b")));
        assert_eq!(request.workspace_id, spiders_core::WorkspaceId::from("ws-b"));
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|target| target.window_id == window_id(1)));
        assert!(targets.iter().any(|target| target.window_id == window_id(2)));
        assert!(tree.contains_window(&window_id(1)));
        assert!(tree.contains_window(&window_id(2)));
        assert_eq!(appearance.appearance, AppearanceValue::Auto);
    }
}
