use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::Config;
use spiders_core::runtime::prepared_layout::PreparedStylesheet;
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::{
    LayoutNodeMeta, RemainingTake, ResolvedLayoutNode, SlotTake, SourceLayoutNode, WindowId,
    WorkspaceId,
};
use spiders_scene::ast::ValidatedLayoutTree;
use spiders_scene::pipeline::SceneCache;
use spiders_scene::{CompiledKeyframesRule, LayoutSnapshotNode, SceneRequest};
use tracing::{debug, warn};

use crate::model::WmState;

#[derive(Debug, Clone)]
pub struct ComputedLayoutSnapshot {
    pub root: LayoutSnapshotNode,
    pub keyframes: Vec<CompiledKeyframesRule>,
}

impl ComputedLayoutSnapshot {
    pub fn find_by_window_id(&self, window_id: &WindowId) -> Option<&LayoutSnapshotNode> {
        self.root.find_by_window_id(window_id)
    }
}

const FALLBACK_HORIZONTAL_STYLESHEET: &str = "workspace { display: flex; flex-direction: row; width: 100%; height: 100%; } group { display: flex; flex-direction: row; width: 100%; height: 100%; } window { flex-grow: 1; flex-basis: 0; height: 100%; }";

static FALLBACK_LAYOUT_WARN_KEYS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn log_fallback_layout_stylesheet(
    workspace_id: &WorkspaceId,
    workspace_name: &str,
    selected_layout: Option<&str>,
) {
    let selected_layout = selected_layout.unwrap_or("<none>");
    let key = format!("{}::{selected_layout}", workspace_id);
    let warned_once = FALLBACK_LAYOUT_WARN_KEYS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .ok()
        .is_some_and(|mut seen| !seen.insert(key));

    if warned_once {
        debug!(
            %workspace_id,
            workspace_name = %workspace_name,
            selected_layout,
            "layout stylesheet missing or empty; using fallback stylesheet (repeat)"
        );
    } else {
        warn!(
            %workspace_id,
            workspace_name = %workspace_name,
            selected_layout,
            "layout stylesheet missing or empty; applying fallback stylesheet"
        );
    }
}

fn default_source_layout_tree() -> SourceLayoutNode {
    SourceLayoutNode::Workspace {
        meta: LayoutNodeMeta { class: vec!["river-workspace".into()], ..LayoutNodeMeta::default() },
        children: vec![SourceLayoutNode::Group {
            meta: LayoutNodeMeta {
                id: Some("primary".into()),
                class: vec!["river-group".into()],
                ..LayoutNodeMeta::default()
            },
            children: vec![SourceLayoutNode::Slot {
                meta: LayoutNodeMeta {
                    class: vec!["river-slot".into()],
                    ..LayoutNodeMeta::default()
                },
                window_match: None,
                take: SlotTake::Remaining(RemainingTake::Remaining),
            }],
        }],
    }
}

fn resolved_workspace_meta(workspace_classes: &[&str]) -> LayoutNodeMeta {
    let mut class = vec!["river-workspace".into()];
    class.extend(workspace_classes.iter().map(|class_name| (*class_name).to_owned()));

    LayoutNodeMeta { class, ..LayoutNodeMeta::default() }
}

fn visible_window_snapshots(
    state: &spiders_core::snapshot::StateSnapshot,
    window_ids: &[WindowId],
) -> Vec<WindowSnapshot> {
    let windows: Vec<WindowSnapshot> = window_ids
        .iter()
        .filter_map(|window_id| {
            state.windows.iter().find(|window| &window.id == window_id).cloned()
        })
        .collect();

    if windows.len() != window_ids.len() {
        warn!(
            requested_window_count = window_ids.len(),
            found_window_count = windows.len(),
            "some window ids were missing from state snapshot during layout resolve"
        );
    }

    windows
}

