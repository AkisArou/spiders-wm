use smithay::utils::Serial;
use tracing::info;

use crate::state::SpidersWm;

impl SpidersWm {
    pub fn ensure_and_select_workspace(&mut self, name: impl Into<String>, serial: Serial) {
        let window_order = self.managed_window_ids();
        let (selection, events) = {
            let mut runtime = self.runtime();
            let workspace_id = runtime.ensure_workspace(name.into());
            let selection = runtime.request_select_workspace(workspace_id, window_order);
            (selection, runtime.take_events())
        };

        let Some(selection) = selection else {
            return;
        };
        self.broadcast_runtime_events(events);
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn select_next_workspace(&mut self, serial: Serial) {
        let window_order = self.managed_window_ids();
        let (selection, events) = {
            let mut runtime = self.runtime();
            let selection = runtime.request_select_next_workspace(window_order);
            (selection, runtime.take_events())
        };

        let Some(selection) = selection else {
            return;
        };
        self.broadcast_runtime_events(events);
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn select_previous_workspace(&mut self, serial: Serial) {
        let window_order = self.managed_window_ids();
        let (selection, events) = {
            let mut runtime = self.runtime();
            let selection = runtime.request_select_previous_workspace(window_order);
            (selection, runtime.take_events())
        };

        let Some(selection) = selection else {
            return;
        };
        self.broadcast_runtime_events(events);
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn assign_focused_window_to_workspace(&mut self, workspace: u8, serial: Serial) {
        let window_order = self.managed_window_ids();
        let (workspace_id, focused_window_id, events) = {
            let mut runtime = self.runtime();
            let workspace_id = runtime.ensure_workspace(workspace.to_string());
            let focused_window_id = runtime
                .assign_focused_window_to_workspace(workspace_id.clone(), window_order)
                .focused_window_id;
            (workspace_id, focused_window_id, runtime.take_events())
        };

        info!(workspace = %workspace_id.0, "assigned focused window to workspace");
        self.schedule_relayout();
        self.broadcast_runtime_events(events);
        self.apply_modeled_focus(focused_window_id, serial);
    }

    pub fn toggle_assign_focused_window_to_workspace(&mut self, workspace: u8, serial: Serial) {
        let window_order = self.managed_window_ids();
        let (workspace_id, focused_window_id, events) = {
            let mut runtime = self.runtime();
            let workspace_id = runtime.ensure_workspace(workspace.to_string());
            let focused_window_id = runtime
                .toggle_assign_focused_window_to_workspace(workspace_id.clone(), window_order)
                .focused_window_id;
            (workspace_id, focused_window_id, runtime.take_events())
        };

        info!(workspace = %workspace_id.0, "toggled focused window assignment to workspace");
        self.schedule_relayout();
        self.broadcast_runtime_events(events);
        self.apply_modeled_focus(focused_window_id, serial);
    }
}
