use crate::actions::WmActions;
use crate::actions::focus::FocusUpdate;
use crate::model::wm::WmModel;
use crate::model::{OutputId, SeatId, WindowId, WorkspaceId};
use crate::state::SpidersWm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    EnsureWorkspace { name: String },
    EnsureDefaultWorkspace { name: String },
    SelectWorkspace { workspace_id: WorkspaceId },
    SelectNextWorkspace,
    EnsureSeat { seat_id: SeatId },
    SyncOutput {
        output_id: OutputId,
        name: String,
        logical_width: u32,
        logical_height: u32,
    },
    PlaceNewWindow { window_id: WindowId },
    RequestFocusWindow {
        seat_id: SeatId,
        window_id: Option<WindowId>,
    },
    RequestFocusNextWindow { seat_id: SeatId },
    SyncHoveredWindow {
        seat_id: SeatId,
        hovered_window_id: Option<WindowId>,
    },
    SyncInteractedWindow {
        seat_id: SeatId,
        interacted_window_id: Option<WindowId>,
    },
    RemoveWindow { window_id: WindowId },
    RequestCloseFocusedWindow,
    SyncWindowIdentity {
        window_id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
    },
    SyncWindowMapped {
        window_id: WindowId,
        mapped: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeResult {
    Workspace(WorkspaceId),
    Seat(SeatId),
    Output(OutputId),
    Window(Option<WindowId>),
    FocusUpdate(FocusUpdate),
}

pub struct WmRuntime<'a> {
    actions: WmActions<'a>,
}

impl<'a> WmRuntime<'a> {
    pub fn new(model: &'a mut WmModel) -> Self {
        Self {
            actions: WmActions::new(model),
        }
    }

    pub fn execute(&mut self, command: RuntimeCommand) -> RuntimeResult {
        match command {
            RuntimeCommand::EnsureWorkspace { name } => {
                RuntimeResult::Workspace(self.ensure_workspace(name))
            }
            RuntimeCommand::EnsureDefaultWorkspace { name } => {
                RuntimeResult::Workspace(self.ensure_default_workspace(name))
            }
            RuntimeCommand::SelectWorkspace { workspace_id } => {
                RuntimeResult::Workspace(self.select_workspace(workspace_id).unwrap_or_default())
            }
            RuntimeCommand::SelectNextWorkspace => {
                RuntimeResult::Workspace(self.select_next_workspace().unwrap_or_default())
            }
            RuntimeCommand::EnsureSeat { seat_id } => RuntimeResult::Seat(self.ensure_seat(seat_id)),
            RuntimeCommand::SyncOutput {
                output_id,
                name,
                logical_width,
                logical_height,
            } => RuntimeResult::Output(self.sync_output(output_id, name, logical_width, logical_height)),
            RuntimeCommand::PlaceNewWindow { window_id } => {
                RuntimeResult::Window(Some(self.place_new_window(window_id)))
            }
            RuntimeCommand::RequestFocusWindow { seat_id, window_id } => {
                RuntimeResult::Window(self.request_focus_window(seat_id, window_id))
            }
            RuntimeCommand::RequestFocusNextWindow { seat_id } => {
                RuntimeResult::Window(self.request_focus_next_window(seat_id))
            }
            RuntimeCommand::SyncHoveredWindow {
                seat_id,
                hovered_window_id,
            } => RuntimeResult::Window(self.sync_hovered_window(seat_id, hovered_window_id)),
            RuntimeCommand::SyncInteractedWindow {
                seat_id,
                interacted_window_id,
            } => RuntimeResult::Window(self.sync_interacted_window(seat_id, interacted_window_id)),
            RuntimeCommand::RemoveWindow { window_id } => {
                RuntimeResult::FocusUpdate(self.remove_window(window_id))
            }
            RuntimeCommand::RequestCloseFocusedWindow => {
                RuntimeResult::Window(self.request_close_focused_window())
            }
            RuntimeCommand::SyncWindowIdentity {
                window_id,
                title,
                app_id,
            } => RuntimeResult::Window(self.sync_window_identity(window_id, title, app_id)),
            RuntimeCommand::SyncWindowMapped { window_id, mapped } => {
                RuntimeResult::Window(self.sync_window_mapped(window_id, mapped))
            }
        }
    }

