use std::collections::BTreeMap;

use super::output::OutputModel;
use super::seat::SeatModel;
use super::window::WindowModel;
use super::workspace::WorkspaceModel;
use super::{OutputId, SeatId, WindowId, WorkspaceId};

/// Top-level compositor model.
///
/// The current Smithay shell still owns the live backend objects. This type exists so
/// future actions, scene integration, and runtime/config services can target stable state
/// rather than `Space<Window>` and handler-local bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WmModel {
    pub windows: BTreeMap<WindowId, WindowModel>,
    pub workspaces: BTreeMap<WorkspaceId, WorkspaceModel>,
    pub outputs: BTreeMap<OutputId, OutputModel>,
    pub seats: BTreeMap<SeatId, SeatModel>,
    pub focused_window_id: Option<WindowId>,
    pub current_workspace_id: Option<WorkspaceId>,
    pub current_output_id: Option<OutputId>,
}

impl WmModel {
    pub fn upsert_seat(&mut self, seat_id: SeatId) {
        self.seats.entry(seat_id.clone()).or_insert_with(|| SeatModel {
            id: seat_id,
            ..SeatModel::default()
        });
    }

    pub fn upsert_workspace(&mut self, workspace_id: WorkspaceId, name: String) {
        self.workspaces
            .entry(workspace_id.clone())
            .and_modify(|workspace| {
                workspace.name = name.clone();
            })
            .or_insert_with(|| WorkspaceModel {
                id: workspace_id,
                name,
                output_id: None,
                focused: false,
                visible: false,
            });
    }

    pub fn set_current_workspace(&mut self, workspace_id: WorkspaceId) {
        self.current_workspace_id = Some(workspace_id.clone());

        for (candidate_id, workspace) in &mut self.workspaces {
            let is_current = *candidate_id == workspace_id;
            workspace.focused = is_current;
            workspace.visible = is_current;
        }
    }

    pub fn upsert_output(
        &mut self,
        output_id: impl Into<OutputId>,
        name: impl Into<String>,
        logical_width: u32,
        logical_height: u32,
        focused_workspace_id: Option<WorkspaceId>,
    ) {
        let output_id = output_id.into();
        let name = name.into();

        self.outputs
            .entry(output_id.clone())
            .and_modify(|output| {
                output.name = name.clone();
                output.logical_width = logical_width;
                output.logical_height = logical_height;
                output.enabled = true;
                output.focused_workspace_id = focused_workspace_id.clone();
            })
            .or_insert_with(|| OutputModel {
                id: output_id.clone(),
                name,
                logical_x: 0,
                logical_y: 0,
                logical_width,
                logical_height,
                enabled: true,
                focused_workspace_id,
            });
    }

    pub fn attach_workspace_to_output(&mut self, workspace_id: WorkspaceId, output_id: OutputId) {
        if let Some(workspace) = self.workspaces.get_mut(&workspace_id) {
            workspace.output_id = Some(output_id);
            workspace.focused = true;
            workspace.visible = true;
        }
    }

    pub fn set_current_output(&mut self, output_id: OutputId) {
        self.current_output_id = Some(output_id);
    }

    pub fn set_seat_focused_window(&mut self, seat_id: SeatId, focused_window_id: Option<WindowId>) {
        if let Some(seat) = self.seats.get_mut(&seat_id) {
            seat.focused_window_id = focused_window_id;
        }
    }

    pub fn set_seat_hovered_window(&mut self, seat_id: SeatId, hovered_window_id: Option<WindowId>) {
        if let Some(seat) = self.seats.get_mut(&seat_id) {
            seat.hovered_window_id = hovered_window_id;
        }
    }

    pub fn set_seat_interacted_window(&mut self, seat_id: SeatId, interacted_window_id: Option<WindowId>) {
        if let Some(seat) = self.seats.get_mut(&seat_id) {
            seat.interacted_window_id = interacted_window_id;
        }
    }

    pub fn insert_window(
        &mut self,
        id: WindowId,
        workspace_id: Option<WorkspaceId>,
        output_id: Option<OutputId>,
    ) {
        self.windows.insert(
            id,
            WindowModel {
                id,
                output_id,
                workspace_id,
                ..WindowModel::default()
            },
        );
    }

    pub fn window_is_on_current_workspace(&self, id: WindowId) -> bool {
        let Some(window) = self.windows.get(&id) else {
            return false;
        };

        match self.current_workspace_id.as_ref() {
            Some(current_workspace_id) => window.workspace_id.as_ref() == Some(current_workspace_id),
            None => true,
        }
    }

