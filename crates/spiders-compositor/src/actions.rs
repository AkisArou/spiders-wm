use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_shared::api::{CompositorEvent, LayoutCycleDirection, WmAction};

use crate::runtime::CompositorRuntimeState;
use crate::wm::{WmState, WmStateError};
use crate::CompositorLayoutError;

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

pub fn apply_action<L, R>(
    runtime: &mut CompositorRuntimeState<L, R>,
    wm_state: &mut WmState,
    action: &WmAction,
) -> Result<ActionOutcome, ActionError>
where
    L: spiders_config::loader::LayoutSourceLoader,
    R: LayoutRuntime,
{
    let mut recompute = false;

    let events = match action {
        WmAction::Spawn { .. } | WmAction::ReloadConfig => Vec::new(),
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
        WmAction::ToggleFloating => {
            let event = wm_state.toggle_focused_floating()?;
            vec![event]
        }
        WmAction::ToggleFullscreen => {
            let event = wm_state.toggle_focused_fullscreen()?;
            vec![event]
        }
        WmAction::FocusDirection { direction } => {
            let event = wm_state.focus_direction(*direction)?;
            vec![event]
        }
        WmAction::CloseFocusedWindow => {
            let focused = wm_state.focused_window_id()?.clone();
            recompute = true;
            wm_state.destroy_window(&focused)?
        }
    };

    runtime.update_from_wm_state(wm_state.snapshot().clone());

    if recompute {
        runtime.recompute_current_layout()?;
    }

    Ok(ActionOutcome::new(events, recompute))
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

    use spiders_config::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_config::runtime::BoaLayoutRuntime;
    use spiders_config::service::ConfigRuntimeService;
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
                },
                LayoutDefinition {
                    name: "columns".into(),
                    module: "layouts/columns.js".into(),
                    stylesheet: String::new(),
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

    fn runtime_state() -> CompositorRuntimeState<
        RuntimeProjectLayoutSourceLoader,
        BoaLayoutRuntime<RuntimeProjectLayoutSourceLoader>,
    > {
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
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);

        LayoutService
            .initialize_runtime_state(service, config(), state())
            .unwrap()
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
        assert!(outcome
            .events
            .iter()
            .any(|event| matches!(event, CompositorEvent::LayoutChange { .. })));
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
        assert!(wm_state
            .snapshot()
            .windows
            .iter()
            .all(|window| window.id != WindowId::from("w1")));
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
        assert!(outcome
            .events
            .iter()
            .any(|event| matches!(event, CompositorEvent::TagChange { .. })));
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
