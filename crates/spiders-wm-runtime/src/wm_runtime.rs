use spiders_core::focus::{
    FocusSelection, FocusUpdate, remove_window, request_focus_next_window,
    request_focus_previous_window, request_focus_window, unmap_window,
};
use spiders_core::wm::WmModel;
use spiders_core::workspace::{
    WorkspaceSelection, ensure_default_workspace, ensure_workspace, place_new_window,
    request_select_next_workspace, request_select_previous_workspace, request_select_workspace,
};
use spiders_core::{OutputId, SeatId, WindowId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    EnsureWorkspace { name: String },
    EnsureDefaultWorkspace { name: String },
    RequestSelectWorkspace { workspace_id: WorkspaceId, window_order: Vec<WindowId> },
    RequestSelectNextWorkspace { window_order: Vec<WindowId> },
    RequestSelectPreviousWorkspace { window_order: Vec<WindowId> },
    EnsureSeat { seat_id: SeatId },
    SyncOutput { output_id: OutputId, name: String, logical_width: u32, logical_height: u32 },
    PlaceNewWindow { window_id: WindowId },
    RequestFocusWindowSelection { seat_id: SeatId, window_id: Option<WindowId> },
    RequestFocusNextWindowSelection { seat_id: SeatId, window_order: Vec<WindowId> },
    RequestFocusPreviousWindowSelection { seat_id: SeatId, window_order: Vec<WindowId> },
    SyncHoveredWindow { seat_id: SeatId, hovered_window_id: Option<WindowId> },
    SyncInteractedWindow { seat_id: SeatId, interacted_window_id: Option<WindowId> },
    UnmapWindow { window_id: WindowId, window_order: Vec<WindowId> },
    RemoveWindow { window_id: WindowId, window_order: Vec<WindowId> },
    RequestCloseFocusedWindowSelection,
    AssignFocusedWindowToWorkspace { workspace_id: WorkspaceId, window_order: Vec<WindowId> },
    ToggleAssignFocusedWindowToWorkspace { workspace_id: WorkspaceId, window_order: Vec<WindowId> },
    ToggleFocusedWindowFloating,
    ToggleFocusedWindowFullscreen,
    SyncWindowIdentity { window_id: WindowId, title: Option<String>, app_id: Option<String> },
    SyncWindowMapped { window_id: WindowId, mapped: bool },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseSelection {
    pub closing_window_id: Option<WindowId>,
}

pub struct WmRuntime<'a> {
    model: &'a mut WmModel,
}

impl<'a> WmRuntime<'a> {
    pub fn new(model: &'a mut WmModel) -> Self {
        Self { model }
    }

