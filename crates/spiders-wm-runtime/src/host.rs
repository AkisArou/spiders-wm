use spiders_core::WindowId;
use spiders_core::command::{FocusDirection, WmCommand};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Named(String),
    Next,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceAssignment {
    Move(u8),
    Toggle(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusTarget {
    Next,
    Previous,
    Direction(FocusDirection),
    Window(WindowId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowToggle {
    Floating,
    Fullscreen,
}

pub trait WmEnvironment {
    fn spawn_command(&mut self, command: &str);
    fn request_quit(&mut self);
    fn activate_workspace(&mut self, target: WorkspaceTarget);
    fn assign_focused_window_to_workspace(&mut self, assignment: WorkspaceAssignment);
    fn spawn_terminal(&mut self);
    fn focus_window(&mut self, target: FocusTarget);
    fn close_focused_window(&mut self);
    fn reload_config(&mut self);
    fn toggle_focused_window(&mut self, toggle: WindowToggle);
    fn swap_focused_window(&mut self, direction: FocusDirection);
}

pub fn execute_wm_command<E: WmEnvironment>(environment: &mut E, command: WmCommand) {
    match command {
        WmCommand::Spawn { command } => environment.spawn_command(&command),
        WmCommand::Quit => environment.request_quit(),
        WmCommand::ViewWorkspace { workspace } => {
            environment.activate_workspace(WorkspaceTarget::Named(workspace.to_string()))
        }
        WmCommand::ActivateWorkspace { workspace_id } => {
            environment.activate_workspace(WorkspaceTarget::Named(workspace_id.0))
        }
        WmCommand::AssignFocusedWindowToWorkspace { workspace } => {
            environment.assign_focused_window_to_workspace(WorkspaceAssignment::Move(workspace))
        }
        WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace } => {
            environment.assign_focused_window_to_workspace(WorkspaceAssignment::Toggle(workspace))
        }
        WmCommand::SpawnTerminal => environment.spawn_terminal(),
        WmCommand::FocusNextWindow => environment.focus_window(FocusTarget::Next),
        WmCommand::FocusPreviousWindow => environment.focus_window(FocusTarget::Previous),
        WmCommand::SelectNextWorkspace => environment.activate_workspace(WorkspaceTarget::Next),
        WmCommand::SelectPreviousWorkspace => {
            environment.activate_workspace(WorkspaceTarget::Previous)
        }
        WmCommand::SelectWorkspace { workspace_id } => {
            environment.activate_workspace(WorkspaceTarget::Named(workspace_id.0))
        }
        WmCommand::CloseFocusedWindow => environment.close_focused_window(),
        WmCommand::ResizeDirection { direction } => {
            warn!(?direction, "resize wm command is intentionally stubbed for now")
        }
        WmCommand::ResizeTiledDirection { direction } => {
            warn!(?direction, "resize-tiled wm command is intentionally stubbed for now")
        }
        WmCommand::ReloadConfig => environment.reload_config(),
        WmCommand::SetLayout { name } => {
            warn!(layout = %name, "set-layout wm command is not implemented yet")
        }
        WmCommand::CycleLayout { direction } => {
            warn!(?direction, "cycle-layout wm command is not implemented yet")
        }
        WmCommand::ToggleFullscreen => environment.toggle_focused_window(WindowToggle::Fullscreen),
        WmCommand::ToggleFloating => environment.toggle_focused_window(WindowToggle::Floating),
        WmCommand::FocusDirection { direction } => {
            environment.focus_window(FocusTarget::Direction(direction))
        }
        WmCommand::FocusWindow { window_id } => {
            environment.focus_window(FocusTarget::Window(window_id))
        }
        WmCommand::SwapDirection { direction } => environment.swap_focused_window(direction),
        WmCommand::MoveDirection { direction } => {
            warn!(?direction, "move-direction wm command is not implemented yet")
        }
        unsupported => {
            warn!(?unsupported, "ignoring unsupported wm command");
        }
    }
}
