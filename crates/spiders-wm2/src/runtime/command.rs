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
            WmCommand::SpawnTerminal => self.spawn_foot(),
            WmCommand::FocusNextWindow => self.focus_next_window(serial),
            WmCommand::FocusPreviousWindow => self.focus_previous_window(serial),
            WmCommand::SelectNextWorkspace => self.select_next_workspace(serial),
            WmCommand::SelectPreviousWorkspace => self.select_previous_workspace(serial),
            WmCommand::SelectWorkspace { workspace_id } => {
                self.ensure_and_select_workspace(workspace_id.0, serial)
            }
            WmCommand::CloseFocusedWindow => self.close_focused_window(),
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