fn resolved_layout_root(
    source_tree: SourceLayoutNode,
    state: &spiders_core::snapshot::StateSnapshot,
    window_ids: &[WindowId],
    workspace_classes: &[&str],
) -> ResolvedLayoutNode {
    let windows = visible_window_snapshots(state, window_ids);

    let resolved = match ValidatedLayoutTree::new(source_tree) {
        Ok(tree) => match tree.resolve(&windows) {
            Ok(resolved) => Some(resolved.root),
            Err(error) => {
                warn!(
                    %error,
                    window_count = windows.len(),
                    "layout tree resolve failed; falling back to flat workspace layout"
                );
                None
            }
        },
        Err(error) => {
            warn!(
                %error,
                window_count = windows.len(),
                "layout tree validation failed; falling back to flat workspace layout"
            );
            None
        }
    };

    let resolved = resolved.map(|mut root| {
        if let ResolvedLayoutNode::Workspace { meta, .. } = &mut root {
            meta.class.retain(|class_name| class_name != "river-workspace");
            meta.class.insert(0, "river-workspace".into());
            meta.class.extend(workspace_classes.iter().map(|class_name| (*class_name).to_owned()));
        }

        root
    });

    resolved.unwrap_or_else(|| ResolvedLayoutNode::Workspace {
        meta: resolved_workspace_meta(workspace_classes),
        children: window_ids
            .iter()
            .map(|window_id| ResolvedLayoutNode::Window {
                meta: LayoutNodeMeta {
                    class: vec!["river-window".into()],
                    ..LayoutNodeMeta::default()
                },
                window_id: Some(window_id.clone()),
                children: Vec::new(),
            })
            .collect(),
    })
}

