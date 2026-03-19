use std::collections::{HashMap, HashSet};

use smithay::utils::{Logical, Rectangle};
use spiders_config::model::Config;
use spiders_layout::ast::ValidatedLayoutTree;
use spiders_layout::pipeline::compute_layout;
use spiders_shared::layout::{LayoutNodeMeta, LayoutSnapshotNode, ResolvedLayoutNode};

use crate::{
    config::ConfigRuntimeState,
    model::{OutputId, OutputNode, WindowId, WindowMode, WmState, WorkspaceId},
    transactions::LayoutRecomputePlan,
};

#[derive(Debug, Default)]
pub struct LayoutState {
    pub revision: u64,
    pub last_summary: Option<LayoutPassSummary>,
    pub desired_tiled_window_rects: HashMap<WindowId, Rectangle<i32, Logical>>,
    pub committed_tiled_window_rects: HashMap<WindowId, Rectangle<i32, Logical>>,
    pub desired_layout_snapshots: HashMap<WorkspaceId, LayoutSnapshotNode>,
    pub committed_layout_snapshots: HashMap<WorkspaceId, LayoutSnapshotNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutPassSummary {
    pub transaction_id: Option<u64>,
    pub revision: u64,
    pub recomputed_workspaces: HashSet<WorkspaceId>,
    pub full_scene: bool,
    pub window_counts: HashMap<WorkspaceId, usize>,
}

impl LayoutState {
    pub fn recompute(
        &mut self,
        wm: &WmState,
        outputs: &HashMap<OutputId, OutputNode>,
        config_runtime: &ConfigRuntimeState,
        transaction_id: Option<u64>,
        plan: &LayoutRecomputePlan,
    ) -> Option<LayoutPassSummary> {
        if !plan.full_scene && plan.workspace_roots.is_empty() {
            return None;
        }

        self.revision += 1;

        let recomputed_workspaces = if plan.full_scene {
            self.desired_tiled_window_rects.clear();
            self.desired_layout_snapshots.clear();
            wm.workspaces.keys().cloned().collect()
        } else {
            plan.workspace_roots.clone()
        };

        let window_counts = recomputed_workspaces
            .iter()
            .filter_map(|workspace_id| {
                wm.workspaces
                    .get(workspace_id)
                    .map(|workspace| (workspace_id.clone(), workspace.windows.len()))
            })
            .collect();

        for workspace_id in &recomputed_workspaces {
            self.recompute_workspace(wm, outputs, config_runtime, workspace_id);
        }

        let summary = LayoutPassSummary {
            transaction_id,
            revision: self.revision,
            recomputed_workspaces,
            full_scene: plan.full_scene,
            window_counts,
        };

        self.last_summary = Some(summary.clone());
        Some(summary)
    }

    pub fn desired_tiled_rect(&self, window_id: &WindowId) -> Option<Rectangle<i32, Logical>> {
        self.desired_tiled_window_rects.get(window_id).copied()
    }

    pub fn committed_tiled_rect(&self, window_id: &WindowId) -> Option<Rectangle<i32, Logical>> {
        self.committed_tiled_window_rects.get(window_id).copied()
    }

    pub fn commit_desired(&mut self) {
        self.committed_tiled_window_rects = self.desired_tiled_window_rects.clone();
        self.committed_layout_snapshots = self.desired_layout_snapshots.clone();
    }

