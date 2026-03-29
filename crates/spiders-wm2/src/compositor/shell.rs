use smithay::desktop::{PopupKind, Window};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Rectangle, SERIAL_COUNTER, Serial};
use smithay::wayland::shell::xdg::PopupSurface;
use spiders_shared::command::FocusDirection;
use tracing::info;

use crate::actions::focus::FocusUpdate;
use crate::frame_sync::{Transaction, WindowFrameSyncState};
use crate::model::{WindowId, window_id};
use crate::runtime::{RuntimeCommand, RuntimeResult};
use crate::state::{ManagedWindow, SpidersWm};

impl SpidersWm {
    pub fn ensure_and_select_workspace(&mut self, name: impl Into<String>, serial: Serial) {
        let window_order: Vec<_> = self.managed_windows.iter().map(|record| record.id.clone()).collect();
        let workspace_id = match self.runtime().execute(RuntimeCommand::EnsureWorkspace { name: name.into() }) {
            RuntimeResult::Workspace(workspace_id) => workspace_id,
            _ => return,
        };

        let selection = match self.runtime().execute(RuntimeCommand::RequestSelectWorkspace {
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
        let window_order: Vec<_> = self.managed_windows.iter().map(|record| record.id.clone()).collect();
        let selection = match self.runtime().execute(RuntimeCommand::RequestSelectNextWorkspace {
            window_order,
        }) {
            RuntimeResult::WorkspaceSelection(Some(selection)) => selection,
            _ => return,
        };
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn select_previous_workspace(&mut self, serial: Serial) {
        let window_order: Vec<_> = self.managed_windows.iter().map(|record| record.id.clone()).collect();
        let selection = match self.runtime().execute(RuntimeCommand::RequestSelectPreviousWorkspace {
            window_order,
        }) {
            RuntimeResult::WorkspaceSelection(Some(selection)) => selection,
            _ => return,
        };
        info!(workspace = %selection.workspace_id.0, "selected workspace");
        self.apply_workspace_selection(selection.focused_window_id, serial);
    }

    pub fn focus_next_window(&mut self, serial: Serial) {
        let next_focus_window_id = match self.runtime().execute(RuntimeCommand::RequestFocusNextWindowSelection {
            seat_id: "winit".into(),
        }) {
            RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
            _ => None,
        };
        let next_surface = next_focus_window_id.and_then(|window_id| self.surface_for_window_id(window_id));

        self.apply_backend_focus(next_surface.clone(), serial);
        self.apply_window_activation(next_surface.as_ref());
    }

    pub fn focus_previous_window(&mut self, serial: Serial) {
        let previous_focus_window_id = match self.runtime().execute(
            RuntimeCommand::RequestFocusPreviousWindowSelection {
                seat_id: "winit".into(),
            },
        ) {
            RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
            _ => None,
        };
        let previous_surface = previous_focus_window_id.and_then(|window_id| self.surface_for_window_id(window_id));

        self.apply_backend_focus(previous_surface.clone(), serial);
        self.apply_window_activation(previous_surface.as_ref());
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

        let next_surface = next_focus_window_id.and_then(|window_id| self.surface_for_window_id(window_id));
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
                let window_order: Vec<_> = self.managed_windows.iter().map(|record| record.id.clone()).collect();
                let selection = match self.runtime().execute(RuntimeCommand::RequestSelectWorkspace {
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

        let window_order = self.managed_windows.iter().map(|record| record.id.clone()).collect::<Vec<_>>();
        let Some((focused_index, target_index)) = managed_window_swap_positions(
            &window_order,
            current_focused_window_id.clone(),
            target_window_id.clone(),
        ) else {
            return;
        };

        self.managed_windows.swap(focused_index, target_index);
        self.schedule_relayout();
        info!(?direction, ?current_focused_window_id, ?target_window_id, "swapped focused window with directional neighbor");
    }

    fn apply_workspace_selection(&mut self, focused_window_id: Option<WindowId>, serial: Serial) {
        self.schedule_relayout();
        let focused_surface = focused_window_id.and_then(|window_id| self.surface_for_window_id(window_id));
        self.set_focus(focused_surface, serial);
    }

    pub fn close_focused_window(&mut self) {
        let closing_window_id = match self.runtime().execute(RuntimeCommand::RequestCloseFocusedWindowSelection) {
            RuntimeResult::CloseSelection(selection) => selection.closing_window_id,
            _ => None,
        };
        let Some(focused_surface) = closing_window_id.and_then(|window_id| self.surface_for_window_id(window_id)) else {
            return;
        };

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
        let window_order: Vec<_> = self.managed_windows.iter().map(|record| record.id.clone()).collect();
        let focused_window_id = match self.runtime().execute(
            RuntimeCommand::AssignFocusedWindowToWorkspace {
                workspace_id: workspace_id.clone(),
                window_order,
            },
        ) {
            RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
            _ => return,
        };

        info!(workspace = %workspace_id.0, "assigned focused window to workspace");
        self.schedule_relayout();
        if let Some(window_id) = focused_window_id.clone() {
            self.emit_window_workspace_change(window_id);
        }
        let focused_surface = focused_window_id.and_then(|window_id| self.surface_for_window_id(window_id));
        self.set_focus(focused_surface, serial);
    }

    pub fn toggle_assign_focused_window_to_workspace(&mut self, workspace: u8, serial: Serial) {
        let workspace_id = match self.runtime().execute(RuntimeCommand::EnsureWorkspace {
            name: workspace.to_string(),
        }) {
            RuntimeResult::Workspace(workspace_id) => workspace_id,
            _ => return,
        };
        let window_order: Vec<_> = self.managed_windows.iter().map(|record| record.id.clone()).collect();
        let focused_window_id = match self.runtime().execute(
            RuntimeCommand::ToggleAssignFocusedWindowToWorkspace {
                workspace_id: workspace_id.clone(),
                window_order,
            },
        ) {
            RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
            _ => return,
        };

        info!(workspace = %workspace_id.0, "toggled focused window assignment to workspace");
        self.schedule_relayout();
        if let Some(window_id) = focused_window_id.clone() {
            self.emit_window_workspace_change(window_id);
        }
        let focused_surface = focused_window_id.and_then(|window_id| self.surface_for_window_id(window_id));
        self.set_focus(focused_surface, serial);
    }

    pub fn toggle_focused_window_floating(&mut self) {
        let toggled_window_id = match self.runtime().execute(RuntimeCommand::ToggleFocusedWindowFloating) {
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
        let toggled_window_id = match self.runtime().execute(RuntimeCommand::ToggleFocusedWindowFullscreen) {
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

    pub fn set_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        let focused_window_id = self.resolve_focus_window_id(surface.as_ref());
        let focused_window_id = self.update_modeled_focus(focused_window_id);
        let focused_surface = focused_window_id.and_then(|window_id| self.surface_for_window_id(window_id));

        self.apply_backend_focus(focused_surface.clone(), serial);
        self.apply_window_activation(focused_surface.as_ref());
        self.emit_focus_change();
    }

    fn resolve_focus_window_id(&self, surface: Option<&WlSurface>) -> Option<WindowId> {
        surface.and_then(|surface| self.window_id_for_surface(surface))
    }

    fn update_modeled_focus(&mut self, focused_window_id: Option<WindowId>) -> Option<WindowId> {
        match self.runtime().execute(RuntimeCommand::RequestFocusWindowSelection {
            seat_id: "winit".into(),
            window_id: focused_window_id,
        }) {
            RuntimeResult::FocusSelection(selection) => selection.focused_window_id,
            _ => None,
        }
    }

    fn apply_backend_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        self.focused_surface = surface.clone();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, surface, serial);
        }
    }

    fn apply_window_activation(&self, focused_surface: Option<&WlSurface>) {
        for record in &self.managed_windows {
            let active = focused_surface.is_some_and(|focused| record.wl_surface() == *focused);
            record.window.set_activated(active);
            if let Some(toplevel) = record.window.toplevel() {
                let _ = toplevel.send_pending_configure();
            }
        }
    }

    pub fn add_window(&mut self, window: Window) {
        let window_id = window_id(self.next_window_id);
        self.next_window_id += 1;
        let _ = self.runtime().execute(RuntimeCommand::PlaceNewWindow {
            window_id: window_id.clone(),
        });

        self.managed_windows.push(ManagedWindow {
            id: window_id.clone(),
            window,
            mapped: false,
            frame_sync: WindowFrameSyncState::default(),
        });
    }

    pub fn handle_window_close(&mut self, surface: &WlSurface) {
        let Some(position) = self.managed_window_position_for_surface(surface) else {
            return;
        };

        let record = self.managed_windows.remove(position);
        let focus_update = match self.runtime().execute(RuntimeCommand::RemoveWindow {
            window_id: record.id,
        }) {
            RuntimeResult::FocusUpdate(focus_update) => focus_update,
            _ => FocusUpdate::Unchanged,
        };
        let transaction = Transaction::new();
        let monitor = transaction.monitor();

        if record.mapped {
            if let (Some(snapshot), Some(element_location)) = (
                record.frame_sync.snapshot_owned(),
                self.space.element_location(&record.window),
            ) {
                self.frame_sync.push_closing_window(snapshot.into_closing_window(
                    element_location,
                    record.window.geometry().loc,
                    monitor,
                ));
            }
            self.space.unmap_elem(&record.window);
        }

        if let FocusUpdate::Set(next_focus_window_id) = focus_update {
            let next_focus = next_focus_window_id.and_then(|window_id| self.surface_for_window_id(window_id));
            self.set_focus(next_focus, SERIAL_COUNTER.next_serial());
        }

        self.schedule_relayout_with_transaction(Some(transaction));
    }

    pub fn find_window_mut(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.managed_window_mut_for_surface(surface)
    }

    pub fn is_known_window_mapped(&self, surface: &WlSurface) -> bool {
        self.managed_window_for_surface(surface)
            .is_some_and(|record| record.mapped)
    }

    pub fn handle_window_commit(&mut self, surface: &WlSurface) {
        let mut mapped_window_id = None;
        let window_update = if let Some(record) = self.find_window_mut(surface) {
            let update = record.frame_sync.consume_commit_update(record.mapped);
            if !record.mapped && update.pending_location.is_some() {
                record.mapped = true;
                record.frame_sync.mark_snapshot_dirty();
                mapped_window_id = Some(record.id.clone());
            }

            Some((record.id.clone(), record.window.clone(), update.pending_location, update.first_map))
        } else {
            None
        };

        if let Some(window_id) = mapped_window_id {
            let _ = self.runtime().execute(RuntimeCommand::SyncWindowMapped {
                window_id,
                mapped: true,
            });
        }

        if let Some((window_id, window, pending_location, first_map)) = window_update {
            window.on_commit();

            if first_map {
                self.schedule_relayout();
                self.set_focus(Some(surface.clone()), SERIAL_COUNTER.next_serial());

                let mut mapped_window_id = None;
                if let Some(record) = self.find_window_mut(surface) {
                    let pending_location = record.frame_sync.take_pending_location();
                    if pending_location.is_some() {
                        record.mapped = true;
                        record.frame_sync.mark_snapshot_dirty();
                        mapped_window_id = Some(record.id.clone());
                    }

                    if let Some(location) = pending_location {
                        self.space.map_element(window, location, false);
                    }
                }

                if let Some(window_id) = mapped_window_id {
                    let _ = self.runtime().execute(RuntimeCommand::SyncWindowMapped {
                        window_id,
                        mapped: true,
                    });
                }

                return;
            }

            let location = pending_location.or_else(|| {
                self.find_window_mut(surface)
                    .and_then(|record| record.frame_sync.pending_location())
            });

            if let Some(location) = location {
                self.space.map_element(window, location, false);
                let _ = self.runtime().execute(RuntimeCommand::SyncWindowMapped {
                    window_id,
                    mapped: true,
                });
            }
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

    fn visible_geometry_candidates(&self) -> Vec<GeometryCandidate> {
        self.visible_managed_window_positions()
            .into_iter()
            .filter_map(|managed_index| {
                let record = &self.managed_windows[managed_index];
                let location = self.space.element_location(&record.window)?;
                Some(GeometryCandidate {
                    window_id: record.id.clone(),
                    rect: Rectangle::new(location, record.window.geometry().size),
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeometryCandidate {
    window_id: WindowId,
    rect: Rectangle<i32, smithay::utils::Logical>,
}

fn select_directional_focus_candidate(
    candidates: &[GeometryCandidate],
    current_focused_window_id: Option<WindowId>,
    direction: FocusDirection,
) -> Option<WindowId> {
    let current = current_focused_window_id
        .and_then(|window_id| candidates.iter().find(|candidate| candidate.window_id == window_id))?;
    let current_center = rect_center(current.rect);

    candidates
        .iter()
        .filter(|candidate| candidate.window_id != current.window_id)
        .filter_map(|candidate| {
            let candidate_center = rect_center(candidate.rect);
            directional_score(current_center, candidate_center, direction)
            .map(|score| (score, candidate.window_id.clone()))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, window_id)| window_id)
}

fn rect_center(rect: Rectangle<i32, smithay::utils::Logical>) -> (i32, i32) {
    (
        rect.loc.x + rect.size.w / 2,
        rect.loc.y + rect.size.h / 2,
    )
}

fn directional_score(
    current_center: (i32, i32),
    candidate_center: (i32, i32),
    direction: FocusDirection,
) -> Option<(i32, i32, i32)> {
    let dx = candidate_center.0 - current_center.0;
    let dy = candidate_center.1 - current_center.1;
    let total_distance = dx.abs() + dy.abs();

    match direction {
        FocusDirection::Left if dx < 0 => Some((total_distance, dy.abs(), -dx)),
        FocusDirection::Right if dx > 0 => Some((total_distance, dy.abs(), dx)),
        FocusDirection::Up if dy < 0 => Some((total_distance, dx.abs(), -dy)),
        FocusDirection::Down if dy > 0 => Some((total_distance, dx.abs(), dy)),
        _ => None,
    }
}

fn managed_window_swap_positions(
    window_order: &[WindowId],
    first_window_id: WindowId,
    second_window_id: WindowId,
) -> Option<(usize, usize)> {
    let first_index = window_order.iter().position(|window_id| *window_id == first_window_id)?;
    let second_index = window_order.iter().position(|window_id| *window_id == second_window_id)?;
    Some((first_index, second_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::utils::{Logical, Point, Rectangle, Size};

    fn candidate(id: u64, x: i32, y: i32, w: i32, h: i32) -> GeometryCandidate {
        GeometryCandidate {
            window_id: window_id(id),
            rect: Rectangle::<i32, Logical>::new(Point::from((x, y)), Size::from((w, h))),
        }
    }

    #[test]
    fn directional_focus_prefers_nearest_window_in_direction() {
        let candidates = vec![
            candidate(1, 0, 0, 100, 100),
            candidate(2, 140, 10, 100, 100),
            candidate(3, 320, 0, 100, 100),
        ];

        assert_eq!(
            select_directional_focus_candidate(&candidates, Some(window_id(1)), FocusDirection::Right),
            Some(window_id(2))
        );
    }

    #[test]
    fn directional_focus_filters_to_requested_axis() {
        let candidates = vec![
            candidate(1, 120, 120, 100, 100),
            candidate(2, 120, 0, 100, 100),
            candidate(3, 260, 120, 100, 100),
        ];

        assert_eq!(
            select_directional_focus_candidate(&candidates, Some(window_id(1)), FocusDirection::Up),
            Some(window_id(2))
        );
        assert_eq!(
            select_directional_focus_candidate(&candidates, Some(window_id(1)), FocusDirection::Left),
            None
        );
    }

    #[test]
    fn directional_focus_prefers_lower_cross_axis_offset() {
        let candidates = vec![
            candidate(1, 100, 100, 100, 100),
            candidate(2, 260, 90, 100, 100),
            candidate(3, 250, 220, 100, 100),
        ];

        assert_eq!(
            select_directional_focus_candidate(&candidates, Some(window_id(1)), FocusDirection::Right),
            Some(window_id(2))
        );
    }

    #[test]
    fn managed_window_swap_positions_resolves_both_indices() {
        let window_order = vec![window_id(10), window_id(20), window_id(30)];

        assert_eq!(
            managed_window_swap_positions(&window_order, window_id(10), window_id(30)),
            Some((0, 2))
        );
    }

    #[test]
    fn managed_window_swap_positions_requires_both_windows() {
        let window_order = vec![window_id(10), window_id(20)];

        assert_eq!(
            managed_window_swap_positions(&window_order, window_id(10), window_id(30)),
            None
        );
    }
}