use spiders_core::command::FocusDirection;
use tracing::info;

use crate::state::SpidersWm;
use spiders_core::WindowId;
use spiders_core::navigation::{
    NavigationDirection, WindowGeometryCandidate, managed_window_swap_positions,
    select_directional_focus_candidate,
};
use spiders_core::wm::WindowGeometry;

impl SpidersWm {
    pub fn focus_next_window(&mut self, serial: smithay::utils::Serial) {
        let window_order = self.managed_window_ids();
        let (next_focus_window_id, events) = {
            let mut runtime = self.runtime();
            let next_focus_window_id = runtime
                .request_focus_next_window_selection("winit", window_order)
                .focused_window_id;
            (next_focus_window_id, runtime.take_events())
        };

        self.broadcast_runtime_events(events);
        self.apply_modeled_focus(next_focus_window_id, serial);
    }

    pub fn focus_previous_window(&mut self, serial: smithay::utils::Serial) {
        let window_order = self.managed_window_ids();
        let (previous_focus_window_id, events) = {
            let mut runtime = self.runtime();
            let previous_focus_window_id = runtime
                .request_focus_previous_window_selection("winit", window_order)
                .focused_window_id;
            (previous_focus_window_id, runtime.take_events())
        };

        self.broadcast_runtime_events(events);
        self.apply_modeled_focus(previous_focus_window_id, serial);
    }

    pub fn focus_direction_window(
        &mut self,
        direction: FocusDirection,
        serial: smithay::utils::Serial,
    ) {
        let current_focused_window_id =
            self.focused_surface.as_ref().and_then(|surface| self.window_id_for_surface(surface));
        let candidates = self.visible_geometry_candidates();

        let next_focus_window_id = match select_directional_focus_candidate(
            &candidates,
            current_focused_window_id,
            navigation_direction(direction),
            &self.model.last_focused_window_id_by_scope,
            self.model.focus_tree.as_ref(),
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

    pub fn focus_window_by_id(&mut self, window_id: WindowId, serial: smithay::utils::Serial) {
        let Some(target_surface) = self.surface_for_window_id(window_id.clone()) else {
            return;
        };

        let target_workspace_id = self.window_workspace_id(&window_id);

        if let Some(workspace_id) = target_workspace_id {
            if self.current_workspace_id() != Some(&workspace_id) {
                let window_order = self.managed_window_ids();
                let selection = match {
                    let mut runtime = self.runtime();
                    let selection = runtime.request_select_workspace(workspace_id, window_order);
                    let events = runtime.take_events();
                    self.broadcast_runtime_events(events);
                    selection
                } {
                    Some(selection) => selection,
                    None => return,
                };
                self.apply_workspace_selection(selection.focused_window_id, serial);
            }
        }

        self.set_focus(Some(target_surface), serial);
    }

    pub fn swap_focused_window_direction(&mut self, direction: FocusDirection) {
        let Some(current_focused_window_id) =
            self.focused_surface.as_ref().and_then(|surface| self.window_id_for_surface(surface))
        else {
            return;
        };

        let candidates = self.visible_geometry_candidates();
        let Some(target_window_id) = select_directional_focus_candidate(
            &candidates,
            Some(current_focused_window_id.clone()),
            navigation_direction(direction),
            &self.model.last_focused_window_id_by_scope,
            self.model.focus_tree.as_ref(),
        ) else {
            return;
        };

        let window_order = self.managed_window_ids();
        let Some((focused_index, target_index)) = managed_window_swap_positions(
            &window_order,
            current_focused_window_id.clone(),
            target_window_id.clone(),
        ) else {
            return;
        };

        self.swap_managed_window_positions(focused_index, target_index);
        self.schedule_relayout();
        info!(
            ?direction,
            ?current_focused_window_id,
            ?target_window_id,
            "swapped focused window with directional neighbor"
        );
    }

    pub(crate) fn visible_geometry_candidates(&self) -> Vec<WindowGeometryCandidate> {
        self.visible_managed_window_positions()
            .into_iter()
            .filter_map(|managed_index| {
                let record = self.managed_window_at(managed_index)?;
                let location = self.element_location(&record.window)?;
                Some(WindowGeometryCandidate {
                    window_id: record.id.clone(),
                    geometry: WindowGeometry {
                        x: location.x,
                        y: location.y,
                        width: record.window.geometry().size.w,
                        height: record.window.geometry().size.h,
                    },
                    scope_path: self
                        .model
                        .focus_scope_path(&record.id)
                        .map(|scope_path| scope_path.to_vec())
                        .unwrap_or_default(),
                })
            })
            .collect()
    }
}

fn navigation_direction(direction: FocusDirection) -> NavigationDirection {
    match direction {
        FocusDirection::Left => NavigationDirection::Left,
        FocusDirection::Right => NavigationDirection::Right,
        FocusDirection::Up => NavigationDirection::Up,
        FocusDirection::Down => NavigationDirection::Down,
    }
}
