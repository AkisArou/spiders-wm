use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::Config;
use spiders_scene::ast::ValidatedLayoutTree;
use spiders_scene::{LayoutSnapshotNode, SceneRequest};
use spiders_scene::pipeline::compute_layout_from_request;
use spiders_tree::{LayoutNodeMeta, RemainingTake, ResolvedLayoutNode, SlotTake, SourceLayoutNode, WindowId};
use spiders_shared::runtime::PreparedStylesheet;
use spiders_shared::wm::{LayoutRef, WindowSnapshot};
use spiders_runtime_js::DefaultLayoutRuntime;

use crate::model::WmState;

const FALLBACK_HORIZONTAL_STYLESHEET: &str = "workspace { display: flex; flex-direction: row; width: 100%; height: 100%; } group { display: flex; flex-direction: row; width: 100%; height: 100%; } window { flex-grow: 1; flex-basis: 0; height: 100%; }";

fn selected_layout_name(config: &Config, state: &spiders_shared::wm::StateSnapshot) -> Option<String> {
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

fn default_source_layout_tree() -> SourceLayoutNode {
    SourceLayoutNode::Workspace {
        meta: LayoutNodeMeta {
            class: vec!["river-workspace".into()],
            ..LayoutNodeMeta::default()
        },
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

fn visible_window_snapshots(state: &spiders_shared::wm::StateSnapshot, window_ids: &[WindowId]) -> Vec<WindowSnapshot> {
    window_ids
        .iter()
        .filter_map(|window_id| state.windows.iter().find(|window| &window.id == window_id).cloned())
        .collect()
}

fn resolved_layout_root(
    source_tree: SourceLayoutNode,
    state: &spiders_shared::wm::StateSnapshot,
    window_ids: &[WindowId],
) -> ResolvedLayoutNode {
    let windows = visible_window_snapshots(state, window_ids);

    ValidatedLayoutTree::new(source_tree)
        .ok()
        .and_then(|tree| tree.resolve(&windows).ok())
        .map(|resolved| resolved.root)
        .unwrap_or_else(|| ResolvedLayoutNode::Workspace {
            meta: LayoutNodeMeta {
                class: vec!["river-workspace".into()],
                ..LayoutNodeMeta::default()
            },
            children: window_ids
                .iter()
                .map(|window_id| ResolvedLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        class: vec!["river-window".into()],
                        ..LayoutNodeMeta::default()
                    },
                    window_id: Some(window_id.clone()),
                })
                .collect(),
        })
}

pub fn compute_layout_snapshot(
    layout_service: &mut AuthoringLayoutService<DefaultLayoutRuntime>,
    config: &Config,
    state: &WmState,
    window_ids: &[WindowId],
) -> Option<LayoutSnapshotNode> {
    let mut snapshot = state.as_state_snapshot();
    let selected_layout = selected_layout_name(config, &snapshot);

    if let Some(current_workspace) = snapshot
        .workspaces
        .iter_mut()
        .find(|workspace| Some(&workspace.id) == snapshot.current_workspace_id.as_ref())
    {
        current_workspace.effective_layout = selected_layout.map(|name| LayoutRef { name });
    }

    let evaluation = snapshot
        .current_workspace()
        .and_then(|workspace| {
            layout_service
                .evaluate_prepared_for_workspace(config, &snapshot, workspace)
                .ok()
                .flatten()
        });

    let source_tree = evaluation
        .as_ref()
        .map(|evaluation| evaluation.layout.clone())
        .unwrap_or_else(default_source_layout_tree);

    let mut request: SceneRequest = config
        .build_scene_request_from_state(
            &snapshot,
            resolved_layout_root(source_tree, &snapshot, window_ids),
            &evaluation.as_ref()?.artifact,
        )
        .ok()??;

    let layout_stylesheet_missing = request
        .stylesheets
        .layout
        .as_ref()
        .is_none_or(|sheet| sheet.source.trim().is_empty());
    if layout_stylesheet_missing {
        request.stylesheets.layout = Some(PreparedStylesheet {
            path: "fallback://river-layout.css".into(),
            source: FALLBACK_HORIZONTAL_STYLESHEET.into(),
        });
    }

    compute_layout_from_request(&request).ok().map(|response| response.root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_config::model::Config;
    use spiders_tree::{MatchClause, MatchKey, OutputId, WindowMatch};

    fn fixture_state() -> spiders_shared::wm::StateSnapshot {
        let config = Config {
            workspaces: vec!["1".into()],
            ..Config::default()
        };
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
                    meta: LayoutNodeMeta {
                        id: Some("top".into()),
                        ..LayoutNodeMeta::default()
                    },
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
                    meta: LayoutNodeMeta {
                        id: Some("bottom".into()),
                        ..LayoutNodeMeta::default()
                    },
                    children: vec![SourceLayoutNode::Slot {
                        meta: LayoutNodeMeta::default(),
                        window_match: None,
                        take: SlotTake::Remaining(RemainingTake::Remaining),
                    }],
                },
            ],
        };

        let resolved = resolved_layout_root(source_tree, &state, &["w1".into(), "w2".into(), "w3".into()]);

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
                meta: LayoutNodeMeta {
                    id: Some("matched".into()),
                    ..LayoutNodeMeta::default()
                },
                window_match: Some(WindowMatch {
                    clauses: vec![MatchClause {
                        key: MatchKey::Title,
                        value: "Terminal".into(),
                    }],
                }),
            }],
        };

        let resolved = resolved_layout_root(source_tree, &state, &["w1".into(), "w2".into(), "w3".into()]);

        let ResolvedLayoutNode::Workspace { children, .. } = resolved else {
            panic!("expected workspace root");
        };
        assert_eq!(children.len(), 1);
        assert!(matches!(
            &children[0],
            ResolvedLayoutNode::Window { window_id: Some(id), .. } if id == &WindowId::from("w2")
        ));
    }
}