    fn recompute_workspace(
        &mut self,
        wm: &WmState,
        outputs: &HashMap<OutputId, OutputNode>,
        config_runtime: &ConfigRuntimeState,
        workspace_id: &WorkspaceId,
    ) {
        let Some(workspace) = wm.workspaces.get(workspace_id) else {
            return;
        };

        for window_id in &workspace.windows {
            self.desired_tiled_window_rects.remove(window_id);
        }
        self.desired_layout_snapshots.remove(workspace_id);

        let tiled_windows = workspace
            .windows
            .iter()
            .filter(|window_id| {
                matches!(
                    wm.windows.get(*window_id).map(|window| window.mode()),
                    Some(WindowMode::Tiled)
                )
            })
            .cloned()
            .collect::<Vec<_>>();

        if tiled_windows.is_empty() {
            return;
        }

        let output_size = workspace
            .output
            .as_ref()
            .and_then(|output_id| outputs.get(output_id))
            .map(|output| output.logical_size)
            .unwrap_or((960, 640));

        let root = workspace_resolved_tree(config_runtime, workspace, outputs, wm, &tiled_windows);
        let stylesheet = workspace_stylesheet(config_runtime.current(), workspace, outputs).unwrap_or(
            "workspace { display: flex; flex-direction: row; width: 100%; height: 100%; } window { flex-basis: 0px; flex-grow: 1; min-width: 0px; height: 100%; }",
        );

        if let Ok(tree) = compute_layout(
            &root,
            stylesheet,
            output_size.0 as f32,
            output_size.1 as f32,
        ) {
            let snapshot = tree.snapshot();
            collect_tiled_rects(&snapshot, &mut self.desired_tiled_window_rects);
            self.desired_layout_snapshots
                .insert(workspace_id.clone(), snapshot);
            return;
        }

        let total_width = output_size.0 as i32;
        let total_height = output_size.1 as i32;
        let tiled_window_count = tiled_windows.len();
        let column_width = total_width / tiled_window_count as i32;

        for (index, window_id) in tiled_windows.into_iter().enumerate() {
            let x = column_width * index as i32;
            let width = if index == tiled_window_count.saturating_sub(1) {
                total_width - x
            } else {
                column_width
            };

            self.desired_tiled_window_rects.insert(
                window_id,
                Rectangle::new((x, 0).into(), (width.max(1), total_height.max(1)).into()),
            );
        }
    }
}

fn collect_tiled_rects(
    snapshot: &LayoutSnapshotNode,
    rects: &mut HashMap<WindowId, Rectangle<i32, Logical>>,
) {
    if let LayoutSnapshotNode::Window {
        window_id: Some(window_id),
        rect,
        ..
    } = snapshot
    {
        rects.insert(
            window_id.clone(),
            Rectangle::new(
                (rect.x.round() as i32, rect.y.round() as i32).into(),
                (rect.width.round() as i32, rect.height.round() as i32).into(),
            ),
        );
    }

    for child in snapshot.children() {
        collect_tiled_rects(child, rects);
    }
}

fn workspace_stylesheet<'a>(
    config: &'a Config,
    workspace: &crate::model::WorkspaceState,
    outputs: &'a HashMap<OutputId, OutputNode>,
) -> Option<&'a str> {
    let selected_name = workspace_selected_layout_name(config, workspace, outputs)?;
    config
        .layout_by_name(selected_name)
        .map(|layout| layout.stylesheet.as_str())
}

fn workspace_selected_layout_name<'a>(
    config: &'a Config,
    workspace: &crate::model::WorkspaceState,
    outputs: &'a HashMap<OutputId, OutputNode>,
) -> Option<&'a str> {
    let from_monitor = workspace
        .output
        .as_ref()
        .and_then(|output_id| outputs.get(output_id))
        .and_then(|output| config.layout_selection.per_monitor.get(&output.name))
        .map(String::as_str);

    let from_workspace_list = config
        .workspaces
        .iter()
        .position(|name| name == &workspace.name)
        .and_then(|index| config.layout_selection.per_workspace.get(index))
        .map(String::as_str);

    let from_numeric_name = workspace
        .name
        .parse::<usize>()
        .ok()
        .and_then(|index| {
            config
                .layout_selection
                .per_workspace
                .get(index.saturating_sub(1))
        })
        .map(String::as_str);

    from_monitor
        .or(from_workspace_list)
        .or(from_numeric_name)
        .or(config.layout_selection.default.as_deref())
}

