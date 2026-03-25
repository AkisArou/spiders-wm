use crate::actions::{focus, output, seat, window, workspace};
use crate::model::wm::WmModel;
use crate::model::{OutputId, SeatId, WindowId, WorkspaceId};

pub struct WmActions<'a> {
    model: &'a mut WmModel,
}

impl<'a> WmActions<'a> {
    pub fn new(model: &'a mut WmModel) -> Self {
        Self { model }
    }

    pub fn ensure_default_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        workspace::ensure_default_workspace(self.model, name)
    }

    pub fn ensure_seat(&mut self, seat_id: impl Into<SeatId>) -> SeatId {
        seat::ensure_seat(self.model, seat_id)
    }

    pub fn sync_output(
        &mut self,
        output_id: impl Into<OutputId>,
        name: impl Into<String>,
        logical_width: u32,
        logical_height: u32,
    ) -> OutputId {
        output::sync_output(self.model, output_id, name, logical_width, logical_height)
    }

    pub fn place_new_window(&mut self, window_id: WindowId) -> WindowId {
        workspace::place_new_window(self.model, window_id)
    }

    pub fn sync_focus(
        &mut self,
        seat_id: impl Into<SeatId>,
        focused_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        let focused_window_id = focus::set_focused_window(self.model, focused_window_id);
        seat::sync_focused_window(self.model, seat_id, focused_window_id)
    }

    pub fn sync_hovered_window(
        &mut self,
        seat_id: impl Into<SeatId>,
        hovered_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        seat::sync_hovered_window(self.model, seat_id, hovered_window_id)
    }

    pub fn sync_interacted_window(
        &mut self,
        seat_id: impl Into<SeatId>,
        interacted_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        seat::sync_interacted_window(self.model, seat_id, interacted_window_id)
    }

    pub fn remove_window(&mut self, window_id: WindowId) -> focus::FocusUpdate {
        focus::remove_window(self.model, window_id)
    }

    pub fn mark_focused_window_closing(&mut self) -> Option<WindowId> {
        window::mark_focused_window_closing(self.model)
    }

    pub fn sync_window_identity(
        &mut self,
        window_id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
    ) -> Option<WindowId> {
        window::sync_window_identity(self.model, window_id, title, app_id)
    }

    pub fn sync_window_mapped(&mut self, window_id: WindowId, mapped: bool) -> Option<WindowId> {
        if !self.model.windows.contains_key(&window_id) {
            return None;
        }

        self.model.set_window_mapped(window_id, mapped);
        Some(window_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_composes_workspace_output_window_and_focus_actions() {
        let mut model = WmModel::default();
        let mut actions = WmActions::new(&mut model);

        actions.ensure_default_workspace("1");
        actions.ensure_seat("winit");
        actions.sync_output("winit", "winit", 1280, 720);
        actions.place_new_window(WindowId(5));
        let focused = actions.sync_focus("winit", Some(WindowId(5)));
        let mapped = actions.sync_window_mapped(WindowId(5), true);

        assert_eq!(focused, Some(WindowId(5)));
        assert_eq!(mapped, Some(WindowId(5)));
        assert_eq!(model.current_workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
        assert_eq!(model.focused_window_id, Some(WindowId(5)));
        assert_eq!(model.windows.get(&WindowId(5)).map(|window| window.mapped), Some(true));
        assert_eq!(
            model.seats.get(&SeatId("winit".to_string())).and_then(|seat| seat.focused_window_id),
            Some(WindowId(5))
        );
    }
}