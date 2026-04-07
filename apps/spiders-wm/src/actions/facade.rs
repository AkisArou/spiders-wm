use crate::actions::focus::FocusSelection;
use crate::actions::window::CloseSelection;
use crate::actions::workspace::WorkspaceSelection;
use crate::actions::{focus, output, seat, window, workspace};
use spiders_core::wm::WmModel;
use spiders_core::{OutputId, SeatId, WindowId, WorkspaceId};

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

    pub fn ensure_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        workspace::ensure_workspace(self.model, name)
    }

    pub fn request_select_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_ids: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        workspace::request_select_workspace(self.model, workspace_id, window_ids)
    }

    pub fn request_select_next_workspace<I>(&mut self, window_ids: I) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        workspace::request_select_next_workspace(self.model, window_ids)
    }

    pub fn request_select_previous_workspace<I>(
        &mut self,
        window_ids: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        workspace::request_select_previous_workspace(self.model, window_ids)
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

    pub fn request_focus_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_id: Option<WindowId>,
    ) -> FocusSelection {
        let selection = focus::request_focus_window(self.model, window_id);
        let focused_window_id =
            seat::sync_focused_window(self.model, seat_id, selection.focused_window_id);
        FocusSelection { focused_window_id }
    }

    pub fn request_focus_next_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> FocusSelection {
        let selection = focus::request_focus_next_window(self.model, window_order);
        let focused_window_id =
            seat::sync_focused_window(self.model, seat_id, selection.focused_window_id);
        FocusSelection { focused_window_id }
    }

    pub fn request_focus_previous_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> FocusSelection {
        let selection = focus::request_focus_previous_window(self.model, window_order);
        let focused_window_id =
            seat::sync_focused_window(self.model, seat_id, selection.focused_window_id);
        FocusSelection { focused_window_id }
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

    pub fn remove_window(
        &mut self,
        window_id: WindowId,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> focus::FocusUpdate {
        focus::remove_window(self.model, window_id, window_order)
    }

    pub fn unmap_window(
        &mut self,
        window_id: WindowId,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> focus::FocusUpdate {
        focus::unmap_window(self.model, window_id, window_order)
    }

    pub fn request_close_focused_window_selection(&mut self) -> CloseSelection {
        window::request_close_focused_window(self.model)
    }

    pub fn assign_focused_window_to_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_ids: I,
    ) -> FocusSelection
    where
        I: IntoIterator<Item = WindowId>,
    {
        FocusSelection {
            focused_window_id: window::assign_focused_window_to_workspace(
                self.model,
                workspace_id,
                window_ids,
            ),
        }
    }

    pub fn toggle_assign_focused_window_to_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_ids: I,
    ) -> FocusSelection
    where
        I: IntoIterator<Item = WindowId>,
    {
        FocusSelection {
            focused_window_id: window::toggle_assign_focused_window_to_workspace(
                self.model,
                workspace_id,
                window_ids,
            ),
        }
    }

    pub fn toggle_focused_window_floating(&mut self) -> Option<WindowId> {
        window::toggle_focused_window_floating(self.model)
    }

    pub fn toggle_focused_window_fullscreen(&mut self) -> Option<WindowId> {
        window::toggle_focused_window_fullscreen(self.model)
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
        if !self.model.has_window(&window_id) {
            return None;
        }

        self.model.set_window_mapped(window_id.clone(), mapped);
        Some(window_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::window_id;

    #[test]
    fn facade_composes_workspace_output_window_and_focus_actions() {
        let mut model = WmModel::default();
        let mut actions = WmActions::new(&mut model);

        actions.ensure_default_workspace("1");
        actions.ensure_seat("winit");
        actions.sync_output("winit", "winit", 1280, 720);
        actions.place_new_window(window_id(5));
        let focused = actions.request_focus_window_selection("winit", Some(window_id(5)));
        let mapped = actions.sync_window_mapped(window_id(5), true);

        assert_eq!(focused.focused_window_id, Some(window_id(5)));
        assert_eq!(mapped, Some(window_id(5)));
        assert_eq!(model.current_workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
        assert_eq!(model.focused_window_id, Some(window_id(5)));
        assert_eq!(model.windows.get(&window_id(5)).map(|window| window.mapped), Some(true));
        assert_eq!(
            model
                .seats
                .get(&SeatId("winit".to_string()))
                .and_then(|seat| seat.focused_window_id.clone()),
            Some(window_id(5))
        );
    }
}