    pub fn set_window_mapped(&mut self, id: WindowId, mapped: bool) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.mapped = mapped;
        }
    }

    pub fn set_window_focused(&mut self, focused_id: Option<WindowId>) {
        self.focused_window_id = focused_id;

        for (window_id, window) in &mut self.windows {
            window.focused = Some(*window_id) == self.focused_window_id;
        }
    }

    pub fn set_window_closing(&mut self, id: WindowId, closing: bool) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.closing = closing;
        }
    }

    pub fn set_window_identity(
        &mut self,
        id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
    ) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.title = title;
            window.app_id = app_id;
        }
    }

    pub fn remove_window(&mut self, id: WindowId) {
        self.windows.remove(&id);
        if self.focused_window_id == Some(id) {
            self.focused_window_id = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_current_workspace_updates_focus_and_visibility() {
        let mut model = WmModel::default();
        model.upsert_workspace(WorkspaceId("1".to_string()), "1".to_string());
        model.upsert_workspace(WorkspaceId("2".to_string()), "2".to_string());

        model.set_current_workspace(WorkspaceId("2".to_string()));

        assert_eq!(model.current_workspace_id, Some(WorkspaceId("2".to_string())));
        assert_eq!(
            model.workspaces.get(&WorkspaceId("1".to_string())).map(|workspace| workspace.focused),
            Some(false)
        );
        assert_eq!(
            model.workspaces.get(&WorkspaceId("2".to_string())).map(|workspace| workspace.focused),
            Some(true)
        );
        assert_eq!(
            model.workspaces.get(&WorkspaceId("1".to_string())).map(|workspace| workspace.visible),
            Some(false)
        );
        assert_eq!(
            model.workspaces.get(&WorkspaceId("2".to_string())).map(|workspace| workspace.visible),
            Some(true)
        );
    }

    #[test]
    fn setting_seat_focused_window_updates_that_seat() {
        let mut model = WmModel::default();
        model.upsert_seat(SeatId("winit".to_string()));

        model.set_seat_focused_window(SeatId("winit".to_string()), Some(WindowId(7)));

        assert_eq!(
            model.seats.get(&SeatId("winit".to_string())).and_then(|seat| seat.focused_window_id),
            Some(WindowId(7))
        );
    }

    #[test]
    fn setting_seat_hovered_and_interacted_window_updates_that_seat() {
        let mut model = WmModel::default();
        model.upsert_seat(SeatId("winit".to_string()));

        model.set_seat_hovered_window(SeatId("winit".to_string()), Some(WindowId(3)));
        model.set_seat_interacted_window(SeatId("winit".to_string()), Some(WindowId(4)));

        let seat = model.seats.get(&SeatId("winit".to_string())).expect("seat missing");
        assert_eq!(seat.hovered_window_id, Some(WindowId(3)));
        assert_eq!(seat.interacted_window_id, Some(WindowId(4)));
    }

    #[test]
    fn upserting_output_assigns_current_workspace() {
        let mut model = WmModel::default();
        model.upsert_workspace(WorkspaceId("1".to_string()), "1".to_string());
        model.set_current_workspace(WorkspaceId("1".to_string()));

        model.upsert_output(
            "winit",
            "winit",
            1280,
            720,
            Some(WorkspaceId("1".to_string())),
        );
        model.attach_workspace_to_output(
            WorkspaceId("1".to_string()),
            OutputId("winit".to_string()),
        );
        model.set_current_output(OutputId("winit".to_string()));

        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
        assert_eq!(
            model.outputs.get(&OutputId("winit".to_string())).map(|output| output.logical_width),
            Some(1280)
        );
        assert_eq!(
            model.workspaces.get(&WorkspaceId("1".to_string())).and_then(|workspace| workspace.output_id.clone()),
            Some(OutputId("winit".to_string()))
        );
    }

    #[test]
    fn inserting_window_uses_current_workspace_and_output() {
        let mut model = WmModel::default();
        model.upsert_workspace(WorkspaceId("1".to_string()), "1".to_string());
        model.set_current_workspace(WorkspaceId("1".to_string()));
        model.upsert_output(
            "winit",
            "winit",
            1280,
            720,
            Some(WorkspaceId("1".to_string())),
        );
        model.attach_workspace_to_output(
            WorkspaceId("1".to_string()),
            OutputId("winit".to_string()),
        );
        model.set_current_output(OutputId("winit".to_string()));

        model.insert_window(
            WindowId(7),
            model.current_workspace_id.clone(),
            model.current_output_id.clone(),
        );

        let window = model.windows.get(&WindowId(7)).expect("window missing");
        assert_eq!(window.workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(window.output_id, Some(OutputId("winit".to_string())));
        assert!(!window.mapped);
    }

    #[test]
    fn focusing_window_updates_focus_flags() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(2), None, None);

        model.set_window_focused(Some(WindowId(2)));

        assert_eq!(model.focused_window_id, Some(WindowId(2)));
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.focused), Some(false));
        assert_eq!(model.windows.get(&WindowId(2)).map(|window| window.focused), Some(true));
    }

    #[test]
    fn setting_window_identity_updates_title_and_app_id() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(3), None, None);

        model.set_window_identity(
            WindowId(3),
            Some("Terminal".to_string()),
            Some("foot".to_string()),
        );

        let window = model.windows.get(&WindowId(3)).expect("window missing");
        assert_eq!(window.title.as_deref(), Some("Terminal"));
        assert_eq!(window.app_id.as_deref(), Some("foot"));
    }

    #[test]
    fn current_workspace_window_membership_is_explicit() {
        let mut model = WmModel::default();
        model.upsert_workspace(WorkspaceId("1".to_string()), "1".to_string());
        model.upsert_workspace(WorkspaceId("2".to_string()), "2".to_string());
        model.set_current_workspace(WorkspaceId("2".to_string()));
        model.insert_window(
            WindowId(1),
            Some(WorkspaceId("1".to_string())),
            Some(OutputId("winit".to_string())),
        );
        model.insert_window(
            WindowId(2),
            Some(WorkspaceId("2".to_string())),
            Some(OutputId("winit".to_string())),
        );

        assert!(!model.window_is_on_current_workspace(WindowId(1)));
        assert!(model.window_is_on_current_workspace(WindowId(2)));
        assert!(!model.window_is_on_current_workspace(WindowId(99)));
    }
}