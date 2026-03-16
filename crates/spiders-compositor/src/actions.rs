use spiders_config::model::Config;
use spiders_shared::api::{CompositorEvent, LayoutCycleDirection, WmAction};
use spiders_shared::ids::WindowId;
use spiders_shared::runtime::AuthoringLayoutRuntime;

use crate::CompositorLayoutError;
use crate::runtime::{CompositorRuntimeState, WindowPlacement};
use crate::wm::{WmState, WmStateError};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ActionError {
    #[error(transparent)]
    WmState(#[from] WmStateError),
    #[error(transparent)]
    Layout(#[from] CompositorLayoutError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionOutcome {
    pub events: Vec<CompositorEvent>,
    pub recomputed_layout: bool,
}

impl ActionOutcome {
    fn new(events: Vec<CompositorEvent>, recomputed_layout: bool) -> Self {
        Self {
            events,
            recomputed_layout,
        }
    }
}

pub fn apply_action<R>(
    runtime: &mut CompositorRuntimeState<R>,
    wm_state: &mut WmState,
    action: &WmAction,
) -> Result<ActionOutcome, ActionError>
where
    R: AuthoringLayoutRuntime<Config = Config>,
{
    let mut recompute = false;

    let events = match action {
        WmAction::Spawn { .. } => Vec::new(),
        WmAction::ReloadConfig => {
            runtime.reload_config()?;
            return Ok(ActionOutcome::new(Vec::new(), true));
        }
        WmAction::SetLayout { name } => {
            let workspace_id = wm_state.current_workspace_id()?.clone();
            let event = wm_state.set_layout_for_workspace(&workspace_id, name)?;
            recompute = true;
            vec![event]
        }
        WmAction::CycleLayout { direction } => {
            let current_workspace = wm_state.current_workspace_id()?.clone();
            let next = cycle_layout_name(
                &runtime.startup.runtime.config,
                wm_state
                    .snapshot()
                    .workspace_by_id(&current_workspace)
                    .and_then(|workspace| workspace.effective_layout.as_ref())
                    .map(|layout| layout.name.as_str()),
                direction.unwrap_or(LayoutCycleDirection::Next),
            );
            let event = wm_state.set_layout_for_workspace(&current_workspace, &next)?;
            recompute = true;
            vec![event]
        }
        WmAction::ViewTag { tag } => {
            let output_id = wm_state.current_output_id()?.clone();
            recompute = true;
            wm_state.view_tag_on_output(&output_id, tag)?
        }
        WmAction::ToggleViewTag { tag } => {
            let events = wm_state.toggle_tag_on_current_output(tag)?;
            recompute = !events.is_empty();
            events
        }
        WmAction::ActivateWorkspace { workspace_id } => {
            recompute = true;
            wm_state.activate_workspace(workspace_id)?
        }
        WmAction::AssignWorkspace {
            workspace_id,
            output_id,
        } => {
            recompute = true;
            wm_state.assign_workspace_to_output(workspace_id, output_id)?
        }
        WmAction::FocusMonitorLeft => {
            let event = wm_state.focus_monitor_left()?;
            vec![event]
        }
        WmAction::FocusMonitorRight => {
            let event = wm_state.focus_monitor_right()?;
            vec![event]
        }
        WmAction::SendMonitorLeft => {
            let event = wm_state.send_monitor_left()?;
            recompute = true;
            vec![event]
        }
        WmAction::SendMonitorRight => {
            let event = wm_state.send_monitor_right()?;
            recompute = true;
            vec![event]
        }
        WmAction::ToggleFloating => {
            let event = wm_state.toggle_focused_floating()?;
            vec![event]
        }
        WmAction::ToggleFullscreen => {
            let event = wm_state.toggle_focused_fullscreen()?;
            vec![event]
        }
        WmAction::FocusWindow { window_id } => {
            let event = wm_state.focus_window(window_id)?;
            vec![event]
        }
        WmAction::SetFloatingWindowGeometry { window_id, rect } => {
            let event = wm_state.set_floating_window_geometry(window_id, *rect)?;
            vec![event]
        }
        WmAction::FocusDirection { direction } => {
            let event = wm_state.focus_direction(*direction)?;
            vec![event]
        }
        WmAction::SwapDirection { direction } => {
            wm_state.swap_direction(*direction)?;
            recompute = true;
            Vec::new()
        }
        WmAction::ResizeDirection { direction } => {
            let event = wm_state.resize_direction(*direction)?;
            recompute = true;
            vec![event]
        }
        WmAction::ResizeTiledDirection { direction } => {
            wm_state.resize_tiled_direction(*direction)?;
            recompute = true;
            Vec::new()
        }
        WmAction::MoveDirection { direction } => {
            let event = wm_state.move_direction(*direction)?;
            recompute = true;
            vec![event]
        }
        WmAction::TagFocusedWindow { tag } => {
            let event = wm_state.tag_focused_window(tag)?;
            recompute = true;
            vec![event]
        }
        WmAction::ToggleTagFocusedWindow { tag } => {
            let event = wm_state.toggle_tag_focused_window(tag)?;
            recompute = true;
            vec![event]
        }
        WmAction::CloseFocusedWindow => {
            let Some(focused) = wm_state.snapshot().focused_window_id.clone() else {
                return Ok(ActionOutcome::new(Vec::new(), false));
            };
            let next_focus =
                preferred_focus_after_close(runtime.current_window_placements(), &focused);
            recompute = true;
            let mut events = wm_state.destroy_window(&focused)?;
            if let Some(next_focus) = next_focus {
                if wm_state
                    .snapshot()
                    .windows
                    .iter()
                    .any(|window| window.id == next_focus)
                {
                    events.push(wm_state.focus_window(&next_focus)?);
                }
            }
            events
        }
    };

    runtime.update_from_wm_state(wm_state.snapshot().clone());

    if recompute {
        runtime.recompute_current_layout()?;
    } else {
        runtime.refresh_view_state()?;
    }

    Ok(ActionOutcome::new(events, recompute))
}

pub(crate) fn preferred_focus_after_close(
    placements: Vec<WindowPlacement>,
    closed_window_id: &WindowId,
) -> Option<WindowId> {
    let closed_rect = placements
        .iter()
        .find(|placement| &placement.window_id == closed_window_id)?
        .rect;

    placements
        .into_iter()
        .filter(|placement| placement.window_id != *closed_window_id)
        .map(|placement| {
            let candidate = placement.rect;
            let vertical_overlap = overlap_1d(
                closed_rect.y,
                closed_rect.y + closed_rect.height,
                candidate.y,
                candidate.y + candidate.height,
            );
            let horizontal_overlap = overlap_1d(
                closed_rect.x,
                closed_rect.x + closed_rect.width,
                candidate.x,
                candidate.x + candidate.width,
            );

            let rank = if horizontal_overlap > 0.0
                && candidate.y + candidate.height <= closed_rect.y
            {
                (
                    0u8,
                    (closed_rect.y - (candidate.y + candidate.height)).round() as i32,
                )
            } else if horizontal_overlap > 0.0 && candidate.y >= closed_rect.y + closed_rect.height
            {
                (
                    1u8,
                    (candidate.y - (closed_rect.y + closed_rect.height)).round() as i32,
                )
            } else if vertical_overlap > 0.0 && candidate.x + candidate.width <= closed_rect.x {
                (
                    2u8,
                    (closed_rect.x - (candidate.x + candidate.width)).round() as i32,
                )
            } else if vertical_overlap > 0.0 && candidate.x >= closed_rect.x + closed_rect.width {
                (
                    3u8,
                    (candidate.x - (closed_rect.x + closed_rect.width)).round() as i32,
                )
            } else {
                let dx = rect_center_x(candidate) - rect_center_x(closed_rect);
                let dy = rect_center_y(candidate) - rect_center_y(closed_rect);
                (4u8, (dx * dx + dy * dy).round() as i32)
            };

            (rank, placement.window_id)
        })
        .min_by(|(left_rank, left_id), (right_rank, right_id)| {
            left_rank
                .cmp(right_rank)
                .then_with(|| left_id.cmp(right_id))
        })
        .map(|(_, window_id)| window_id)
}

fn overlap_1d(a_start: f32, a_end: f32, b_start: f32, b_end: f32) -> f32 {
    (a_end.min(b_end) - a_start.max(b_start)).max(0.0)
}

fn rect_center_x(rect: spiders_shared::layout::LayoutRect) -> f32 {
    rect.x + rect.width / 2.0
}

fn rect_center_y(rect: spiders_shared::layout::LayoutRect) -> f32 {
    rect.y + rect.height / 2.0
}

fn cycle_layout_name(
    config: &Config,
    current: Option<&str>,
    direction: LayoutCycleDirection,
) -> String {
    if config.layouts.is_empty() {
        return current.unwrap_or_default().to_owned();
    }

    let current_index =
        current.and_then(|name| config.layouts.iter().position(|layout| layout.name == name));
    let next_index = match (current_index, direction) {
        (Some(index), LayoutCycleDirection::Next) => (index + 1) % config.layouts.len(),
        (Some(index), LayoutCycleDirection::Previous) => {
            (index + config.layouts.len() - 1) % config.layouts.len()
        }
        (None, _) => 0,
    };

    config.layouts[next_index].name.clone()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::authoring_layout::AuthoringLayoutService;
    use spiders_config::model::{Config, ConfigPaths, LayoutDefinition};
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::QuickJsPreparedLayoutRuntime;
    use spiders_shared::api::{CompositorEvent, FocusDirection, WmAction};
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;
    use crate::{CompositorRuntimeState, LayoutService, WmState};

    fn config() -> Config {
        Config {
            layouts: vec![
                LayoutDefinition {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                    effects_stylesheet: String::new(),
                    runtime_graph: None,
                },
                LayoutDefinition {
                    name: "columns".into(),
                    module: "layouts/columns.js".into(),
                    stylesheet: String::new(),
                    effects_stylesheet: String::new(),
                    runtime_graph: None,
                },
            ],
            ..Config::default()
        }
    }

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
            workspaces: vec![
                WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-1"),
                    name: "1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["1".into()],
                    focused: true,
                    visible: true,
                    effective_layout: Some(LayoutRef {
                        name: "master-stack".into(),
                    }),
                },
                WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-2"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["2".into()],
                    focused: false,
                    visible: false,
                    effective_layout: Some(LayoutRef {
                        name: "columns".into(),
                    }),
                },
            ],
            windows: vec![
                WindowSnapshot {
                    id: WindowId::from("w1"),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("firefox".into()),
                    title: Some("Firefox".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: false,
                    floating_rect: None,
                    fullscreen: false,
                    focused: true,
                    urgent: false,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-1")),
                    tags: vec!["1".into()],
                },
                WindowSnapshot {
                    id: WindowId::from("w2"),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("alacritty".into()),
                    title: Some("Terminal".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: false,
                    floating_rect: None,
                    fullscreen: false,
                    focused: false,
                    urgent: false,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-2")),
                    tags: vec!["2".into()],
                },
            ],
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into(), "2".into()],
        }
    }

    fn runtime_state()
    -> CompositorRuntimeState<QuickJsPreparedLayoutRuntime<RuntimeProjectLayoutSourceLoader>> {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-action-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();
        fs::write(
            runtime_root.join("layouts/columns.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        LayoutService
            .initialize_runtime_state(service, config(), state())
            .unwrap()
    }

    fn runtime_state_with_reloadable_cache()
    -> CompositorRuntimeState<QuickJsPreparedLayoutRuntime<RuntimeProjectLayoutSourceLoader>> {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let project_root = temp_dir.join(format!("spiders-action-reload-{unique}"));
        let cache_root = project_root.join("cache");
        let _ = fs::create_dir_all(project_root.join("layouts/master-stack"));
        fs::write(
            project_root.join("config.ts"),
            r#"
                export default {
                  layouts: { default: "master-stack" },
                };
            "#,
        )
        .unwrap();
        fs::write(
            project_root.join("layouts/master-stack/index.ts"),
            r#"
                export default function layout() {
                  return { type: "workspace", children: [{ type: "window", id: "main" }] };
                }
            "#,
        )
        .unwrap();

        let paths = ConfigPaths::new(project_root.join("config.ts"), cache_root.join("config.js"));
        let service = spiders_runtime_js::build_authoring_layout_service(&paths);
        let config = service.load_config(&paths).unwrap();
        let runtime = LayoutService
            .initialize_runtime_state(service, config, state())
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            project_root.join("layouts/master-stack/index.ts"),
            r#"
                export default function layout() {
                  return { type: "workspace", children: [] };
                }
            "#,
        )
        .unwrap();

        runtime
    }

    #[test]
    fn set_layout_action_updates_state_and_recomputes_layout() {
        let mut runtime = runtime_state();
        let mut wm_state = WmState::from_snapshot(state());

        let outcome = apply_action(
            &mut runtime,
            &mut wm_state,
            &WmAction::SetLayout {
                name: "columns".into(),
            },
        )
        .unwrap();

        assert!(outcome.recomputed_layout);
        assert!(
            outcome
                .events
                .iter()
                .any(|event| matches!(event, CompositorEvent::LayoutChange { .. }))
        );
        assert_eq!(
            wm_state
                .snapshot()
                .current_workspace()
                .unwrap()
                .effective_layout
                .as_ref()
                .map(|layout| layout.name.as_str()),
            Some("columns")
        );
        assert_eq!(
            runtime
                .current_layout()
                .and_then(|layout| layout.request.layout_name.as_deref()),
            Some("columns")
        );
    }

    #[test]
    fn close_focused_window_action_emits_destroy_and_recomputes() {
        let mut runtime = runtime_state();
        let mut wm_state = WmState::from_snapshot(state());

        let outcome =
            apply_action(&mut runtime, &mut wm_state, &WmAction::CloseFocusedWindow).unwrap();

        assert!(outcome.recomputed_layout);
        assert!(outcome.events.iter().any(|event| matches!(
            event,
            CompositorEvent::WindowDestroyed { window_id } if window_id == &WindowId::from("w1")
        )));
        assert!(
            wm_state
                .snapshot()
                .windows
                .iter()
                .all(|window| window.id != WindowId::from("w1"))
        );
    }

    #[test]
    fn close_focus_prefers_stacked_window_above_before_master_column() {
        let next = preferred_focus_after_close(
            vec![
                crate::runtime::WindowPlacement {
                    window_id: WindowId::from("w1"),
                    mode: crate::runtime::WindowPlacementMode::Tiled,
                    rect: spiders_shared::layout::LayoutRect {
                        x: 4.0,
                        y: 4.0,
                        width: 747.0,
                        height: 1539.0,
                    },
                },
                crate::runtime::WindowPlacement {
                    window_id: WindowId::from("w2"),
                    mode: crate::runtime::WindowPlacementMode::Tiled,
                    rect: spiders_shared::layout::LayoutRect {
                        x: 755.0,
                        y: 4.0,
                        width: 498.0,
                        height: 767.0,
                    },
                },
                crate::runtime::WindowPlacement {
                    window_id: WindowId::from("w3"),
                    mode: crate::runtime::WindowPlacementMode::Tiled,
                    rect: spiders_shared::layout::LayoutRect {
                        x: 755.0,
                        y: 776.0,
                        width: 498.0,
                        height: 767.0,
                    },
                },
            ],
            &WindowId::from("w3"),
        );

        assert_eq!(next, Some(WindowId::from("w2")));
    }

    #[test]
    fn reload_config_action_rebuilds_runtime_cache_before_relayout() {
        let mut runtime = runtime_state_with_reloadable_cache();
        let mut wm_state = WmState::from_snapshot(state());

        let outcome = apply_action(&mut runtime, &mut wm_state, &WmAction::ReloadConfig).unwrap();

        assert!(outcome.recomputed_layout);
        assert_eq!(
            runtime
                .current_layout()
                .unwrap()
                .response
                .root
                .window_nodes()
                .len(),
            0
        );
    }

    #[test]
    fn toggle_floating_action_updates_focused_window_without_relayout() {
        let mut runtime = runtime_state();
        let mut wm_state = WmState::from_snapshot(state());

        let outcome = apply_action(&mut runtime, &mut wm_state, &WmAction::ToggleFloating).unwrap();

        assert!(!outcome.recomputed_layout);
        assert!(outcome.events.iter().any(|event| matches!(
            event,
            CompositorEvent::WindowFloatingChange { window_id, floating } if window_id == &WindowId::from("w1") && *floating
        )));
    }

    #[test]
    fn toggle_view_tag_switches_when_tag_is_not_currently_visible() {
        let mut runtime = runtime_state();
        let mut wm_state = WmState::from_snapshot(state());

        let outcome = apply_action(
            &mut runtime,
            &mut wm_state,
            &WmAction::ToggleViewTag { tag: "2".into() },
        )
        .unwrap();

        assert!(outcome.recomputed_layout);
        assert_eq!(
            wm_state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert!(
            outcome
                .events
                .iter()
                .any(|event| matches!(event, CompositorEvent::TagChange { .. }))
        );
    }

    #[test]
    fn toggle_view_tag_is_noop_for_currently_visible_tag() {
        let mut runtime = runtime_state();
        let mut wm_state = WmState::from_snapshot(state());

        let outcome = apply_action(
            &mut runtime,
            &mut wm_state,
            &WmAction::ToggleViewTag { tag: "1".into() },
        )
        .unwrap();

        assert!(!outcome.recomputed_layout);
        assert!(outcome.events.is_empty());
        assert_eq!(
            wm_state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );
    }

    #[test]
    fn activate_workspace_action_updates_current_workspace() {
        let mut runtime = runtime_state();
        let mut wm_state = WmState::from_snapshot(state());

        let outcome = apply_action(
            &mut runtime,
            &mut wm_state,
            &WmAction::ActivateWorkspace {
                workspace_id: WorkspaceId::from("ws-2"),
            },
        )
        .unwrap();

        assert!(outcome.recomputed_layout);
        assert_eq!(
            wm_state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
    }

    #[test]
    fn assign_workspace_action_moves_workspace_to_output() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        let mut runtime = runtime_state();
        runtime.update_from_wm_state(snapshot.clone());
        let mut wm_state = WmState::from_snapshot(snapshot);

        let outcome = apply_action(
            &mut runtime,
            &mut wm_state,
            &WmAction::AssignWorkspace {
                workspace_id: WorkspaceId::from("ws-2"),
                output_id: OutputId::from("out-2"),
            },
        )
        .unwrap();

        assert!(outcome.recomputed_layout);
        assert_eq!(
            wm_state
                .snapshot()
                .workspace_by_id(&WorkspaceId::from("ws-2"))
                .unwrap()
                .output_id,
            Some(OutputId::from("out-2"))
        );
    }

    #[test]
    fn focus_direction_cycles_visible_windows() {
        let mut runtime = runtime_state();
        let mut state = state();
        state.windows.push(WindowSnapshot {
            id: WindowId::from("w3"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("thunar".into()),
            title: Some("Files".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            floating: false,
            floating_rect: None,
            fullscreen: false,
            focused: false,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            tags: vec!["1".into()],
        });
        state.visible_window_ids = vec![WindowId::from("w1"), WindowId::from("w3")];
        let mut wm_state = WmState::from_snapshot(state);

        let outcome = apply_action(
            &mut runtime,
            &mut wm_state,
            &WmAction::FocusDirection {
                direction: FocusDirection::Right,
            },
        )
        .unwrap();

        assert!(!outcome.recomputed_layout);
        assert!(outcome.events.iter().any(|event| matches!(
            event,
            CompositorEvent::FocusChange {
                focused_window_id: Some(window_id),
                ..
            } if window_id == &WindowId::from("w3")
        )));
        assert_eq!(
            wm_state.snapshot().focused_window_id,
            Some(WindowId::from("w3"))
        );
    }
}
