use spiders_shared::api::{FocusDirection, WmAction};
use spiders_tree::{OutputId, WindowId, WorkspaceId};
use spiders_tree::LayoutRect;

#[derive(Debug, Clone, PartialEq)]
pub enum RiverCommand {
    Spawn {
        command: String,
    },
    ReloadConfig,
    SetLayout {
        name: String,
    },
    CycleLayoutNext,
    CycleLayoutPrevious,
    ActivateWorkspace {
        workspace_id: WorkspaceId,
    },
    AssignFocusedWindowToWorkspace {
        workspace_id: WorkspaceId,
    },
    FocusOutput {
        output_id: OutputId,
    },
    FocusWindow {
        window_id: WindowId,
    },
    CloseFocusedWindow,
    ToggleFloating,
    ToggleFullscreen,
    FocusDirection {
        direction: FocusDirection,
    },
    MoveDirection {
        direction: FocusDirection,
    },
    SetFloatingWindowGeometry {
        window_id: WindowId,
        rect: LayoutRect,
    },
    Unsupported {
        action: &'static str,
    },
}

pub fn bridge_action(action: &WmAction) -> RiverCommand {
    match action {
        WmAction::Spawn { command } => RiverCommand::Spawn {
            command: command.clone(),
        },
        WmAction::ReloadConfig => RiverCommand::ReloadConfig,
        WmAction::SetLayout { name } => RiverCommand::SetLayout { name: name.clone() },
        WmAction::CycleLayout { direction } => match direction {
            Some(spiders_shared::api::LayoutCycleDirection::Previous) => {
                RiverCommand::CycleLayoutPrevious
            }
            None | Some(spiders_shared::api::LayoutCycleDirection::Next) => {
                RiverCommand::CycleLayoutNext
            }
        },
        WmAction::ActivateWorkspace { workspace_id } => RiverCommand::ActivateWorkspace {
            workspace_id: workspace_id.clone(),
        },
        WmAction::FocusWindow { window_id } => RiverCommand::FocusWindow {
            window_id: window_id.clone(),
        },
        WmAction::CloseFocusedWindow => RiverCommand::CloseFocusedWindow,
        WmAction::ToggleFloating => RiverCommand::ToggleFloating,
        WmAction::ToggleFullscreen => RiverCommand::ToggleFullscreen,
        WmAction::FocusDirection { direction } => RiverCommand::FocusDirection {
            direction: *direction,
        },
        WmAction::SetFloatingWindowGeometry { window_id, rect } => {
            RiverCommand::SetFloatingWindowGeometry {
                window_id: window_id.clone(),
                rect: *rect,
            }
        }
        WmAction::ViewWorkspace { workspace } => RiverCommand::ActivateWorkspace {
            workspace_id: workspace.to_string().into(),
        },
        WmAction::ToggleViewWorkspace { workspace } => RiverCommand::ActivateWorkspace {
            workspace_id: workspace.to_string().into(),
        },
        WmAction::AssignWorkspace { output_id, .. } => RiverCommand::FocusOutput {
            output_id: output_id.clone(),
        },
        WmAction::FocusMonitorLeft => RiverCommand::Unsupported {
            action: "focus-monitor-left",
        },
        WmAction::FocusMonitorRight => RiverCommand::Unsupported {
            action: "focus-monitor-right",
        },
        WmAction::SendMonitorLeft => RiverCommand::Unsupported {
            action: "send-monitor-left",
        },
        WmAction::SendMonitorRight => RiverCommand::Unsupported {
            action: "send-monitor-right",
        },
        WmAction::AssignFocusedWindowToWorkspace { workspace } => {
            RiverCommand::AssignFocusedWindowToWorkspace {
                workspace_id: workspace.to_string().into(),
            }
        }
        WmAction::ToggleAssignFocusedWindowToWorkspace { .. } => RiverCommand::Unsupported {
            action: "toggle-assign-focused-window-to-workspace",
        },
        WmAction::SwapDirection { direction } => RiverCommand::MoveDirection {
            direction: *direction,
        },
        WmAction::ResizeDirection { .. } => RiverCommand::Unsupported {
            action: "resize-direction",
        },
        WmAction::ResizeTiledDirection { .. } => RiverCommand::Unsupported {
            action: "resize-tiled-direction",
        },
        WmAction::MoveDirection { direction } => RiverCommand::MoveDirection {
            direction: *direction,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_action_bridges_directly() {
        assert_eq!(
            bridge_action(&WmAction::Spawn {
                command: "foot".into(),
            }),
            RiverCommand::Spawn {
                command: "foot".into(),
            }
        );
    }

    #[test]
    fn view_workspace_bridges_to_activate_workspace() {
        assert_eq!(
            bridge_action(&WmAction::ViewWorkspace { workspace: 3 }),
            RiverCommand::ActivateWorkspace {
                workspace_id: "3".into(),
            }
        );
    }

    #[test]
    fn swap_direction_bridges_to_move_direction() {
        assert_eq!(
            bridge_action(&WmAction::SwapDirection {
                direction: FocusDirection::Left,
            }),
            RiverCommand::MoveDirection {
                direction: FocusDirection::Left,
            }
        );
    }
}
