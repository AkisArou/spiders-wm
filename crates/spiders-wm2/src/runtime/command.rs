pub use spiders_shared::command::WmCommand;

use smithay::utils::{SERIAL_COUNTER, Serial};
use tracing::warn;

use crate::state::SpidersWm;

impl SpidersWm {
    #[allow(dead_code)]
    pub fn execute_wm_command(&mut self, command: WmCommand) {
        self.execute_wm_command_with_serial(command, SERIAL_COUNTER.next_serial());
    }

    pub fn execute_wm_command_with_serial(&mut self, command: WmCommand, serial: Serial) {
        match command {
            WmCommand::Spawn { command } => self.spawn_command(&command),
            WmCommand::ViewWorkspace { workspace } => {
                self.ensure_and_select_workspace(workspace.to_string(), serial)
            }
            WmCommand::ActivateWorkspace { workspace_id } => {
                self.ensure_and_select_workspace(workspace_id.0, serial)
            }
            WmCommand::AssignFocusedWindowToWorkspace { workspace } => {
                self.assign_focused_window_to_workspace(workspace, serial)
            }
            WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace } => {
                self.toggle_assign_focused_window_to_workspace(workspace, serial)
            }
            WmCommand::SpawnTerminal => self.spawn_foot(),
            WmCommand::FocusNextWindow => self.focus_next_window(serial),
            WmCommand::FocusPreviousWindow => self.focus_previous_window(serial),
            WmCommand::SelectNextWorkspace => self.select_next_workspace(serial),
            WmCommand::SelectPreviousWorkspace => self.select_previous_workspace(serial),
            WmCommand::SelectWorkspace { workspace_id } => {
                self.ensure_and_select_workspace(workspace_id.0, serial)
            }
            WmCommand::CloseFocusedWindow => self.close_focused_window(),
            WmCommand::ResizeDirection { direction }
            | WmCommand::ResizeTiledDirection { direction } => {
                warn!(?direction, "resize wm command is intentionally stubbed for now")
            }
            WmCommand::ReloadConfig => self.reload_config(),
            WmCommand::SetLayout { name } => {
                warn!(layout = %name, "set-layout wm command is not implemented yet")
            }
            WmCommand::CycleLayout { direction } => {
                warn!(?direction, "cycle-layout wm command is not implemented yet")
            }
            WmCommand::ToggleFullscreen => {
                self.toggle_focused_window_fullscreen()
            }
            WmCommand::ToggleFloating => {
                self.toggle_focused_window_floating()
            }
            WmCommand::FocusDirection { direction } => {
                self.focus_direction_window(direction, serial)
            }
            WmCommand::FocusWindow { window_id } => {
                self.focus_window_by_id(window_id, serial)
            }
            WmCommand::SwapDirection { direction } => {
                self.swap_focused_window_direction(direction)
            }
            WmCommand::MoveDirection { direction } => {
                warn!(?direction, "move-direction wm command is not implemented yet")
            }
            unsupported => {
                warn!(?unsupported, "ignoring unsupported wm command");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_selection_command_keeps_workspace_name() {
        assert_eq!(
            WmCommand::SelectWorkspace {
                workspace_id: "3".into(),
            },
            WmCommand::SelectWorkspace {
                workspace_id: "3".into(),
            }
        );
    }
}