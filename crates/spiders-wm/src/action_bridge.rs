use spiders_shared::command::{FocusDirection, LayoutCycleDirection, WmCommand};
use spiders_tree::LayoutRect;
use spiders_tree::{OutputId, WindowId, WorkspaceId};

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

pub fn bridge_action(action: &WmCommand) -> RiverCommand {
    match action {
        WmCommand::Spawn { command } => RiverCommand::Spawn {
            command: command.clone(),
        },
        WmCommand::ReloadConfig => RiverCommand::ReloadConfig,
        WmCommand::SetLayout { name } => RiverCommand::SetLayout { name: name.clone() },
        WmCommand::CycleLayout { direction } => match direction {
            Some(LayoutCycleDirection::Previous) => RiverCommand::CycleLayoutPrevious,
            None | Some(LayoutCycleDirection::Next) => RiverCommand::CycleLayoutNext,
        },
        WmCommand::ActivateWorkspace { workspace_id } => RiverCommand::ActivateWorkspace {
            workspace_id: workspace_id.clone(),
        },
        WmCommand::FocusWindow { window_id } => RiverCommand::FocusWindow {
            window_id: window_id.clone(),
        },
        WmCommand::CloseFocusedWindow => RiverCommand::CloseFocusedWindow,
        WmCommand::ToggleFloating => RiverCommand::ToggleFloating,
        WmCommand::ToggleFullscreen => RiverCommand::ToggleFullscreen,
        WmCommand::FocusDirection { direction } => RiverCommand::FocusDirection {
            direction: *direction,
        },
        WmCommand::SetFloatingWindowGeometry { window_id, rect } => {
            RiverCommand::SetFloatingWindowGeometry {
                window_id: window_id.clone(),
                rect: *rect,
            }
        }
        WmCommand::ViewWorkspace { workspace } => RiverCommand::ActivateWorkspace {
            workspace_id: workspace.to_string().into(),
        },
        WmCommand::ToggleViewWorkspace { workspace } => RiverCommand::ActivateWorkspace {
            workspace_id: workspace.to_string().into(),
        },
        WmCommand::AssignWorkspace { output_id, .. } => RiverCommand::FocusOutput {
            output_id: output_id.clone(),
        },
        WmCommand::FocusMonitorLeft => RiverCommand::Unsupported {
            action: "focus-monitor-left",
        },
        WmCommand::FocusMonitorRight => RiverCommand::Unsupported {
            action: "focus-monitor-right",
        },
        WmCommand::SendMonitorLeft => RiverCommand::Unsupported {
            action: "send-monitor-left",
        },
        WmCommand::SendMonitorRight => RiverCommand::Unsupported {
            action: "send-monitor-right",
        },
        WmCommand::AssignFocusedWindowToWorkspace { workspace } => {
            RiverCommand::AssignFocusedWindowToWorkspace {
                workspace_id: workspace.to_string().into(),
            }
        }
        WmCommand::ToggleAssignFocusedWindowToWorkspace { .. } => RiverCommand::Unsupported {
            action: "toggle-assign-focused-window-to-workspace",
        },
        WmCommand::SwapDirection { direction } => RiverCommand::MoveDirection {
            direction: *direction,
        },
        WmCommand::ResizeDirection { .. } => RiverCommand::Unsupported {
            action: "resize-direction",
        },
        WmCommand::ResizeTiledDirection { .. } => RiverCommand::Unsupported {
            action: "resize-tiled-direction",
        },
        WmCommand::MoveDirection { direction } => RiverCommand::MoveDirection {
            direction: *direction,
        },
        WmCommand::SpawnTerminal
        | WmCommand::FocusNextWindow
        | WmCommand::FocusPreviousWindow
        | WmCommand::SelectNextWorkspace
        | WmCommand::SelectPreviousWorkspace
        | WmCommand::SelectWorkspace { .. } => RiverCommand::Unsupported {
            action: "wm2-only-command",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_action_bridges_directly() {
        assert_eq!(
            bridge_action(&WmCommand::Spawn {
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
            bridge_action(&WmCommand::ViewWorkspace { workspace: 3 }),
            RiverCommand::ActivateWorkspace {
                workspace_id: "3".into(),
            }
        );
    }

    #[test]
    fn swap_direction_bridges_to_move_direction() {
        assert_eq!(
            bridge_action(&WmCommand::SwapDirection {
                direction: FocusDirection::Left,
            }),
            RiverCommand::MoveDirection {
                direction: FocusDirection::Left,
            }
        );
    }
}