    pub fn execute(&mut self, command: RuntimeCommand) -> RuntimeResult {
        match command {
            RuntimeCommand::EnsureWorkspace { name } => {
                RuntimeResult::Workspace(self.ensure_workspace(name))
            }
            RuntimeCommand::EnsureDefaultWorkspace { name } => {
                RuntimeResult::Workspace(self.ensure_default_workspace(name))
            }
            RuntimeCommand::RequestSelectWorkspace { workspace_id, window_order } => {
                RuntimeResult::WorkspaceSelection(
                    self.request_select_workspace(workspace_id, window_order),
                )
            }
            RuntimeCommand::RequestSelectNextWorkspace { window_order } => {
                RuntimeResult::WorkspaceSelection(self.request_select_next_workspace(window_order))
            }
            RuntimeCommand::RequestSelectPreviousWorkspace { window_order } => {
                RuntimeResult::WorkspaceSelection(
                    self.request_select_previous_workspace(window_order),
                )
            }
            RuntimeCommand::EnsureSeat { seat_id } => {
                RuntimeResult::Seat(self.ensure_seat(seat_id))
            }
            RuntimeCommand::SyncOutput { output_id, name, logical_width, logical_height } => {
                RuntimeResult::Output(self.sync_output(
                    output_id,
                    name,
                    logical_width,
                    logical_height,
                ))
            }
            RuntimeCommand::PlaceNewWindow { window_id } => {
                RuntimeResult::Window(Some(self.place_new_window(window_id)))
            }
            RuntimeCommand::RequestFocusWindowSelection { seat_id, window_id } => {
                RuntimeResult::FocusSelection(
                    self.request_focus_window_selection(seat_id, window_id),
                )
            }
            RuntimeCommand::RequestFocusNextWindowSelection { seat_id, window_order } => {
                RuntimeResult::FocusSelection(
                    self.request_focus_next_window_selection(seat_id, window_order),
                )
            }
            RuntimeCommand::RequestFocusPreviousWindowSelection { seat_id, window_order } => {
                RuntimeResult::FocusSelection(
                    self.request_focus_previous_window_selection(seat_id, window_order),
                )
            }
            RuntimeCommand::SyncHoveredWindow { seat_id, hovered_window_id } => {
                RuntimeResult::Window(self.sync_hovered_window(seat_id, hovered_window_id))
            }
            RuntimeCommand::SyncInteractedWindow { seat_id, interacted_window_id } => {
                RuntimeResult::Window(self.sync_interacted_window(seat_id, interacted_window_id))
            }
            RuntimeCommand::UnmapWindow { window_id, window_order } => {
                RuntimeResult::FocusUpdate(self.unmap_window(window_id, window_order))
            }
            RuntimeCommand::RemoveWindow { window_id, window_order } => {
                RuntimeResult::FocusUpdate(self.remove_window(window_id, window_order))
            }
            RuntimeCommand::RequestCloseFocusedWindowSelection => {
                RuntimeResult::CloseSelection(self.request_close_focused_window_selection())
            }
            RuntimeCommand::AssignFocusedWindowToWorkspace { workspace_id, window_order } => {
                RuntimeResult::FocusSelection(
                    self.assign_focused_window_to_workspace(workspace_id, window_order),
                )
            }
            RuntimeCommand::ToggleAssignFocusedWindowToWorkspace { workspace_id, window_order } => {
                RuntimeResult::FocusSelection(
                    self.toggle_assign_focused_window_to_workspace(workspace_id, window_order),
                )
            }
            RuntimeCommand::ToggleFocusedWindowFloating => {
                RuntimeResult::Window(self.toggle_focused_window_floating())
            }
            RuntimeCommand::ToggleFocusedWindowFullscreen => {
                RuntimeResult::Window(self.toggle_focused_window_fullscreen())
            }
            RuntimeCommand::SyncWindowIdentity { window_id, title, app_id } => {
                RuntimeResult::Window(self.sync_window_identity(window_id, title, app_id))
            }
            RuntimeCommand::SyncWindowMapped { window_id, mapped } => {
                RuntimeResult::Window(self.sync_window_mapped(window_id, mapped))
            }
        }
    }

