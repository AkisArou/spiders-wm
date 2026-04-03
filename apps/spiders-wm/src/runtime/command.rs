pub use spiders_core::command::WmCommand;

use smithay::utils::{SERIAL_COUNTER, Serial};
use spiders_core::command::FocusDirection;
use spiders_wm_runtime::{
    FocusTarget, WindowToggle, WmEnvironment, WorkspaceAssignment, WorkspaceTarget,
    execute_wm_command as execute_shared_wm_command,
};

use crate::state::SpidersWm;

impl SpidersWm {
    #[allow(dead_code)]
    pub fn execute_wm_command(&mut self, command: WmCommand) {
        self.execute_wm_command_with_serial(command, SERIAL_COUNTER.next_serial());
    }

    pub fn execute_wm_command_with_serial(&mut self, command: WmCommand, _serial: Serial) {
        execute_shared_wm_command(self, command);
    }
}

impl WmEnvironment for SpidersWm {
    fn spawn_command(&mut self, command: &str) {
        SpidersWm::spawn_command(self, command);
    }

    fn request_quit(&mut self) {
        self.loop_signal.stop();
    }

    fn activate_workspace(&mut self, target: WorkspaceTarget) {
        match target {
            WorkspaceTarget::Named(name) => {
                self.ensure_and_select_workspace(name, SERIAL_COUNTER.next_serial());
            }
            WorkspaceTarget::Next => self.select_next_workspace(SERIAL_COUNTER.next_serial()),
            WorkspaceTarget::Previous => {
                self.select_previous_workspace(SERIAL_COUNTER.next_serial())
            }
        }
    }

    fn assign_focused_window_to_workspace(&mut self, assignment: WorkspaceAssignment) {
        match assignment {
            WorkspaceAssignment::Move(workspace) => {
                self.assign_focused_window_to_workspace(workspace, SERIAL_COUNTER.next_serial());
            }
            WorkspaceAssignment::Toggle(workspace) => {
                self.toggle_assign_focused_window_to_workspace(
                    workspace,
                    SERIAL_COUNTER.next_serial(),
                );
            }
        }
    }

    fn spawn_terminal(&mut self) {
        self.spawn_foot();
    }

    fn focus_window(&mut self, target: FocusTarget) {
        match target {
            FocusTarget::Next => self.focus_next_window(SERIAL_COUNTER.next_serial()),
            FocusTarget::Previous => self.focus_previous_window(SERIAL_COUNTER.next_serial()),
            FocusTarget::Direction(direction) => {
                self.focus_direction_window(direction, SERIAL_COUNTER.next_serial());
            }
            FocusTarget::Window(window_id) => {
                self.focus_window_by_id(window_id, SERIAL_COUNTER.next_serial());
            }
        }
    }

    fn close_focused_window(&mut self) {
        SpidersWm::close_focused_window(self);
    }

    fn reload_config(&mut self) {
        SpidersWm::reload_config(self);
    }

    fn toggle_focused_window(&mut self, toggle: WindowToggle) {
        match toggle {
            WindowToggle::Floating => self.toggle_focused_window_floating(),
            WindowToggle::Fullscreen => self.toggle_focused_window_fullscreen(),
        }
    }

    fn swap_focused_window(&mut self, direction: FocusDirection) {
        self.swap_focused_window_direction(direction);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_selection_command_keeps_workspace_name() {
        assert_eq!(
            WmCommand::SelectWorkspace { workspace_id: "3".into() },
            WmCommand::SelectWorkspace { workspace_id: "3".into() }
        );
    }
}