fn workspace_resolved_tree(
    config_runtime: &ConfigRuntimeState,
    workspace: &crate::model::WorkspaceState,
    outputs: &HashMap<OutputId, OutputNode>,
    wm: &WmState,
    tiled_windows: &[WindowId],
) -> ResolvedLayoutNode {
    let Some(layout_name) =
        workspace_selected_layout_name(config_runtime.current(), workspace, outputs)
    else {
        return default_resolved_tree(tiled_windows);
    };

    let Some(source_tree) = config_runtime.layout_tree(layout_name) else {
        return default_resolved_tree(tiled_windows);
    };

    let Ok(validated) = ValidatedLayoutTree::new(source_tree.clone()) else {
        return default_resolved_tree(tiled_windows);
    };

    let windows = tiled_windows
        .iter()
        .filter_map(|window_id| {
            wm.windows
                .get(window_id)
                .map(|window| window.snapshot(wm.focused_window.as_ref() == Some(&window.id)))
        })
        .collect::<Vec<_>>();

    validated
        .resolve(&windows)
        .map(|resolved| resolved.root)
        .unwrap_or_else(|_| default_resolved_tree(tiled_windows))
}

fn default_resolved_tree(tiled_windows: &[WindowId]) -> ResolvedLayoutNode {
    ResolvedLayoutNode::Workspace {
        meta: LayoutNodeMeta::default(),
        children: tiled_windows
            .iter()
            .map(|window_id| ResolvedLayoutNode::Window {
                meta: LayoutNodeMeta::default(),
                window_id: Some(window_id.clone()),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use spiders_config::model::{Config, LayoutDefinition, LayoutSelectionConfig};
    use spiders_shared::layout::{LayoutNodeMeta, RemainingTake, SlotTake, SourceLayoutNode};

    use super::LayoutState;
    use crate::{
        config::{ConfigRuntimeState, ConfigSource},
        model::{
            ManagedWindowState, OutputId, OutputNode, WindowId, WmState, WorkspaceId,
            WorkspaceState,
        },
        transactions::LayoutRecomputePlan,
    };

    fn wm_fixture() -> WmState {
        let mut wm = WmState::default();
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: Some(OutputId::from("out-1")),
                windows: vec![WindowId::from("w1"), WindowId::from("w2")],
            },
        );
        wm.workspaces.insert(
            WorkspaceId::from("ws-2"),
            WorkspaceState {
                id: WorkspaceId::from("ws-2"),
                name: "ws-2".into(),
                output: Some(OutputId::from("out-2")),
                windows: vec![WindowId::from("w3")],
            },
        );
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );
        wm.windows.insert(
            WindowId::from("w2"),
            ManagedWindowState::tiled(
                WindowId::from("w2"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );
        wm.windows.insert(
            WindowId::from("w3"),
            ManagedWindowState::tiled(
                WindowId::from("w3"),
                WorkspaceId::from("ws-2"),
                Some(OutputId::from("out-2")),
            ),
        );
        wm
    }

    fn outputs_fixture() -> HashMap<OutputId, OutputNode> {
        HashMap::from([
            (
                OutputId::from("out-1"),
                OutputNode {
                    id: OutputId::from("out-1"),
                    name: "out-1".into(),
                    enabled: true,
                    current_workspace: Some(WorkspaceId::from("ws-1")),
                    logical_size: (1200, 700),
                },
            ),
            (
                OutputId::from("out-2"),
                OutputNode {
                    id: OutputId::from("out-2"),
                    name: "out-2".into(),
                    enabled: true,
                    current_workspace: Some(WorkspaceId::from("ws-2")),
                    logical_size: (900, 600),
                },
            ),
        ])
    }

    fn config_runtime_fixture() -> ConfigRuntimeState {
        let config = Config {
            workspaces: vec!["alpha".into(), "beta".into()],
            layouts: vec![
                LayoutDefinition {
                    name: "columns".into(),
                    module: "layouts/columns.js".into(),
                    stylesheet: "workspace { display: flex; flex-direction: row; width: 100%; height: 100%; } window { flex-basis: 0px; flex-grow: 1; min-width: 0px; height: 100%; }".into(),
                    effects_stylesheet: String::new(),
                    runtime_graph: None,
                },
                LayoutDefinition {
                    name: "rows".into(),
                    module: "layouts/rows.js".into(),
                    stylesheet: "workspace { display: flex; flex-direction: column; width: 100%; height: 100%; } window { flex-basis: 0px; flex-grow: 1; min-height: 0px; width: 100%; }".into(),
                    effects_stylesheet: String::new(),
                    runtime_graph: None,
                },
            ],
            layout_selection: LayoutSelectionConfig {
                default: Some("columns".into()),
                per_workspace: vec!["rows".into(), "columns".into()],
                per_monitor: Default::default(),
            },
            ..Config::default()
        };

        let mut runtime = ConfigRuntimeState::default();
        runtime.replace(config, ConfigSource::PreparedConfig);
        runtime
    }

    #[test]
    fn recompute_skips_empty_plan() {
        let mut layout = LayoutState::default();

        let summary = layout.recompute(
            &wm_fixture(),
            &outputs_fixture(),
            &config_runtime_fixture(),
            Some(7),
            &LayoutRecomputePlan::default(),
        );

        assert!(summary.is_none());
        assert_eq!(layout.revision, 0);
    }

    #[test]
    fn recompute_tracks_workspace_roots() {
        let mut layout = LayoutState::default();
        let summary = layout
            .recompute(
                &wm_fixture(),
                &outputs_fixture(),
                &config_runtime_fixture(),
                Some(8),
                &LayoutRecomputePlan {
                    workspace_roots: HashSet::from([WorkspaceId::from("ws-1")]),
                    full_scene: false,
                },
            )
            .unwrap();

        assert_eq!(
            summary.recomputed_workspaces,
            HashSet::from([WorkspaceId::from("ws-1")])
        );
        assert_eq!(
            summary.window_counts.get(&WorkspaceId::from("ws-1")),
            Some(&2)
        );
    }

    #[test]
    fn recompute_full_scene_includes_all_workspaces() {
        let mut layout = LayoutState::default();
        let summary = layout
            .recompute(
                &wm_fixture(),
                &outputs_fixture(),
                &config_runtime_fixture(),
                Some(9),
                &LayoutRecomputePlan {
                    workspace_roots: HashSet::new(),
                    full_scene: true,
                },
            )
            .unwrap();

        assert!(summary.full_scene);
        assert!(summary
            .recomputed_workspaces
            .contains(&WorkspaceId::from("ws-1")));
        assert!(summary
            .recomputed_workspaces
            .contains(&WorkspaceId::from("ws-2")));
    }

    #[test]
    fn recompute_populates_tiled_window_rects() {
        let mut layout = LayoutState::default();

        layout.recompute(
            &wm_fixture(),
            &outputs_fixture(),
            &config_runtime_fixture(),
            Some(10),
            &LayoutRecomputePlan {
                workspace_roots: HashSet::from([WorkspaceId::from("ws-1")]),
                full_scene: false,
            },
        );

        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .w,
            600
        );
        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w2"))
                .unwrap()
                .size
                .w,
            600
        );
        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .h,
            700
        );
        assert_eq!(
            layout.last_summary.as_ref().unwrap().transaction_id,
            Some(10)
        );
    }

    #[test]
    fn commit_desired_promotes_geometry_to_committed_snapshot() {
        let mut layout = LayoutState::default();

        layout.recompute(
            &wm_fixture(),
            &outputs_fixture(),
            &config_runtime_fixture(),
            Some(14),
            &LayoutRecomputePlan {
                workspace_roots: HashSet::from([WorkspaceId::from("ws-1")]),
                full_scene: false,
            },
        );
        assert!(layout.committed_tiled_rect(&WindowId::from("w1")).is_none());

        layout.commit_desired();

        assert_eq!(
            layout
                .committed_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .w,
            600
        );
    }

    #[test]
    fn recompute_uses_workspace_selected_layout_stylesheet() {
        let mut layout = LayoutState::default();
        let mut wm = wm_fixture();
        wm.workspaces
            .get_mut(&WorkspaceId::from("ws-1"))
            .unwrap()
            .name = "alpha".into();

        layout.recompute(
            &wm,
            &outputs_fixture(),
            &config_runtime_fixture(),
            Some(11),
            &LayoutRecomputePlan {
                workspace_roots: HashSet::from([WorkspaceId::from("ws-1")]),
                full_scene: false,
            },
        );

        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .w,
            1200
        );
        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .h,
            350
        );
        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w2"))
                .unwrap()
                .size
                .h,
            350
        );
    }

    #[test]
    fn recompute_prefers_per_monitor_layout_over_workspace_selection() {
        let mut layout = LayoutState::default();
        let mut wm = wm_fixture();
        wm.workspaces
            .get_mut(&WorkspaceId::from("ws-1"))
            .unwrap()
            .name = "alpha".into();

        let mut config_runtime = config_runtime_fixture();
        let mut config = config_runtime.current().clone();
        config
            .layout_selection
            .per_monitor
            .insert("out-1".into(), "columns".into());
        config_runtime.replace(config, ConfigSource::PreparedConfig);

        layout.recompute(
            &wm,
            &outputs_fixture(),
            &config_runtime,
            Some(12),
            &LayoutRecomputePlan {
                workspace_roots: HashSet::from([WorkspaceId::from("ws-1")]),
                full_scene: false,
            },
        );

        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .w,
            600
        );
        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .h,
            700
        );
    }

    #[test]
    fn recompute_uses_installed_layout_tree_when_available() {
        let mut layout = LayoutState::default();
        let mut wm = wm_fixture();
        wm.workspaces
            .get_mut(&WorkspaceId::from("ws-1"))
            .unwrap()
            .name = "alpha".into();

        let mut config_runtime = config_runtime_fixture();
        let mut config = config_runtime.current().clone();
        config.layouts[1].stylesheet = "workspace { display: flex; flex-direction: column; width: 100%; height: 100%; } group { width: 100%; } #top { height: 200px; } #bottom { flex-grow: 1; } window { width: 100%; height: 100%; }".into();
        config_runtime.replace(config, ConfigSource::PreparedConfig);
        config_runtime.install_layout_tree(
            "rows",
            SourceLayoutNode::Workspace {
                meta: Default::default(),
                children: vec![
                    SourceLayoutNode::Group {
                        meta: LayoutNodeMeta {
                            id: Some("top".into()),
                            ..Default::default()
                        },
                        children: vec![SourceLayoutNode::Slot {
                            meta: Default::default(),
                            window_match: None,
                            take: SlotTake::Count(1),
                        }],
                    },
                    SourceLayoutNode::Group {
                        meta: LayoutNodeMeta {
                            id: Some("bottom".into()),
                            ..Default::default()
                        },
                        children: vec![SourceLayoutNode::Slot {
                            meta: Default::default(),
                            window_match: None,
                            take: SlotTake::Remaining(RemainingTake::Remaining),
                        }],
                    },
                ],
            },
            crate::config::LayoutTreeSource::JsRuntime,
        );

        layout.recompute(
            &wm,
            &outputs_fixture(),
            &config_runtime,
            Some(13),
            &LayoutRecomputePlan {
                workspace_roots: HashSet::from([WorkspaceId::from("ws-1")]),
                full_scene: false,
            },
        );

        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w1"))
                .unwrap()
                .size
                .h,
            200
        );
        assert_eq!(
            layout
                .desired_tiled_rect(&WindowId::from("w2"))
                .unwrap()
                .size
                .h,
            500
        );
    }
}