    pub fn ensure_default_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        ensure_default_workspace(self.model, name)
    }

    pub fn ensure_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        ensure_workspace(self.model, name)
    }

    pub fn request_select_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        request_select_workspace(self.model, workspace_id, window_order)
    }

    pub fn request_select_next_workspace<I>(
        &mut self,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        request_select_next_workspace(self.model, window_order)
    }

    pub fn request_select_previous_workspace<I>(
        &mut self,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        request_select_previous_workspace(self.model, window_order)
    }

    pub fn ensure_seat(&mut self, seat_id: impl Into<SeatId>) -> SeatId {
        ensure_seat(self.model, seat_id)
    }

    pub fn sync_output(
        &mut self,
        output_id: impl Into<OutputId>,
        name: impl Into<String>,
        logical_width: u32,
        logical_height: u32,
    ) -> OutputId {
        sync_output(self.model, output_id, name, logical_width, logical_height)
    }

    pub fn place_new_window(&mut self, window_id: WindowId) -> WindowId {
        place_new_window(self.model, window_id)
    }

    pub fn request_focus_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_id: Option<WindowId>,
    ) -> FocusSelection {
        let selection = request_focus_window(self.model, window_id);
        let focused_window_id =
            sync_focused_window(self.model, seat_id, selection.focused_window_id);
        FocusSelection { focused_window_id }
    }

    pub fn request_focus_next_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> FocusSelection {
        let selection = request_focus_next_window(self.model, window_order);
        let focused_window_id =
            sync_focused_window(self.model, seat_id, selection.focused_window_id);
        FocusSelection { focused_window_id }
    }

    pub fn request_focus_previous_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> FocusSelection {
        let selection = request_focus_previous_window(self.model, window_order);
        let focused_window_id =
            sync_focused_window(self.model, seat_id, selection.focused_window_id);
        FocusSelection { focused_window_id }
    }

    pub fn sync_hovered_window(
        &mut self,
        seat_id: impl Into<SeatId>,
        hovered_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        sync_hovered_window(self.model, seat_id, hovered_window_id)
    }

    pub fn sync_interacted_window(
        &mut self,
        seat_id: impl Into<SeatId>,
        interacted_window_id: Option<WindowId>,
    ) -> Option<WindowId> {
        sync_interacted_window(self.model, seat_id, interacted_window_id)
    }

    pub fn remove_window(
        &mut self,
        window_id: WindowId,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> FocusUpdate {
        remove_window(self.model, window_id, window_order)
    }

    pub fn unmap_window(
        &mut self,
        window_id: WindowId,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> FocusUpdate {
        unmap_window(self.model, window_id, window_order)
    }

    pub fn request_close_focused_window_selection(&mut self) -> CloseSelection {
        request_close_focused_window(self.model)
    }

    pub fn assign_focused_window_to_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> FocusSelection
    where
        I: IntoIterator<Item = WindowId>,
    {
        FocusSelection {
            focused_window_id: assign_focused_window_to_workspace(
                self.model,
                workspace_id,
                window_order,
            ),
        }
    }

    pub fn toggle_assign_focused_window_to_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> FocusSelection
    where
        I: IntoIterator<Item = WindowId>,
    {
        FocusSelection {
            focused_window_id: toggle_assign_focused_window_to_workspace(
                self.model,
                workspace_id,
                window_order,
            ),
        }
    }

    pub fn toggle_focused_window_floating(&mut self) -> Option<WindowId> {
        toggle_focused_window_floating(self.model)
    }

    pub fn toggle_focused_window_fullscreen(&mut self) -> Option<WindowId> {
        toggle_focused_window_fullscreen(self.model)
    }

    pub fn sync_window_identity(
        &mut self,
        window_id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
    ) -> Option<WindowId> {
        sync_window_identity(self.model, window_id, title, app_id)
    }

    pub fn sync_window_mapped(&mut self, window_id: WindowId, mapped: bool) -> Option<WindowId> {
        if !self.model.windows.contains_key(&window_id) {
            return None;
        }

        self.model.set_window_mapped(window_id.clone(), mapped);
        Some(window_id)
    }
}

fn sync_output(
    model: &mut WmModel,
    output_id: impl Into<OutputId>,
    name: impl Into<String>,
    logical_width: u32,
    logical_height: u32,
) -> OutputId {
    let output_id = output_id.into();
    let name = name.into();
    let focused_workspace_id = model
        .outputs
        .get(&output_id)
        .and_then(|output| output.focused_workspace_id.clone())
        .or_else(|| model.current_workspace_id.clone());

    model.upsert_output(
        output_id.clone(),
        name,
        logical_width,
        logical_height,
        focused_workspace_id,
    );

    if let Some(workspace_id) = model.current_workspace_id.clone() {
        model.attach_workspace_to_output(workspace_id, output_id.clone());
    }

    if model.current_output_id.is_none() {
        model.set_current_output(output_id.clone());
    }

    output_id
}

fn ensure_seat(model: &mut WmModel, seat_id: impl Into<SeatId>) -> SeatId {
    let seat_id = seat_id.into();
    model.upsert_seat(seat_id.clone());
    seat_id
}

fn sync_focused_window(
    model: &mut WmModel,
    seat_id: impl Into<SeatId>,
    focused_window_id: Option<WindowId>,
) -> Option<WindowId> {
    let seat_id = ensure_seat(model, seat_id);
    let focused_window_id =
        focused_window_id.filter(|window_id| model.windows.contains_key(window_id));
    model.set_seat_focused_window(seat_id, focused_window_id.clone());
    focused_window_id
}

fn sync_hovered_window(
    model: &mut WmModel,
    seat_id: impl Into<SeatId>,
    hovered_window_id: Option<WindowId>,
) -> Option<WindowId> {
    let seat_id = ensure_seat(model, seat_id);
    let hovered_window_id =
        hovered_window_id.filter(|window_id| model.windows.contains_key(window_id));
    model.set_seat_hovered_window(seat_id, hovered_window_id.clone());
    hovered_window_id
}

fn sync_interacted_window(
    model: &mut WmModel,
    seat_id: impl Into<SeatId>,
    interacted_window_id: Option<WindowId>,
) -> Option<WindowId> {
    let seat_id = ensure_seat(model, seat_id);
    let interacted_window_id =
        interacted_window_id.filter(|window_id| model.windows.contains_key(window_id));
    model.set_seat_interacted_window(seat_id, interacted_window_id.clone());
    interacted_window_id
}

fn request_close_focused_window(model: &mut WmModel) -> CloseSelection {
    let focused_id =
        model.focused_window_id.clone().filter(|window_id| model.windows.contains_key(window_id));

    if focused_id != model.focused_window_id {
        model.set_window_focused(None);
    }

    if let Some(window_id) = focused_id.as_ref() {
        model.set_window_closing(window_id.clone(), true);
    }

    CloseSelection { closing_window_id: focused_id }
}

fn sync_window_identity(
    model: &mut WmModel,
    window_id: WindowId,
    title: Option<String>,
    app_id: Option<String>,
) -> Option<WindowId> {
    if !model.windows.contains_key(&window_id) {
        return None;
    }

    model.set_window_identity(window_id.clone(), title, app_id);
    Some(window_id)
}

fn assign_focused_window_to_workspace<I>(
    model: &mut WmModel,
    workspace_id: WorkspaceId,
    window_order: I,
) -> Option<WindowId>
where
    I: IntoIterator<Item = WindowId>,
{
    let focused_window_id =
        model.focused_window_id.clone().filter(|window_id| model.windows.contains_key(window_id));
    let Some(focused_window_id) = focused_window_id else {
        return model.focused_window_id.clone();
    };

    model.set_window_workspace(focused_window_id.clone(), Some(workspace_id.clone()));

    let next_focused_window_id = if model.current_workspace_id.as_ref() == Some(&workspace_id) {
        Some(focused_window_id)
    } else {
        model.preferred_focus_window_on_current_workspace(window_order)
    };
    model.set_window_focused(next_focused_window_id.clone());
    next_focused_window_id
}

fn toggle_assign_focused_window_to_workspace<I>(
    model: &mut WmModel,
    workspace_id: WorkspaceId,
    window_order: I,
) -> Option<WindowId>
where
    I: IntoIterator<Item = WindowId>,
{
    assign_focused_window_to_workspace(model, workspace_id, window_order)
}

fn toggle_focused_window_floating(model: &mut WmModel) -> Option<WindowId> {
    let focused_window_id =
        model.focused_window_id.clone().filter(|window_id| model.windows.contains_key(window_id));
    let Some(focused_window_id) = focused_window_id else {
        return None;
    };

    let next_floating =
        model.windows.get(&focused_window_id).map(|window| !window.floating).unwrap_or(false);
    model.set_window_floating(focused_window_id.clone(), next_floating);
    Some(focused_window_id)
}

fn toggle_focused_window_fullscreen(model: &mut WmModel) -> Option<WindowId> {
    let focused_window_id =
        model.focused_window_id.clone().filter(|window_id| model.windows.contains_key(window_id));
    let Some(focused_window_id) = focused_window_id else {
        return None;
    };

    let next_fullscreen =
        model.windows.get(&focused_window_id).map(|window| !window.fullscreen).unwrap_or(false);

    let window_ids = model.windows.keys().cloned().collect::<Vec<_>>();
    for window_id in window_ids {
        model.set_window_fullscreen(window_id, false);
    }
    model.set_window_fullscreen(focused_window_id.clone(), next_fullscreen);

    Some(focused_window_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::window_id;

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

        let workspace =
            runtime.execute(RuntimeCommand::EnsureDefaultWorkspace { name: "1".to_string() });
        let ensured_workspace =
            runtime.execute(RuntimeCommand::EnsureWorkspace { name: "2".to_string() });
        let selected_workspace = runtime.execute(RuntimeCommand::RequestSelectWorkspace {
            workspace_id: WorkspaceId("1".to_string()),
            window_order: Vec::new(),
        });
        let next_workspace = runtime
            .execute(RuntimeCommand::RequestSelectNextWorkspace { window_order: Vec::new() });
        let previous_workspace = runtime
            .execute(RuntimeCommand::RequestSelectPreviousWorkspace { window_order: Vec::new() });
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
        let seat =
            runtime.execute(RuntimeCommand::EnsureSeat { seat_id: SeatId("winit".to_string()) });

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
            RuntimeResult::FocusSelection(FocusSelection { focused_window_id: None })
        );
        assert_eq!(
            close_selection,
            RuntimeResult::CloseSelection(CloseSelection { closing_window_id: None })
        );
        assert_eq!(
            assigned_workspace,
            RuntimeResult::FocusSelection(FocusSelection { focused_window_id: None })
        );
        assert_eq!(seat, RuntimeResult::Seat(SeatId("winit".to_string())));
    }
}