pub fn compute_workspace_layout_snapshot(
    layout_service: &mut AuthoringLayoutService,
    scene_cache: &mut SceneCache,
    config: &Config,
    state: &WmState,
    workspace_id: &WorkspaceId,
    window_ids: &[WindowId],
    workspace_classes: &[&str],
) -> Option<ComputedLayoutSnapshot> {
    let mut snapshot = state.as_state_snapshot();
    snapshot.current_workspace_id = Some(workspace_id.clone());
    snapshot.current_output_id = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == *workspace_id)
        .and_then(|workspace| workspace.output_id.clone());
    snapshot.visible_window_ids = window_ids.to_vec();

    let current_workspace_id = snapshot.current_workspace_id.clone();
    let selected_layout = snapshot
        .current_workspace()
        .and_then(|workspace| workspace.effective_layout.as_ref())
        .map(|layout| layout.name.clone());

    let Some(workspace) = snapshot.current_workspace() else {
        warn!(
            ?current_workspace_id,
            "state snapshot has no current workspace; skipping layout snapshot computation"
        );
        return None;
    };

    let workspace_id = workspace.id.clone();
    let workspace_name = workspace.name.clone();

    let evaluation =
        match layout_service.evaluate_prepared_for_workspace(config, &snapshot, workspace) {
            Ok(evaluation) => evaluation,
            Err(error) => {
                warn!(
                    %error,
                    %workspace_id,
                    workspace_name = %workspace_name,
                    "failed to evaluate prepared layout for workspace"
                );
                None
            }
        };

    if evaluation.is_none() {
        warn!(
            %workspace_id,
            workspace_name = %workspace_name,
            selected_layout = selected_layout.as_deref().unwrap_or("<none>"),
            "no prepared layout evaluation available; using default source layout tree"
        );
    }

    let source_tree = evaluation
        .as_ref()
        .map(|evaluation| evaluation.layout.clone())
        .unwrap_or_else(default_source_layout_tree);

    let Some(evaluation) = evaluation.as_ref() else {
        return None;
    };

    let request_result = config.build_scene_request_from_state(
        &snapshot,
        resolved_layout_root(source_tree, &snapshot, window_ids, workspace_classes),
        &evaluation.artifact,
    );

    let mut request: SceneRequest = match request_result {
        Ok(Some(request)) => request,
        Ok(None) => {
            warn!(
                %workspace_id,
                workspace_name = %workspace_name,
                "scene request not produced for current workspace"
            );
            return None;
        }
        Err(error) => {
            warn!(
                %error,
                %workspace_id,
                workspace_name = %workspace_name,
                "failed building scene request from state"
            );
            return None;
        }
    };

    let layout_stylesheet_missing =
        request.stylesheets.layout.as_ref().is_none_or(|sheet| sheet.source.trim().is_empty());

    debug!(
        %workspace_id,
        workspace_name = %workspace_name,
        selected_layout = selected_layout.as_deref().unwrap_or("<none>"),
        stylesheet_path = request
            .stylesheets
            .layout
            .as_ref()
            .map(|sheet| sheet.path.as_str())
            .unwrap_or("<none>"),
        stylesheet_bytes = request
            .stylesheets
            .layout
            .as_ref()
            .map(|sheet| sheet.source.len())
            .unwrap_or(0),
        "layout stylesheet before fallback"
    );

    if layout_stylesheet_missing {
        log_fallback_layout_stylesheet(&workspace_id, &workspace_name, selected_layout.as_deref());
        request.stylesheets.layout = Some(PreparedStylesheet {
            path: "fallback://river-layout.css".into(),
            source: FALLBACK_HORIZONTAL_STYLESHEET.into(),
        });
    }

    let layout_name = request.layout_name.as_deref().unwrap_or("__default__").to_string();

    match scene_cache.compute_layout_from_request(&request) {
        Ok(response) => {
            debug!(
                %workspace_id,
                workspace_name = %workspace_name,
                window_count = window_ids.len(),
                "computed layout snapshot"
            );
            Some(ComputedLayoutSnapshot {
                root: response.root,
                keyframes: scene_cache.keyframes_for_layout(&layout_name),
            })
        }
        Err(error) => {
            warn!(
                %error,
                %workspace_id,
                workspace_name = %workspace_name,
                global_stylesheet_path = request
                    .stylesheets
                    .global
                    .as_ref()
                    .map(|sheet| sheet.path.as_str())
                    .unwrap_or("<none>"),
                layout_stylesheet_path = request
                    .stylesheets
                    .layout
                    .as_ref()
                    .map(|sheet| sheet.path.as_str())
                    .unwrap_or("<none>"),
                "scene cache failed computing layout from request"
            );

            if request.stylesheets.global.is_some() && request.stylesheets.layout.is_some() {
                warn!(
                    %workspace_id,
                    workspace_name = %workspace_name,
                    "retrying layout computation without global stylesheet"
                );

                let mut fallback_request = request.clone();
                fallback_request.stylesheets.global = None;
                return scene_cache
                    .compute_layout_from_request(&fallback_request)
                    .map(|response| {
                        debug!(
                            %workspace_id,
                            workspace_name = %workspace_name,
                            window_count = window_ids.len(),
                            "computed layout snapshot after dropping global stylesheet"
                        );
                        ComputedLayoutSnapshot {
                            root: response.root,
                            keyframes: scene_cache.keyframes_for_layout(&layout_name),
                        }
                    })
                    .map_err(|fallback_error| {
                        warn!(
                            %fallback_error,
                            %workspace_id,
                            workspace_name = %workspace_name,
                            "layout computation still failed after dropping global stylesheet"
                        );
                        fallback_error
                    })
                    .ok();
            }

            None
        }
    }
}

