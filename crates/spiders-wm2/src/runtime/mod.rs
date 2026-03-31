pub mod command;

pub use command::WmCommand;

use crate::actions::WmActions;
use crate::actions::focus::{FocusSelection, FocusUpdate};
use crate::actions::window::CloseSelection;
use crate::actions::workspace::WorkspaceSelection;
use crate::model::wm::WmModel;
use crate::model::{OutputId, SeatId, WindowId, WorkspaceId};
use crate::state::SpidersWm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    EnsureWorkspace { name: String },
    EnsureDefaultWorkspace { name: String },
    RequestSelectWorkspace {
        workspace_id: WorkspaceId,
        window_order: Vec<WindowId>,
    },
    RequestSelectNextWorkspace {
        window_order: Vec<WindowId>,
    },
    RequestSelectPreviousWorkspace {
        window_order: Vec<WindowId>,
    },
    EnsureSeat { seat_id: SeatId },
    SyncOutput {
        output_id: OutputId,
        name: String,
        logical_width: u32,
        logical_height: u32,
    },
    PlaceNewWindow { window_id: WindowId },
    RequestFocusWindowSelection {
        seat_id: SeatId,
        window_id: Option<WindowId>,
    },
    RequestFocusNextWindowSelection { seat_id: SeatId },
    RequestFocusPreviousWindowSelection { seat_id: SeatId },
    SyncHoveredWindow {
        seat_id: SeatId,
        hovered_window_id: Option<WindowId>,
    },
    SyncInteractedWindow {
        seat_id: SeatId,
        interacted_window_id: Option<WindowId>,
    },
    UnmapWindow { window_id: WindowId },
    RemoveWindow { window_id: WindowId },
    RequestCloseFocusedWindowSelection,
    AssignFocusedWindowToWorkspace {
        workspace_id: WorkspaceId,
        window_order: Vec<WindowId>,
    },
    ToggleAssignFocusedWindowToWorkspace {
        workspace_id: WorkspaceId,
        window_order: Vec<WindowId>,
    },
    ToggleFocusedWindowFloating,
    ToggleFocusedWindowFullscreen,
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
    WorkspaceSelection(Option<WorkspaceSelection>),
    FocusSelection(FocusSelection),
    CloseSelection(CloseSelection),
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
            RuntimeCommand::RequestSelectWorkspace {
                workspace_id,
                window_order,
            } => RuntimeResult::WorkspaceSelection(
                self.request_select_workspace(workspace_id, window_order),
            ),
            RuntimeCommand::RequestSelectNextWorkspace { window_order } => {
                RuntimeResult::WorkspaceSelection(self.request_select_next_workspace(window_order))
            }
            RuntimeCommand::RequestSelectPreviousWorkspace { window_order } => {
                RuntimeResult::WorkspaceSelection(self.request_select_previous_workspace(window_order))
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
            RuntimeCommand::RequestFocusWindowSelection { seat_id, window_id } => {
                RuntimeResult::FocusSelection(self.request_focus_window_selection(seat_id, window_id))
            }
            RuntimeCommand::RequestFocusNextWindowSelection { seat_id } => {
                RuntimeResult::FocusSelection(self.request_focus_next_window_selection(seat_id))
            }
            RuntimeCommand::RequestFocusPreviousWindowSelection { seat_id } => {
                RuntimeResult::FocusSelection(self.request_focus_previous_window_selection(seat_id))
            }
            RuntimeCommand::SyncHoveredWindow {
                seat_id,
                hovered_window_id,
            } => RuntimeResult::Window(self.sync_hovered_window(seat_id, hovered_window_id)),
            RuntimeCommand::SyncInteractedWindow {
                seat_id,
                interacted_window_id,
            } => RuntimeResult::Window(self.sync_interacted_window(seat_id, interacted_window_id)),
            RuntimeCommand::UnmapWindow { window_id } => {
                RuntimeResult::FocusUpdate(self.unmap_window(window_id))
            }
            RuntimeCommand::RemoveWindow { window_id } => {
                RuntimeResult::FocusUpdate(self.remove_window(window_id))
            }
            RuntimeCommand::RequestCloseFocusedWindowSelection => {
                RuntimeResult::CloseSelection(self.request_close_focused_window_selection())
            }
            RuntimeCommand::AssignFocusedWindowToWorkspace {
                workspace_id,
                window_order,
            } => RuntimeResult::FocusSelection(
                self.assign_focused_window_to_workspace(workspace_id, window_order),
            ),
            RuntimeCommand::ToggleAssignFocusedWindowToWorkspace {
                workspace_id,
                window_order,
            } => RuntimeResult::FocusSelection(
                self.toggle_assign_focused_window_to_workspace(workspace_id, window_order),
            ),
            RuntimeCommand::ToggleFocusedWindowFloating => {
                RuntimeResult::Window(self.toggle_focused_window_floating())
            }
            RuntimeCommand::ToggleFocusedWindowFullscreen => {
                RuntimeResult::Window(self.toggle_focused_window_fullscreen())
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

    pub fn request_select_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        self.actions.request_select_workspace(workspace_id, window_order)
    }

    pub fn request_select_next_workspace<I>(&mut self, window_order: I) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        self.actions.request_select_next_workspace(window_order)
    }

    pub fn request_select_previous_workspace<I>(
        &mut self,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        self.actions.request_select_previous_workspace(window_order)
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

    pub fn request_focus_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_id: Option<WindowId>,
    ) -> FocusSelection {
        self.actions.request_focus_window_selection(seat_id, window_id)
    }

    pub fn request_focus_next_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
    ) -> FocusSelection {
        self.actions.request_focus_next_window_selection(seat_id)
    }

    pub fn request_focus_previous_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
    ) -> FocusSelection {
        self.actions.request_focus_previous_window_selection(seat_id)
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

    pub fn unmap_window(&mut self, window_id: WindowId) -> FocusUpdate {
        self.actions.unmap_window(window_id)
    }

    pub fn request_close_focused_window_selection(&mut self) -> CloseSelection {
        self.actions.request_close_focused_window_selection()
    }

    pub fn assign_focused_window_to_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> FocusSelection
    where
        I: IntoIterator<Item = WindowId>,
    {
        self.actions
            .assign_focused_window_to_workspace(workspace_id, window_order)
    }

    pub fn toggle_assign_focused_window_to_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> FocusSelection
    where
        I: IntoIterator<Item = WindowId>,
    {
        self.actions
            .toggle_assign_focused_window_to_workspace(workspace_id, window_order)
    }

    pub fn toggle_focused_window_floating(&mut self) -> Option<WindowId> {
        self.actions.toggle_focused_window_floating()
    }

    pub fn toggle_focused_window_fullscreen(&mut self) -> Option<WindowId> {
        self.actions.toggle_focused_window_fullscreen()
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
    use crate::model::window_id;

    #[test]
    fn runtime_composes_action_surface() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.request_select_next_workspace(Vec::new());
        runtime.ensure_seat("winit");
        runtime.sync_output("winit", "winit", 1280, 720);
        runtime.place_new_window(window_id(1));
        runtime.request_focus_window_selection("winit", Some(window_id(1)));
        runtime.sync_window_mapped(window_id(1), true);

        assert_eq!(model.current_workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
        assert_eq!(model.focused_window_id, Some(window_id(1)));
        assert_eq!(model.windows.get(&window_id(1)).map(|window| window.mapped), Some(true));
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
        let selected_workspace = runtime.execute(RuntimeCommand::RequestSelectWorkspace {
            workspace_id: WorkspaceId("1".to_string()),
            window_order: Vec::new(),
        });
        let next_workspace = runtime.execute(RuntimeCommand::RequestSelectNextWorkspace {
            window_order: Vec::new(),
        });
        let previous_workspace = runtime.execute(RuntimeCommand::RequestSelectPreviousWorkspace {
            window_order: Vec::new(),
        });
        let requested_selection = runtime.execute(RuntimeCommand::RequestSelectWorkspace {
            workspace_id: WorkspaceId("2".to_string()),
            window_order: Vec::new(),
        });
        let focus_selection = runtime.execute(RuntimeCommand::RequestFocusWindowSelection {
            seat_id: SeatId("winit".to_string()),
            window_id: None,
        });
        let close_selection = runtime.execute(RuntimeCommand::RequestCloseFocusedWindowSelection);
        let assigned_workspace = runtime.execute(RuntimeCommand::AssignFocusedWindowToWorkspace {
            workspace_id: WorkspaceId("2".to_string()),
            window_order: Vec::new(),
        });
        let seat = runtime.execute(RuntimeCommand::EnsureSeat {
            seat_id: SeatId("winit".to_string()),
        });

        assert_eq!(workspace, RuntimeResult::Workspace(WorkspaceId("1".to_string())));
        assert_eq!(ensured_workspace, RuntimeResult::Workspace(WorkspaceId("2".to_string())));
        assert_eq!(
            selected_workspace,
            RuntimeResult::WorkspaceSelection(Some(WorkspaceSelection {
                workspace_id: WorkspaceId("1".to_string()),
                focused_window_id: None,
            }))
        );
        assert_eq!(
            next_workspace,
            RuntimeResult::WorkspaceSelection(Some(WorkspaceSelection {
                workspace_id: WorkspaceId("2".to_string()),
                focused_window_id: None,
            }))
        );
        assert_eq!(
            previous_workspace,
            RuntimeResult::WorkspaceSelection(Some(WorkspaceSelection {
                workspace_id: WorkspaceId("1".to_string()),
                focused_window_id: None,
            }))
        );
        assert_eq!(
            requested_selection,
            RuntimeResult::WorkspaceSelection(Some(WorkspaceSelection {
                workspace_id: WorkspaceId("2".to_string()),
                focused_window_id: None,
            }))
        );
        assert_eq!(
            focus_selection,
            RuntimeResult::FocusSelection(FocusSelection {
                focused_window_id: None,
            })
        );
        assert_eq!(
            close_selection,
            RuntimeResult::CloseSelection(CloseSelection {
                closing_window_id: None,
            })
        );
        assert_eq!(
            assigned_workspace,
            RuntimeResult::FocusSelection(FocusSelection {
                focused_window_id: None,
            })
        );
        assert_eq!(seat, RuntimeResult::Seat(SeatId("winit".to_string())));
    }

    #[test]
    fn runtime_executes_high_level_focus_and_close_requests() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.ensure_seat("winit");
        runtime.place_new_window(window_id(3));
        runtime.place_new_window(window_id(4));

        let focused = runtime.execute(RuntimeCommand::RequestFocusWindowSelection {
            seat_id: SeatId("winit".to_string()),
            window_id: Some(window_id(3)),
        });
        let next_focused = runtime.execute(RuntimeCommand::RequestFocusNextWindowSelection {
            seat_id: SeatId("winit".to_string()),
        });
        let previous_focused = runtime.execute(RuntimeCommand::RequestFocusPreviousWindowSelection {
            seat_id: SeatId("winit".to_string()),
        });
        let closing = runtime.execute(RuntimeCommand::RequestCloseFocusedWindowSelection);

        assert_eq!(
            focused,
            RuntimeResult::FocusSelection(FocusSelection {
                focused_window_id: Some(window_id(3)),
            })
        );
        assert_eq!(
            next_focused,
            RuntimeResult::FocusSelection(FocusSelection {
                focused_window_id: Some(window_id(4)),
            })
        );
        assert_eq!(
            previous_focused,
            RuntimeResult::FocusSelection(FocusSelection {
                focused_window_id: Some(window_id(3)),
            })
        );
        assert_eq!(
            closing,
            RuntimeResult::CloseSelection(CloseSelection {
                closing_window_id: Some(window_id(3)),
            })
        );
        assert_eq!(model.focused_window_id, Some(window_id(3)));
        assert_eq!(model.windows.get(&window_id(3)).map(|window| window.closing), Some(true));
    }

    #[test]
    fn runtime_assigns_focused_window_to_workspace() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.ensure_workspace("2");
        runtime.ensure_seat("winit");
        runtime.place_new_window(window_id(1));
        runtime.place_new_window(window_id(2));
        runtime.request_focus_window_selection("winit", Some(window_id(2)));

        let assigned = runtime.execute(RuntimeCommand::AssignFocusedWindowToWorkspace {
            workspace_id: WorkspaceId("2".to_string()),
            window_order: vec![window_id(1), window_id(2)],
        });

        assert_eq!(
            assigned,
            RuntimeResult::FocusSelection(FocusSelection {
                focused_window_id: Some(window_id(1)),
            })
        );
        assert_eq!(
            model.windows.get(&window_id(2)).and_then(|window| window.workspace_id.clone()),
            Some(WorkspaceId("2".to_string()))
        );
        assert_eq!(model.focused_window_id, Some(window_id(1)));
    }

    #[test]
    fn runtime_toggle_assign_uses_same_workspace_move_path() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.ensure_workspace("2");
        runtime.ensure_seat("winit");
        runtime.place_new_window(window_id(1));
        runtime.place_new_window(window_id(2));
        runtime.request_focus_window_selection("winit", Some(window_id(2)));

        let assigned = runtime.execute(RuntimeCommand::ToggleAssignFocusedWindowToWorkspace {
            workspace_id: WorkspaceId("2".to_string()),
            window_order: vec![window_id(1), window_id(2)],
        });

        assert_eq!(
            assigned,
            RuntimeResult::FocusSelection(FocusSelection {
                focused_window_id: Some(window_id(1)),
            })
        );
        assert_eq!(
            model.windows.get(&window_id(2)).and_then(|window| window.workspace_id.clone()),
            Some(WorkspaceId("2".to_string()))
        );
    }

    #[test]
    fn runtime_toggles_focused_window_floating() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.place_new_window(window_id(6));
        runtime.request_focus_window_selection("winit", Some(window_id(6)));

        let toggled = runtime.execute(RuntimeCommand::ToggleFocusedWindowFloating);

        assert_eq!(toggled, RuntimeResult::Window(Some(window_id(6))));
        assert_eq!(
            model.windows.get(&window_id(6)).map(|window| window.floating),
            Some(true)
        );
    }

    #[test]
    fn runtime_toggles_focused_window_fullscreen() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_default_workspace("1");
        runtime.place_new_window(window_id(7));
        runtime.request_focus_window_selection("winit", Some(window_id(7)));

        let toggled = runtime.execute(RuntimeCommand::ToggleFocusedWindowFullscreen);

        assert_eq!(toggled, RuntimeResult::Window(Some(window_id(7))));
        assert_eq!(
            model.windows.get(&window_id(7)).map(|window| window.fullscreen),
            Some(true)
        );
    }
}