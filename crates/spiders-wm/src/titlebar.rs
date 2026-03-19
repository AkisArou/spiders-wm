use spiders_effects::TitlebarEffects;
use spiders_shared::ids::WindowId;
use spiders_shared::layout::LayoutRect;
use spiders_shared::wm::{StateSnapshot, WindowSnapshot};

use crate::runtime::{WindowPlacement, WorkspaceLayoutState};

const DEFAULT_TITLEBAR_HEIGHT_PX: f32 = 24.0;

#[derive(Debug, Clone, PartialEq)]
pub struct TitlebarRenderItem {
    pub window_id: WindowId,
    pub window_rect: LayoutRect,
    pub titlebar_rect: LayoutRect,
    pub title: String,
    pub app_id: Option<String>,
    pub focused: bool,
    pub style: TitlebarEffects,
}

pub fn compute_titlebar_render_plan(
    state: &StateSnapshot,
    layout: &WorkspaceLayoutState,
    placements: &[WindowPlacement],
) -> Vec<TitlebarRenderItem> {
    layout
        .response
        .root
        .window_nodes()
        .into_iter()
        .filter_map(|node| {
            let window_id = match node {
                spiders_shared::layout::LayoutSnapshotNode::Window {
                    window_id: Some(window_id),
                    ..
                } => window_id,
                _ => return None,
            };

            let policy = layout.effects.window_decoration_policy(window_id)?;
            if !policy.decorations_visible || !policy.titlebar_visible {
                return None;
            }

            let window = state
                .windows
                .iter()
                .find(|window| &window.id == window_id)?;
            let window_rect = placements
                .iter()
                .find(|placement| &placement.window_id == window_id)
                .map(|placement| placement.rect)
                .unwrap_or_else(|| node.rect());
            let titlebar_height = parse_titlebar_height_px(policy.titlebar_style.height.as_deref())
                .unwrap_or(DEFAULT_TITLEBAR_HEIGHT_PX)
                .min(window_rect.height.max(0.0));

            Some(TitlebarRenderItem {
                window_id: window_id.clone(),
                window_rect,
                titlebar_rect: LayoutRect {
                    x: window_rect.x,
                    y: window_rect.y,
                    width: window_rect.width,
                    height: titlebar_height,
                },
                title: title_for_window(window),
                app_id: window.app_id.clone(),
                focused: window.focused,
                style: policy.titlebar_style,
            })
        })
        .collect()
}

fn title_for_window(window: &WindowSnapshot) -> String {
    window
        .title
        .clone()
        .or_else(|| window.app_id.clone())
        .unwrap_or_else(|| "Window".into())
}

fn parse_titlebar_height_px(value: Option<&str>) -> Option<f32> {
    let value = value?.trim();
    let value = value.strip_suffix("px").unwrap_or(value).trim();
    let height = value.parse::<f32>().ok()?;
    (height.is_finite() && height > 0.0).then_some(height)
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::layout::{
        LayoutNodeMeta, LayoutRect, LayoutRequest, LayoutResponse, LayoutSnapshotNode, LayoutSpace,
        ResolvedLayoutNode,
    };
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use crate::effects::EffectsRuntimeState;
    use crate::runtime::{WorkspaceLayoutState, compute_window_placements};

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 800,
                logical_height: 600,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_workspaces: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(LayoutRef {
                    name: "main".into(),
                }),
            }],
            windows: vec![WindowSnapshot {
                id: WindowId::from("w1"),
                shell: spiders_shared::wm::ShellKind::XdgToplevel,
                app_id: Some("foot".into()),
                title: Some("Terminal".into()),
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                mode: spiders_shared::wm::WindowMode::Tiled,
                focused: true,
                urgent: false,
                output_id: Some(OutputId::from("out-1")),
                workspace_id: Some(WorkspaceId::from("ws-1")),
                workspaces: vec!["1".into()],
            }],
            visible_window_ids: vec![WindowId::from("w1")],
            workspace_names: vec!["1".into()],
        }
    }

    fn layout_with_effects(effects_stylesheet: &str) -> WorkspaceLayoutState {
        let mut effects = EffectsRuntimeState::from_stylesheet(effects_stylesheet).unwrap();
        let state = state();
        let workspace = &state.workspaces[0];
        effects.recompute_for_workspace(&state, workspace);

        WorkspaceLayoutState {
            workspace_id: WorkspaceId::from("ws-1"),
            request: LayoutRequest {
                workspace_id: WorkspaceId::from("ws-1"),
                output_id: Some(OutputId::from("out-1")),
                layout_name: Some("main".into()),
                root: ResolvedLayoutNode::Workspace {
                    meta: LayoutNodeMeta::default(),
                    children: vec![ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta::default(),
                        window_id: Some(WindowId::from("w1")),
                    }],
                },
                stylesheet: String::new(),
                effects_stylesheet: effects_stylesheet.into(),
                space: LayoutSpace {
                    width: 800.0,
                    height: 600.0,
                },
            },
            response: LayoutResponse {
                root: LayoutSnapshotNode::Workspace {
                    meta: LayoutNodeMeta::default(),
                    rect: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 800.0,
                        height: 600.0,
                    },
                    children: vec![LayoutSnapshotNode::Window {
                        meta: LayoutNodeMeta::default(),
                        rect: LayoutRect {
                            x: 10.0,
                            y: 20.0,
                            width: 400.0,
                            height: 300.0,
                        },
                        window_id: Some(WindowId::from("w1")),
                    }],
                },
            },
            effects,
        }
    }

    #[test]
    fn computes_titlebar_render_plan_from_layout_and_effects() {
        let state = state();
        let layout = layout_with_effects("window::titlebar { background: #222; height: 28px; }");

        let placements = compute_window_placements(&state, &layout);
        let plan = compute_titlebar_render_plan(&state, &layout, &placements);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].window_id, WindowId::from("w1"));
        assert_eq!(plan[0].title, "Terminal");
        assert_eq!(plan[0].app_id.as_deref(), Some("foot"));
        assert!(plan[0].focused);
        assert_eq!(plan[0].titlebar_rect.x, 10.0);
        assert_eq!(plan[0].titlebar_rect.y, 20.0);
        assert_eq!(plan[0].titlebar_rect.width, 400.0);
        assert_eq!(plan[0].titlebar_rect.height, 28.0);
        assert_eq!(plan[0].style.background.as_deref(), Some("#222"));
    }

    #[test]
    fn skips_titlebar_render_plan_when_decorations_are_hidden() {
        let state = state();
        let layout = layout_with_effects(
            "window { appearance: none; } window::titlebar { background: #222; }",
        );

        let placements = compute_window_placements(&state, &layout);
        assert!(compute_titlebar_render_plan(&state, &layout, &placements).is_empty());
    }

    #[test]
    fn uses_default_titlebar_height_when_style_height_is_missing() {
        let state = state();
        let layout = layout_with_effects("window::titlebar { background: #222; }");

        let placements = compute_window_placements(&state, &layout);
        let plan = compute_titlebar_render_plan(&state, &layout, &placements);
        assert_eq!(plan[0].titlebar_rect.height, DEFAULT_TITLEBAR_HEIGHT_PX);
    }

    #[test]
    fn uses_window_fallback_title_when_title_is_missing() {
        let mut state = state();
        state.windows[0].title = None;
        let layout = layout_with_effects("window::titlebar { background: #222; }");

        let placements = compute_window_placements(&state, &layout);
        let plan = compute_titlebar_render_plan(&state, &layout, &placements);
        assert_eq!(plan[0].title, "foot");
    }
}
