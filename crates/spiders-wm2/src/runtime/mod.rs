use crate::actions::WmActions;
use crate::actions::focus::FocusUpdate;
use crate::model::wm::WmModel;
use crate::model::{OutputId, SeatId, WindowId, WorkspaceId};
use crate::state::SpidersWm;

pub struct WmRuntime<'a> {
    actions: WmActions<'a>,
}

impl<'a> WmRuntime<'a> {
    pub fn new(model: &'a mut WmModel) -> Self {
        Self {
            actions: WmActions::new(model),
        }
    }

    pub fn ensure_default_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        self.actions.ensure_default_workspace(name)
    }

    pub fn ensure_seat(&mut self, seat_id: impl Into<SeatId>) -> SeatId {
        self.actions.ensure_seat(seat_id)
    }

    pub fn sync_output(
        &mut self,
        output_id: impl Into<OutputId>,
        name: impl Into<String>,
        logical_width: u32,
        logical_height: u32,
    ) -> OutputId {
        self.actions
            .sync_output(output_id, name, logical_width, logical_height)
    }

    pub fn place_new_window(&mut self, window_id: WindowId) -> WindowId {
        self.actions.place_new_window(window_id)
    }

    pub fn sync_focus(
        &mut self,
        seat_id: impl Into<SeatId>,
        focused_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        self.actions.sync_focus(seat_id, focused_window_id)
    }

    pub fn sync_hovered_window(
        &mut self,
        seat_id: impl Into<SeatId>,
        hovered_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        self.actions.sync_hovered_window(seat_id, hovered_window_id)
    }

    pub fn sync_interacted_window(
        &mut self,
        seat_id: impl Into<SeatId>,
        interacted_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        self.actions
            .sync_interacted_window(seat_id, interacted_window_id)
    }

    pub fn remove_window(&mut self, window_id: WindowId) -> FocusUpdate {
        self.actions.remove_window(window_id)
    }

    pub fn mark_focused_window_closing(&mut self) -> Option<WindowId> {
        self.actions.mark_focused_window_closing()
    }

    pub fn sync_window_identity(
        &mut self,
        window_id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
    ) -> Option<WindowId> {
        self.actions.sync_window_identity(window_id, title, app_id)
    }

    pub fn sync_window_mapped(&mut self, window_id: WindowId, mapped: bool) -> Option<WindowId> {
        self.actions.sync_window_mapped(window_id, mapped)
    }
}

impl SpidersWm {
    pub fn runtime(&mut self) -> WmRuntime<'_> {
        WmRuntime::new(&mut self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_composes_action_surface() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.ensure_seat("winit");
        runtime.sync_output("winit", "winit", 1280, 720);
        runtime.place_new_window(WindowId(1));
        runtime.sync_focus("winit", Some(WindowId(1)));
        runtime.sync_window_mapped(WindowId(1), true);

        assert_eq!(model.current_workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
        assert_eq!(model.focused_window_id, Some(WindowId(1)));
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.mapped), Some(true));
    }
}