    pub fn ensure_default_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        self.actions.ensure_default_workspace(name)
    }

    pub fn ensure_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        self.actions.ensure_workspace(name)
    }

    pub fn select_workspace(&mut self, workspace_id: WorkspaceId) -> Option<WorkspaceId> {
        self.actions.select_workspace(workspace_id)
    }

    pub fn select_next_workspace(&mut self) -> Option<WorkspaceId> {
        self.actions.select_next_workspace()
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

    pub fn request_focus_window(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        self.actions.request_focus_window(seat_id, window_id)
    }

    pub fn request_focus_next_window(&mut self, seat_id: impl Into<SeatId>) -> Option<WindowId> {
        self.actions.request_focus_next_window(seat_id)
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

    pub fn request_close_focused_window(&mut self) -> Option<WindowId> {
        self.actions.request_close_focused_window()
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
        runtime.select_next_workspace();
        runtime.ensure_seat("winit");
        runtime.sync_output("winit", "winit", 1280, 720);
        runtime.place_new_window(WindowId(1));
        runtime.request_focus_window("winit", Some(WindowId(1)));
        runtime.sync_window_mapped(WindowId(1), true);

        assert_eq!(model.current_workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
        assert_eq!(model.focused_window_id, Some(WindowId(1)));
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.mapped), Some(true));
    }

    #[test]
    fn runtime_executes_commands() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        let workspace = runtime.execute(RuntimeCommand::EnsureDefaultWorkspace {
            name: "1".to_string(),
        });
        let ensured_workspace = runtime.execute(RuntimeCommand::EnsureWorkspace {
            name: "2".to_string(),
        });
        let selected_workspace = runtime.execute(RuntimeCommand::SelectWorkspace {
            workspace_id: WorkspaceId("1".to_string()),
        });
        let next_workspace = runtime.execute(RuntimeCommand::SelectNextWorkspace);
        let seat = runtime.execute(RuntimeCommand::EnsureSeat {
            seat_id: SeatId("winit".to_string()),
        });

        assert_eq!(workspace, RuntimeResult::Workspace(WorkspaceId("1".to_string())));
        assert_eq!(ensured_workspace, RuntimeResult::Workspace(WorkspaceId("2".to_string())));
        assert_eq!(selected_workspace, RuntimeResult::Workspace(WorkspaceId("1".to_string())));
        assert_eq!(next_workspace, RuntimeResult::Workspace(WorkspaceId("2".to_string())));
        assert_eq!(seat, RuntimeResult::Seat(SeatId("winit".to_string())));
    }

    #[test]
    fn runtime_executes_high_level_focus_and_close_requests() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.ensure_seat("winit");
        runtime.place_new_window(WindowId(3));
        runtime.place_new_window(WindowId(4));

        let focused = runtime.execute(RuntimeCommand::RequestFocusWindow {
            seat_id: SeatId("winit".to_string()),
            window_id: Some(WindowId(3)),
        });
        let next_focused = runtime.execute(RuntimeCommand::RequestFocusNextWindow {
            seat_id: SeatId("winit".to_string()),
        });
        let closing = runtime.execute(RuntimeCommand::RequestCloseFocusedWindow);

        assert_eq!(focused, RuntimeResult::Window(Some(WindowId(3))));
        assert_eq!(next_focused, RuntimeResult::Window(Some(WindowId(4))));
        assert_eq!(closing, RuntimeResult::Window(Some(WindowId(4))));
        assert_eq!(model.focused_window_id, Some(WindowId(4)));
        assert_eq!(model.windows.get(&WindowId(4)).map(|window| window.closing), Some(true));
    }
}