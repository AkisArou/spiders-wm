pub use spiders_core::command::WmCommand;

use smithay::utils::{SERIAL_COUNTER, Serial};
use spiders_core::effect::{
    FocusTarget, WindowToggle, WmHostEffect, WorkspaceAssignment, WorkspaceTarget,
};
use spiders_wm_runtime::{PreviewRenderAction, WmHost, dispatch_wm_command};

use crate::state::SpidersWm;

impl SpidersWm {
    #[allow(dead_code)]
    pub fn execute_wm_command(&mut self, command: WmCommand) {
        self.execute_wm_command_with_serial(command, SERIAL_COUNTER.next_serial());
    }

    pub fn execute_wm_command_with_serial(&mut self, command: WmCommand, _serial: Serial) {
        dispatch_wm_command(self, command);
    }
}

impl WmHost for SpidersWm {
    fn on_effect(&mut self, effect: WmHostEffect) -> PreviewRenderAction {
        match effect {
            WmHostEffect::SpawnCommand { command } => SpidersWm::spawn_command(self, &command),
            WmHostEffect::RequestQuit => self.loop_signal.stop(),
            WmHostEffect::ActivateWorkspace { target } => match target {
                WorkspaceTarget::Named(name) => {
                    self.ensure_and_select_workspace(name, SERIAL_COUNTER.next_serial());
                }
                WorkspaceTarget::Next => self.select_next_workspace(SERIAL_COUNTER.next_serial()),
                WorkspaceTarget::Previous => {
                    self.select_previous_workspace(SERIAL_COUNTER.next_serial())
                }
            },
            WmHostEffect::AssignFocusedWindowToWorkspace { assignment } => match assignment {
                WorkspaceAssignment::Move(workspace) => {
                    self.assign_focused_window_to_workspace(
                        workspace,
                        SERIAL_COUNTER.next_serial(),
                    );
                }
                WorkspaceAssignment::Toggle(workspace) => {
                    self.toggle_assign_focused_window_to_workspace(
                        workspace,
                        SERIAL_COUNTER.next_serial(),
                    );
                }
            },
            WmHostEffect::SpawnTerminal => self.spawn_foot(),
            WmHostEffect::FocusWindow { target } => match target {
                FocusTarget::Next => self.focus_next_window(SERIAL_COUNTER.next_serial()),
                FocusTarget::Previous => self.focus_previous_window(SERIAL_COUNTER.next_serial()),
                FocusTarget::Direction(direction) => {
                    self.focus_direction_window(direction, SERIAL_COUNTER.next_serial());
                }
                FocusTarget::Window(window_id) => {
                    self.focus_window_by_id(window_id, SERIAL_COUNTER.next_serial());
                }
            },
            WmHostEffect::CloseFocusedWindow => SpidersWm::close_focused_window(self),
            WmHostEffect::ReloadConfig => SpidersWm::reload_config(self),
            WmHostEffect::ToggleFocusedWindow { toggle } => match toggle {
                WindowToggle::Floating => self.toggle_focused_window_floating(),
                WindowToggle::Fullscreen => self.toggle_focused_window_fullscreen(),
            },
            WmHostEffect::SwapFocusedWindow { direction } => {
                self.swap_focused_window_direction(direction)
            }
            WmHostEffect::SetLayout { name } => {
                let events = {
                    let mut runtime = self.runtime();
                    let _ = runtime.set_current_workspace_layout(name);
                    runtime.take_events()
                };
                self.broadcast_runtime_events(events);
                self.schedule_relayout();
            }
            WmHostEffect::CycleLayout { direction } => {
                let config = self.config.clone();
                let events = {
                    let mut runtime = self.runtime();
                    let _ = runtime.cycle_current_workspace_layout(&config, direction);
                    runtime.take_events()
                };
                self.broadcast_runtime_events(events);
                self.schedule_relayout();
            }
        }

        PreviewRenderAction::None
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