pub fn compute_layout_snapshot(
    layout_service: &mut AuthoringLayoutService,
    scene_cache: &mut SceneCache,
    config: &Config,
    state: &WmState,
    window_ids: &[WindowId],
) -> Option<ComputedLayoutSnapshot> {
    let workspace_id = state.current_workspace_id.as_ref()?;

    compute_workspace_layout_snapshot(
        layout_service,
        scene_cache,
        config,
        state,
        workspace_id,
        window_ids,
        &[],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_config::model::Config;
    use spiders_core::{MatchClause, MatchKey, OutputId, WindowMatch};

    fn fixture_state() -> spiders_core::snapshot::StateSnapshot {
        let config = Config { workspaces: vec!["1".into()], ..Config::default() };
        let mut state = WmState::from_config(&config);
        let output_id = OutputId::from("out-1");
        state.insert_output(output_id.clone(), "HDMI-A-1".into());
        state.focus_output(&output_id);
        state.set_output_dimensions(&output_id, 1200, 700);

        state.insert_window("w1".into());
        state.set_window_app_id(&"w1".into(), Some("firefox".into()));
        state.set_window_title(&"w1".into(), Some("Mozilla Firefox".into()));

        state.insert_window("w2".into());
        state.set_window_app_id(&"w2".into(), Some("foot".into()));
        state.set_window_title(&"w2".into(), Some("Terminal".into()));

        state.insert_window("w3".into());
        state.set_window_app_id(&"w3".into(), Some("firefox".into()));
        state.set_window_title(&"w3".into(), Some("Docs".into()));

        state.as_state_snapshot()
    }

    #[test]
    fn resolved_layout_root_honors_group_slots_and_take_counts() {
        let state = fixture_state();
        let source_tree = SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![
                SourceLayoutNode::Group {
                    meta: LayoutNodeMeta { id: Some("top".into()), ..LayoutNodeMeta::default() },
                    children: vec![SourceLayoutNode::Slot {
                        meta: LayoutNodeMeta::default(),
                        window_match: Some(WindowMatch {
                            clauses: vec![MatchClause {
                                key: MatchKey::AppId,
                                value: "firefox".into(),
                            }],
                        }),
                        take: SlotTake::Count(1),
                    }],
                },
                SourceLayoutNode::Group {
                    meta: LayoutNodeMeta { id: Some("bottom".into()), ..LayoutNodeMeta::default() },
                    children: vec![SourceLayoutNode::Slot {
                        meta: LayoutNodeMeta::default(),
                        window_match: None,
                        take: SlotTake::Remaining(RemainingTake::Remaining),
                    }],
                },
            ],
        };

        let resolved = resolved_layout_root(
            source_tree,
            &state,
            &["w1".into(), "w2".into(), "w3".into()],
            &[],
        );

        let ResolvedLayoutNode::Workspace { children, .. } = resolved else {
            panic!("expected workspace root");
        };
        assert_eq!(children.len(), 2);

        let ResolvedLayoutNode::Group { children: top_children, .. } = &children[0] else {
            panic!("expected top group");
        };
        let ResolvedLayoutNode::Group { children: bottom_children, .. } = &children[1] else {
            panic!("expected bottom group");
        };

        assert_eq!(top_children.len(), 1);
        assert!(matches!(
            &top_children[0],
            ResolvedLayoutNode::Window { window_id: Some(id), .. } if id == &WindowId::from("w1")
        ));

        assert_eq!(bottom_children.len(), 2);
        assert!(matches!(
            &bottom_children[0],
            ResolvedLayoutNode::Window { window_id: Some(id), .. } if id == &WindowId::from("w2")
        ));
        assert!(matches!(
            &bottom_children[1],
            ResolvedLayoutNode::Window { window_id: Some(id), .. } if id == &WindowId::from("w3")
        ));
    }

    #[test]
    fn resolved_layout_root_honors_window_match_nodes() {
        let state = fixture_state();
        let source_tree = SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![SourceLayoutNode::Window {
                meta: LayoutNodeMeta { id: Some("matched".into()), ..LayoutNodeMeta::default() },
                window_match: Some(WindowMatch {
                    clauses: vec![MatchClause { key: MatchKey::Title, value: "Terminal".into() }],
                }),
            }],
        };

        let resolved = resolved_layout_root(
            source_tree,
            &state,
            &["w1".into(), "w2".into(), "w3".into()],
            &[],
        );

        let ResolvedLayoutNode::Workspace { children, .. } = resolved else {
            panic!("expected workspace root");
        };
        assert_eq!(children.len(), 1);
        assert!(matches!(
            &children[0],
            ResolvedLayoutNode::Window { window_id: Some(id), .. } if id == &WindowId::from("w2")
        ));
    }

    #[test]
    fn resolved_layout_root_adds_workspace_transition_classes() {
        let state = fixture_state();
        let resolved = resolved_layout_root(
            default_source_layout_tree(),
            &state,
            &["w1".into()],
            &["enter-from-left"],
        );

        let ResolvedLayoutNode::Workspace { meta, .. } = resolved else {
            panic!("expected workspace root");
        };

        assert!(meta.class.iter().any(|class_name| class_name == "river-workspace"));
        assert!(meta.class.iter().any(|class_name| class_name == "enter-from-left"));
    }
}
