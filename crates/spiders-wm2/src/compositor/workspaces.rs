use smithay::utils::Serial;
use tracing::info;

use crate::runtime::{RuntimeCommand, RuntimeResult};
use crate::state::SpidersWm;

impl SpidersWm {
    pub fn ensure_and_select_workspace(&mut self, name: impl Into<String>, serial: Serial) {
        let window_order = self.managed_window_order();
        let workspace_id = match self
            .runtime()
            .execute(RuntimeCommand::EnsureWorkspace { name: name.into() })
        {
            RuntimeResult::Workspace(workspace_id) => workspace_id,
            _ => return,
        };

        let selection = match self
            .runtime()
            .execute(RuntimeCommand::RequestSelectWorkspace {
                workspace_id,
                window_order,
            }) {
            RuntimeResult::WorkspaceSelection(Some(selection)) => selection,
            _ => return,
        };
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn select_next_workspace(&mut self, serial: Serial) {
        let window_order = self.managed_window_order();
        let selection = match self
            .runtime()
            .execute(RuntimeCommand::RequestSelectNextWorkspace { window_order })
        {
            RuntimeResult::WorkspaceSelection(Some(selection)) => selection,
            _ => return,
        };
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn select_previous_workspace(&mut self, serial: Serial) {
        let window_order = self.managed_window_order();
        let selection = match self
            .runtime()
            .execute(RuntimeCommand::RequestSelectPreviousWorkspace { window_order })
        {
            RuntimeResult::WorkspaceSelection(Some(selection)) => selection,
            _ => return,
        };
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn assign_focused_window_to_workspace(&mut self, workspace: u8, serial: Serial) {
        let window_order = self.managed_window_order();
        let workspace_id = match self.runtime().execute(RuntimeCommand::EnsureWorkspace {
            name: workspace.to_string(),
        }) {
            RuntimeResult::Workspace(workspace_id) => workspace_id,
            _ => return,
        };
        let focused_window_id =
            match self
                .runtime()
                .execute(RuntimeCommand::AssignFocusedWindowToWorkspace {
                    workspace_id: workspace_id.clone(),
                    window_order,
                }) {
                RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
                _ => return,
            };

        info!(workspace = %workspace_id.0, "assigned focused window to workspace");
        self.schedule_relayout();
        if let Some(window_id) = focused_window_id.clone() {
            self.emit_window_workspace_change(window_id);
        }
        self.apply_modeled_focus(focused_window_id, serial);
    }

    pub fn toggle_assign_focused_window_to_workspace(&mut self, workspace: u8, serial: Serial) {
        let window_order = self.managed_window_order();
        let workspace_id = match self.runtime().execute(RuntimeCommand::EnsureWorkspace {
            name: workspace.to_string(),
        }) {
            RuntimeResult::Workspace(workspace_id) => workspace_id,
            _ => return,
        };
        let focused_window_id =
            match self
                .runtime()
                .execute(RuntimeCommand::ToggleAssignFocusedWindowToWorkspace {
                    workspace_id: workspace_id.clone(),
                    window_order,
                }) {
                RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
                _ => return,
            };

        info!(workspace = %workspace_id.0, "toggled focused window assignment to workspace");
        self.schedule_relayout();
        if let Some(window_id) = focused_window_id.clone() {
            self.emit_window_workspace_change(window_id);
        }
        self.apply_modeled_focus(focused_window_id, serial);
    }

    fn managed_window_order(&self) -> Vec<crate::model::WindowId> {
        self.managed_windows
            .iter()
            .map(|record| record.id.clone())
            .collect()
    }
}
