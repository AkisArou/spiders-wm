use smithay::desktop::PopupKind;
use smithay::utils::{Rectangle, Serial};
use smithay::wayland::shell::xdg::PopupSurface;
use spiders_shared::command::FocusDirection;
use tracing::info;

use crate::compositor::navigation::{
    managed_window_swap_positions, select_directional_focus_candidate,
};
use crate::model::WindowId;
use crate::runtime::{RuntimeCommand, RuntimeResult};
use crate::state::SpidersWm;

impl SpidersWm {
    pub fn ensure_and_select_workspace(&mut self, name: impl Into<String>, serial: Serial) {
        let window_order: Vec<_> = self
            .managed_windows
            .iter()
            .map(|record| record.id.clone())
            .collect();
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
        let window_order: Vec<_> = self
            .managed_windows
            .iter()
            .map(|record| record.id.clone())
            .collect();
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
        let window_order: Vec<_> = self
            .managed_windows
            .iter()
            .map(|record| record.id.clone())
            .collect();
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

    pub fn focus_next_window(&mut self, serial: Serial) {
        let next_focus_window_id =
            match self
                .runtime()
                .execute(RuntimeCommand::RequestFocusNextWindowSelection {
                    seat_id: "winit".into(),
                }) {
                RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
                _ => None,
            };

        self.apply_modeled_focus(next_focus_window_id, serial);
    }

    pub fn focus_previous_window(&mut self, serial: Serial) {
        let previous_focus_window_id =
            match self
                .runtime()
                .execute(RuntimeCommand::RequestFocusPreviousWindowSelection {
                    seat_id: "winit".into(),
                }) {
                RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
                _ => None,
            };

        self.apply_modeled_focus(previous_focus_window_id, serial);
    }

    pub fn focus_direction_window(&mut self, direction: FocusDirection, serial: Serial) {
        let current_focused_window_id = self
            .focused_surface
            .as_ref()
            .and_then(|surface| self.window_id_for_surface(surface));
        let candidates = self.visible_geometry_candidates();

        let next_focus_window_id = match select_directional_focus_candidate(
            &candidates,
            current_focused_window_id,
            direction,
        ) {
            Some(window_id) => Some(window_id),
            None => {
                match direction {
                    FocusDirection::Left | FocusDirection::Up => {
                        self.focus_previous_window(serial);
                    }
                    FocusDirection::Right | FocusDirection::Down => {
                        self.focus_next_window(serial);
                    }
                }
                return;
            }
        };

        let next_surface =
            next_focus_window_id.and_then(|window_id| self.surface_for_window_id(window_id));
        self.set_focus(next_surface, serial);
    }

    pub fn focus_window_by_id(&mut self, window_id: WindowId, serial: Serial) {
        let Some(target_surface) = self.surface_for_window_id(window_id.clone()) else {
            return;
        };

        let target_workspace_id = self
            .model
            .windows
            .get(&window_id)
            .and_then(|window| window.workspace_id.clone());

        if let Some(workspace_id) = target_workspace_id {
            if self.model.current_workspace_id.as_ref() != Some(&workspace_id) {
                let window_order: Vec<_> = self
                    .managed_windows
                    .iter()
                    .map(|record| record.id.clone())
                    .collect();
                let selection =
                    match self
                        .runtime()
                        .execute(RuntimeCommand::RequestSelectWorkspace {
                            workspace_id,
                            window_order,
                        }) {
                        RuntimeResult::WorkspaceSelection(Some(selection)) => selection,
                        _ => return,
                    };
                self.apply_workspace_selection(selection.focused_window_id, serial);
            }
        }

        self.set_focus(Some(target_surface), serial);
    }

    pub fn swap_focused_window_direction(&mut self, direction: FocusDirection) {
        let Some(current_focused_window_id) = self
            .focused_surface
            .as_ref()
            .and_then(|surface| self.window_id_for_surface(surface))
        else {
            return;
        };

        let candidates = self.visible_geometry_candidates();
        let Some(target_window_id) = select_directional_focus_candidate(
            &candidates,
            Some(current_focused_window_id.clone()),
            direction,
        ) else {
            return;
        };

        let window_order = self
            .managed_windows
            .iter()
            .map(|record| record.id.clone())
            .collect::<Vec<_>>();
        let Some((focused_index, target_index)) = managed_window_swap_positions(
            &window_order,
            current_focused_window_id.clone(),
            target_window_id.clone(),
        ) else {
            return;
        };

        self.managed_windows.swap(focused_index, target_index);
        self.schedule_relayout();
        info!(
            ?direction,
            ?current_focused_window_id,
            ?target_window_id,
            "swapped focused window with directional neighbor"
        );
    }

    pub fn close_focused_window(&mut self) {
        let closing_window_id = match self
            .runtime()
            .execute(RuntimeCommand::RequestCloseFocusedWindowSelection)
        {
            RuntimeResult::CloseSelection(selection) => selection.closing_window_id,
            _ => None,
        };
        info!(closing_window = ?closing_window_id, "wm2 close focused window request");
        let Some(focused_surface) =
            closing_window_id.and_then(|window_id| self.surface_for_window_id(window_id))
        else {
            return;
        };

        self.capture_close_snapshot(&focused_surface);

        if let Some(record) = self.managed_window_for_surface(&focused_surface) {
            if let Some(toplevel) = record.window.toplevel() {
                toplevel.send_close();
            }
        }
    }

    pub fn assign_focused_window_to_workspace(&mut self, workspace: u8, serial: Serial) {
        let workspace_id = match self.runtime().execute(RuntimeCommand::EnsureWorkspace {
            name: workspace.to_string(),
        }) {
            RuntimeResult::Workspace(workspace_id) => workspace_id,
            _ => return,
        };
        let window_order: Vec<_> = self
            .managed_windows
            .iter()
            .map(|record| record.id.clone())
            .collect();
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
        let workspace_id = match self.runtime().execute(RuntimeCommand::EnsureWorkspace {
            name: workspace.to_string(),
        }) {
            RuntimeResult::Workspace(workspace_id) => workspace_id,
            _ => return,
        };
        let window_order: Vec<_> = self
            .managed_windows
            .iter()
            .map(|record| record.id.clone())
            .collect();
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

    pub fn toggle_focused_window_floating(&mut self) {
        let toggled_window_id = match self
            .runtime()
            .execute(RuntimeCommand::ToggleFocusedWindowFloating)
        {
            RuntimeResult::Window(toggled_window_id) => toggled_window_id,
            _ => None,
        };
        if toggled_window_id.is_none() {
            return;
        }

        self.schedule_relayout();
        if let Some(window_id) = toggled_window_id {
            let floating = self
                .model
                .windows
                .get(&window_id)
                .is_some_and(|window| window.floating);
            self.emit_window_floating_change(window_id, floating);
        }
    }

    pub fn toggle_focused_window_fullscreen(&mut self) {
        let toggled_window_id = match self
            .runtime()
            .execute(RuntimeCommand::ToggleFocusedWindowFullscreen)
        {
            RuntimeResult::Window(toggled_window_id) => toggled_window_id,
            _ => None,
        };
        if toggled_window_id.is_none() {
            return;
        }

        self.schedule_relayout();
        if let Some(window_id) = toggled_window_id {
            let fullscreen = self
                .model
                .windows
                .get(&window_id)
                .is_some_and(|window| window.fullscreen);
            self.emit_window_fullscreen_change(window_id, fullscreen);
        }
    }

    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = smithay::desktop::find_popup_root_surface(&PopupKind::Xdg(popup.clone()))
        else {
            return;
        };
        let Some(window) = self.space.elements().find(|window| {
            window
                .toplevel()
                .is_some_and(|toplevel| toplevel.wl_surface() == &root)
        }) else {
            return;
        };

        let output = self
            .space
            .outputs()
            .next()
            .expect("output missing for popup");
        let output_geo = self
            .space
            .output_geometry(output)
            .expect("output geometry missing");
        let window_geo = self
            .space
            .element_geometry(window)
            .unwrap_or(Rectangle::new((0, 0).into(), (0, 0).into()));

        let mut target = output_geo;
        target.loc -= smithay::desktop::get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
