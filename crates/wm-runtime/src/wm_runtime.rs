use spiders_core::command::WmCommand;
use spiders_core::command::FocusDirection;
use spiders_core::event::WmEvent;
use spiders_core::focus::{
    FocusSelection, FocusTree, FocusTreeWindowGeometry, FocusUpdate, remove_window, request_focus_next_window,
    request_focus_previous_window, request_focus_window, unmap_window,
};
use spiders_core::navigation::{NavigationDirection, WindowGeometryCandidate, select_directional_focus_candidate};
use spiders_core::query::{
    QueryRequest, QueryResponse, output_snapshot, query_response_for_model, window_snapshot,
    workspace_snapshot,
};
use spiders_core::signal::WmSignal;
use spiders_core::types::LayoutRef;
use spiders_core::wm::WmModel;
use spiders_core::workspace::{
    WorkspaceSelection, ensure_default_workspace, ensure_workspace, place_new_window,
    request_select_next_workspace, request_select_previous_workspace, request_select_workspace,
};
use spiders_core::{LayoutRect, OutputId, SeatId, WindowId, WorkspaceId};
use tracing::info;

use crate::host::{WmHost, dispatch_wm_command};
use spiders_config::model::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseSelection {
    pub closing_window_id: Option<WindowId>,
}

pub struct WmRuntime<'a> {
    model: &'a mut WmModel,
    pending_events: Vec<WmEvent>,
}

impl<'a> WmRuntime<'a> {
    pub fn new(model: &'a mut WmModel) -> Self {
        Self { model, pending_events: Vec::new() }
    }

    pub fn dispatch_command<H: WmHost>(
        &mut self,
        host: &mut H,
        command: WmCommand,
    ) -> Vec<WmEvent> {
        dispatch_wm_command(host, command);
        self.take_events()
    }

    pub fn handle_signal<H: WmHost>(&mut self, _host: &mut H, signal: WmSignal) -> Vec<WmEvent> {
        match signal {
            WmSignal::EnsureSeat { seat_id } => {
                self.ensure_seat(seat_id);
            }
            WmSignal::OutputSynced { output_id, name, logical_width, logical_height } => {
                self.sync_output(output_id, name, logical_width, logical_height);
            }
            WmSignal::OutputRemoved { output_id } => {
                self.remove_output(output_id);
            }
            WmSignal::HoveredWindowChanged { seat_id, hovered_window_id } => {
                self.sync_hovered_window(seat_id, hovered_window_id);
            }
            WmSignal::InteractedWindowChanged { seat_id, interacted_window_id } => {
                self.sync_interacted_window(seat_id, interacted_window_id);
            }
            WmSignal::WindowIdentityChanged { window_id, title, app_id, class, instance } => {
                self.sync_window_identity(window_id, title, app_id, class, instance);
            }
            WmSignal::WindowMappedChanged { window_id, mapped } => {
                self.sync_window_mapped(window_id, mapped);
            }
        }

        self.take_events()
    }

    pub fn take_events(&mut self) -> Vec<WmEvent> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn query(&self, request: QueryRequest) -> QueryResponse {
        query_response_for_model(self.model, request)
    }

    pub fn model(&self) -> &WmModel {
        self.model
    }

    pub fn ensure_default_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        ensure_default_workspace(self.model, name)
    }

    pub fn ensure_workspace(&mut self, name: impl Into<String>) -> WorkspaceId {
        ensure_workspace(self.model, name)
    }

    pub fn sync_layout_selection_defaults(&mut self, config: &Config) {
        let workspace_names = self.model.workspace_names();
        let workspace_ids = self.model.workspaces.keys().cloned().collect::<Vec<_>>();

        for workspace_id in workspace_ids {
            let current_layout = self
                .model
                .workspaces
                .get(&workspace_id)
                .and_then(|workspace| workspace.effective_layout.clone());
            let next_layout =
                workspace_default_layout_name(self.model, config, &workspace_names, &workspace_id)
                    .map(|name| LayoutRef { name });
            if current_layout != next_layout {
                self.model
                    .set_workspace_effective_layout(workspace_id.clone(), next_layout.clone());
                self.push_layout_change(Some(workspace_id), next_layout);
            }
        }
    }

    pub fn set_current_workspace_layout(&mut self, name: impl Into<String>) -> Option<LayoutRef> {
        let workspace_id = self.model.current_workspace_id().cloned()?;
        let name = name.into();
        let layout = LayoutRef { name };
        self.model.set_workspace_effective_layout(workspace_id.clone(), Some(layout.clone()));
        self.push_layout_change(Some(workspace_id), Some(layout.clone()));
        Some(layout)
    }

    pub fn cycle_current_workspace_layout(
        &mut self,
        config: &Config,
        direction: Option<spiders_core::command::LayoutCycleDirection>,
    ) -> Option<LayoutRef> {
        let workspace_id = self.model.current_workspace_id().cloned()?;
        let layouts = config.layouts.iter().map(|layout| layout.name.as_str()).collect::<Vec<_>>();
        if layouts.is_empty() {
            return None;
        }

        let current_name = self
            .model
            .workspaces
            .get(&workspace_id)
            .and_then(|workspace| workspace.effective_layout.as_ref())
            .map(|layout| layout.name.as_str());
        let current_index =
            current_name.and_then(|name| layouts.iter().position(|candidate| *candidate == name));

        let next_index = match (direction, current_index) {
            (Some(spiders_core::command::LayoutCycleDirection::Previous), Some(index)) => {
                (index + layouts.len() - 1) % layouts.len()
            }
            (Some(spiders_core::command::LayoutCycleDirection::Previous), None) => {
                layouts.len() - 1
            }
            (_, Some(index)) => (index + 1) % layouts.len(),
            (_, None) => 0,
        };

        let layout = LayoutRef { name: layouts[next_index].to_string() };
        self.model.set_workspace_effective_layout(workspace_id.clone(), Some(layout.clone()));
        self.push_layout_change(Some(workspace_id), Some(layout.clone()));
        Some(layout)
    }

    pub fn request_select_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        let selection = request_select_workspace(self.model, workspace_id, window_order);
        if selection.is_some() {
            self.push_workspace_change();
            self.push_focus_change();
        }
        selection
    }

    pub fn request_select_next_workspace<I>(
        &mut self,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        let selection = request_select_next_workspace(self.model, window_order);
        if selection.is_some() {
            self.push_workspace_change();
            self.push_focus_change();
        }
        selection
    }

    pub fn request_select_previous_workspace<I>(
        &mut self,
        window_order: I,
    ) -> Option<WorkspaceSelection>
    where
        I: IntoIterator<Item = WindowId>,
    {
        let selection = request_select_previous_workspace(self.model, window_order);
        if selection.is_some() {
            self.push_workspace_change();
            self.push_focus_change();
        }
        selection
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
        let output_id = sync_output(self.model, output_id, name, logical_width, logical_height);
        self.push_output_change(&output_id);
        output_id
    }

    pub fn remove_output(&mut self, output_id: impl Into<OutputId>) -> OutputId {
        let output_id = output_id.into();
        self.model.remove_output(&output_id);
        self.push_output_change(&output_id);
        output_id
    }

    pub fn place_new_window(&mut self, window_id: WindowId) -> WindowId {
        let window_id = place_new_window(self.model, window_id);
        self.push_window_created(&window_id);
        window_id
    }

    pub fn request_focus_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        window_id: Option<WindowId>,
    ) -> FocusSelection {
        let selection = request_focus_window(self.model, window_id);
        let focused_window_id =
            sync_focused_window(self.model, seat_id, selection.focused_window_id);
        self.push_focus_change();
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
        self.push_focus_change();
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
        self.push_focus_change();
        FocusSelection { focused_window_id }
    }

    pub fn request_focus_direction_window_selection(
        &mut self,
        seat_id: impl Into<SeatId>,
        direction: FocusDirection,
        geometries: impl IntoIterator<Item = (WindowId, spiders_core::wm::WindowGeometry)>,
    ) -> FocusSelection {
        let candidates = directional_focus_candidates(self.model, geometries);
        info!(
            ?direction,
            candidate_count = candidates.len(),
            current_focus = ?self.model.focused_window_id(),
            focus_tree_present = self.model.focus_tree.is_some(),
            "wm-runtime focus direction selection start"
        );
        let focused_window_id = select_directional_focus_candidate(
            &candidates,
            self.model.focused_window_id().cloned(),
            navigation_direction(direction),
            &self.model.last_focused_window_id_by_scope,
            self.model.focus_tree.as_ref(),
        );
        info!(?direction, ?focused_window_id, "wm-runtime focus direction selection result");
        let selection = request_focus_window(self.model, focused_window_id);
        let focused_window_id = sync_focused_window(self.model, seat_id, selection.focused_window_id);
        self.push_focus_change();
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
        let focus_update = remove_window(self.model, window_id.clone(), window_order);
        self.push_window_destroyed(window_id);
        if matches!(focus_update, FocusUpdate::Set(_)) {
            self.push_focus_change();
        }
        focus_update
    }

    pub fn unmap_window(
        &mut self,
        window_id: WindowId,
        window_order: impl IntoIterator<Item = WindowId>,
    ) -> FocusUpdate {
        let focus_update = unmap_window(self.model, window_id, window_order);
        if matches!(focus_update, FocusUpdate::Set(_)) {
            self.push_focus_change();
        }
        focus_update
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
        let focused_window_id =
            assign_focused_window_to_workspace(self.model, workspace_id, window_order);
        if let Some(window_id) = focused_window_id.as_ref() {
            self.push_window_workspace_change(window_id);
        }
        self.push_focus_change();
        FocusSelection { focused_window_id }
    }

    pub fn toggle_assign_focused_window_to_workspace<I>(
        &mut self,
        workspace_id: WorkspaceId,
        window_order: I,
    ) -> FocusSelection
    where
        I: IntoIterator<Item = WindowId>,
    {
        let focused_window_id =
            toggle_assign_focused_window_to_workspace(self.model, workspace_id, window_order);
        if let Some(window_id) = focused_window_id.as_ref() {
            self.push_window_workspace_change(window_id);
        }
        self.push_focus_change();
        FocusSelection { focused_window_id }
    }

    pub fn toggle_focused_window_floating(&mut self) -> Option<WindowId> {
        let window_id = toggle_focused_window_floating(self.model);
        if let Some(window_id) = window_id.as_ref() {
            self.push_window_floating_change(window_id);
        }
        window_id
    }

    pub fn toggle_focused_window_fullscreen(&mut self) -> Option<WindowId> {
        let window_id = toggle_focused_window_fullscreen(self.model);
        if let Some(window_id) = window_id.as_ref() {
            self.push_window_fullscreen_change(window_id);
        }
        window_id
    }

    pub fn sync_window_identity(
        &mut self,
        window_id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
        class: Option<String>,
        instance: Option<String>,
    ) -> Option<WindowId> {
        let window_id = sync_window_identity(self.model, window_id, title, app_id, class, instance);
        if let Some(window_id) = window_id.as_ref() {
            self.push_window_identity_change(window_id);
        }
        window_id
    }

    pub fn sync_window_mapped(&mut self, window_id: WindowId, mapped: bool) -> Option<WindowId> {
        if !self.model.windows.contains_key(&window_id) {
            return None;
        }

        self.model.set_window_mapped(window_id.clone(), mapped);
        self.push_window_mapped_change(&window_id);
        Some(window_id)
    }

    pub fn set_window_floating_geometry(
        &mut self,
        window_id: WindowId,
        geometry: spiders_core::wm::WindowGeometry,
    ) -> Option<WindowId> {
        if !self.model.windows.contains_key(&window_id) {
            return None;
        }

        self.model.set_window_floating_geometry(window_id.clone(), geometry);
        self.push_window_geometry_change(&window_id);
        Some(window_id)
    }

    fn push_focus_change(&mut self) {
        self.pending_events.push(WmEvent::FocusChange {
            focused_window_id: self.model.focused_window_id().cloned(),
            current_output_id: self.model.current_output_id().cloned(),
            current_workspace_id: self.model.current_workspace_id().cloned(),
        });
    }

    fn push_workspace_change(&mut self) {
        let workspace_id = self.model.current_workspace_id().cloned();
        let active_workspaces = workspace_id
            .as_ref()
            .and_then(|workspace_id| self.model.workspaces.get(workspace_id))
            .map(|workspace| workspace_snapshot(self.model, workspace).active_workspaces)
            .unwrap_or_default();

        self.pending_events.push(WmEvent::WorkspaceChange { workspace_id, active_workspaces });
    }

    fn push_window_created(&mut self, window_id: &WindowId) {
        let Some(window) = self.model.windows.get(window_id) else {
            return;
        };

        self.pending_events
            .push(WmEvent::WindowCreated { window: window_snapshot(self.model, window) });
    }

    fn push_window_destroyed(&mut self, window_id: WindowId) {
        self.pending_events.push(WmEvent::WindowDestroyed { window_id });
    }

    fn push_window_workspace_change(&mut self, window_id: &WindowId) {
        self.pending_events.push(WmEvent::WindowWorkspaceChange {
            window_id: window_id.clone(),
            workspaces: self.model.workspace_names_for_window(window_id),
        });
    }

    fn push_window_floating_change(&mut self, window_id: &WindowId) {
        let Some(window) = self.model.windows.get(window_id) else {
            return;
        };

        self.pending_events.push(WmEvent::WindowFloatingChange {
            window_id: window_id.clone(),
            floating: window.floating,
        });
    }

    fn push_window_identity_change(&mut self, window_id: &WindowId) {
        let Some(window) = self.model.windows.get(window_id) else {
            return;
        };

        self.pending_events
            .push(WmEvent::WindowIdentityChange { window: window_snapshot(self.model, window) });
    }

    fn push_window_mapped_change(&mut self, window_id: &WindowId) {
        let Some(window) = self.model.windows.get(window_id) else {
            return;
        };

        self.pending_events.push(WmEvent::WindowMappedChange {
            window_id: window_id.clone(),
            mapped: window.mapped,
        });
    }

    fn push_window_fullscreen_change(&mut self, window_id: &WindowId) {
        let Some(window) = self.model.windows.get(window_id) else {
            return;
        };

        self.pending_events.push(WmEvent::WindowFullscreenChange {
            window_id: window_id.clone(),
            fullscreen: window.fullscreen,
        });
    }

    fn push_window_geometry_change(&mut self, window_id: &WindowId) {
        let Some(window) = self.model.windows.get(window_id) else {
            return;
        };

        self.pending_events.push(WmEvent::WindowGeometryChange {
            window_id: window_id.clone(),
            floating_rect: window.floating_geometry.map(layout_rect),
            output_id: window.output_id.clone(),
            workspace_id: window.workspace_id.clone(),
        });
    }

    fn push_layout_change(&mut self, workspace_id: Option<WorkspaceId>, layout: Option<LayoutRef>) {
        self.pending_events.push(WmEvent::LayoutChange { workspace_id, layout });
    }

    fn push_output_change(&mut self, output_id: &OutputId) {
        let Some(output) = self.model.outputs.get(output_id) else {
            return;
        };

        self.pending_events.push(WmEvent::OutputChange { output: output_snapshot(output) });
    }
}

fn layout_rect(geometry: spiders_core::wm::WindowGeometry) -> LayoutRect {
    LayoutRect {
        x: geometry.x as f32,
        y: geometry.y as f32,
        width: geometry.width as f32,
        height: geometry.height as f32,
    }
}

fn directional_focus_candidates(
    model: &WmModel,
    geometries: impl IntoIterator<Item = (WindowId, spiders_core::wm::WindowGeometry)>,
) -> Vec<WindowGeometryCandidate> {
    let fallback_focus_tree_entries = geometries
        .into_iter()
        .map(|(window_id, geometry)| FocusTreeWindowGeometry { window_id, geometry })
        .collect::<Vec<_>>();
    let fallback_focus_tree = FocusTree::from_window_geometries(&fallback_focus_tree_entries);
    let focus_tree = model.focus_tree.as_ref().unwrap_or(&fallback_focus_tree);

    fallback_focus_tree_entries
        .into_iter()
        .map(|entry| WindowGeometryCandidate {
            scope_path: focus_tree
                .scope_path(&entry.window_id)
                .map(|scope_path| scope_path.to_vec())
                .unwrap_or_else(|| vec![FocusTree::workspace_scope()]),
            window_id: entry.window_id,
            geometry: entry.geometry,
        })
        .collect()
}

fn navigation_direction(direction: FocusDirection) -> NavigationDirection {
    match direction {
        FocusDirection::Left => NavigationDirection::Left,
        FocusDirection::Right => NavigationDirection::Right,
        FocusDirection::Up => NavigationDirection::Up,
        FocusDirection::Down => NavigationDirection::Down,
    }
}

fn workspace_default_layout_name(
    model: &WmModel,
    config: &Config,
    workspace_names: &[String],
    workspace_id: &WorkspaceId,
) -> Option<String> {
    let workspace = model.workspaces.get(workspace_id)?;

    if let Some(output_id) = workspace.output_id.as_ref()
        && let Some(output) = model.outputs.get(output_id)
        && let Some(layout_name) = config.layout_selection.per_monitor.get(&output.name)
    {
        return Some(layout_name.clone());
    }

    if let Some(index) = workspace_names.iter().position(|name| name == &workspace.name)
        && let Some(layout_name) = config.layout_selection.per_workspace.get(index)
    {
        return Some(layout_name.clone());
    }

    config.layout_selection.default.clone()
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
        let next_focus = model.preferred_focus_window_on_current_workspace(Vec::new());
        model.set_window_focused(next_focus);
    }

    CloseSelection { closing_window_id: focused_id }
}

fn sync_window_identity(
    model: &mut WmModel,
    window_id: WindowId,
    title: Option<String>,
    app_id: Option<String>,
    class: Option<String>,
    instance: Option<String>,
) -> Option<WindowId> {
    if !model.windows.contains_key(&window_id) {
        return None;
    }

    model.set_window_identity(window_id.clone(), title, app_id, class, instance);
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
    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_core::signal::WmSignal;
    use spiders_core::window_id;

    struct NoopHost;

    impl WmHost for NoopHost {
        fn on_effect(&mut self, _effect: spiders_core::effect::WmHostEffect) {}
    }

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
    fn runtime_dispatch_command_uses_host() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);
        let mut host = NoopHost;

        let events = runtime.dispatch_command(&mut host, WmCommand::ReloadConfig);

        assert!(events.is_empty());
    }

    #[test]
    fn selecting_workspace_emits_workspace_and_focus_events() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.ensure_workspace("1");
        runtime.ensure_workspace("2");

        let selection = runtime.request_select_workspace(WorkspaceId("2".to_string()), Vec::new());
        let events = runtime.take_events();

        assert_eq!(
            selection.map(|selection| selection.workspace_id),
            Some(WorkspaceId("2".to_string()))
        );
        assert_eq!(events.len(), 2);
        assert!(
            matches!(events[0], WmEvent::WorkspaceChange { ref workspace_id, .. } if *workspace_id == Some(WorkspaceId("2".to_string())))
        );
        assert!(
            matches!(events[1], WmEvent::FocusChange { ref current_workspace_id, .. } if *current_workspace_id == Some(WorkspaceId("2".to_string())))
        );
    }

    #[test]
    fn placing_window_emits_window_created_event() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);

        runtime.place_new_window(window_id(7));
        let events = runtime.take_events();

        assert!(
            matches!(events.as_slice(), [WmEvent::WindowCreated { window }] if window.id == window_id(7))
        );
    }

    #[test]
    fn setting_floating_geometry_emits_window_geometry_change() {
        let mut model = WmModel::default();
        model.insert_window(window_id(9), None, None);
        let mut runtime = WmRuntime::new(&mut model);

        let updated_window_id = runtime.set_window_floating_geometry(
            window_id(9),
            spiders_core::wm::WindowGeometry { x: 10, y: 20, width: 300, height: 400 },
        );
        let events = runtime.take_events();

        assert_eq!(updated_window_id, Some(window_id(9)));
        assert!(matches!(
            events.as_slice(),
            [WmEvent::WindowGeometryChange { window_id: event_window_id, floating_rect: Some(rect), .. }]
                if *event_window_id == window_id(9)
                    && *rect == LayoutRect { x: 10.0, y: 20.0, width: 300.0, height: 400.0 }
        ));
    }

    #[test]
    fn setting_current_workspace_layout_emits_layout_change() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);
        runtime.ensure_workspace("1");
        let _ = runtime.request_select_workspace(WorkspaceId("1".to_string()), Vec::new());
        let _ = runtime.take_events();

        let layout = runtime.set_current_workspace_layout("master-stack");
        let events = runtime.take_events();

        assert_eq!(layout, Some(LayoutRef { name: "master-stack".to_string() }));
        assert!(matches!(
            events.as_slice(),
            [WmEvent::LayoutChange { workspace_id, layout: Some(layout) }]
                if *workspace_id == Some(WorkspaceId("1".to_string()))
                    && *layout == LayoutRef { name: "master-stack".to_string() }
        ));
    }

    #[test]
    fn cycling_current_workspace_layout_emits_layout_change() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);
        runtime.ensure_workspace("1");
        let _ = runtime.request_select_workspace(WorkspaceId("1".to_string()), Vec::new());
        let _ = runtime.take_events();
        let config = Config {
            layouts: vec![
                LayoutDefinition {
                    name: "master-stack".to_string(),
                    directory: "layouts/master-stack".to_string(),
                    module: "layouts/master-stack.js".to_string(),
                    stylesheet_path: None,
                    runtime_cache_payload: None,
                },
                LayoutDefinition {
                    name: "columns".to_string(),
                    directory: "layouts/columns".to_string(),
                    module: "layouts/columns.js".to_string(),
                    stylesheet_path: None,
                    runtime_cache_payload: None,
                },
            ],
            ..Config::default()
        };

        let layout = runtime.cycle_current_workspace_layout(&config, None);
        let events = runtime.take_events();

        assert_eq!(layout, Some(LayoutRef { name: "master-stack".to_string() }));
        assert!(matches!(
            events.as_slice(),
            [WmEvent::LayoutChange { workspace_id, layout: Some(layout) }]
                if *workspace_id == Some(WorkspaceId("1".to_string()))
                    && *layout == LayoutRef { name: "master-stack".to_string() }
        ));
    }

    #[test]
    fn syncing_layout_defaults_emits_layout_change_only_for_changed_workspaces() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);
        runtime.ensure_default_workspace("1");
        let mut host = NoopHost;
        let _ = runtime.handle_signal(
            &mut host,
            WmSignal::OutputSynced {
                output_id: OutputId("winit".to_string()),
                name: "winit".to_string(),
                logical_width: 1280,
                logical_height: 720,
            },
        );
        let _ = runtime.take_events();

        let mut config = Config::default();
        config.layout_selection.default = Some("master-stack".to_string());
        config.layout_selection.per_monitor.insert("winit".to_string(), "focus-repro".to_string());

        runtime.sync_layout_selection_defaults(&config);
        let events = runtime.take_events();

        assert!(matches!(
            events.as_slice(),
            [WmEvent::LayoutChange { workspace_id, layout: Some(layout) }]
                if *workspace_id == Some(WorkspaceId("1".to_string()))
                    && *layout == LayoutRef { name: "focus-repro".to_string() }
        ));

        runtime.sync_layout_selection_defaults(&config);
        assert!(runtime.take_events().is_empty());
    }

    #[test]
    fn window_mapped_signal_emits_window_mapped_change_event() {
        let mut model = WmModel::default();
        model.insert_window(window_id(1), None, None);
        let mut runtime = WmRuntime::new(&mut model);
        let mut host = NoopHost;

        let events = runtime.handle_signal(
            &mut host,
            WmSignal::WindowMappedChanged { window_id: window_id(1), mapped: true },
        );

        assert!(matches!(
            events.as_slice(),
            [WmEvent::WindowMappedChange { window_id: event_window_id, mapped }]
                if *event_window_id == window_id(1) && *mapped
        ));
        assert_eq!(model.windows.get(&window_id(1)).map(|window| window.mapped), Some(true));
    }

    #[test]
    fn output_sync_signal_emits_output_change_event() {
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);
        let mut host = NoopHost;

        let events = runtime.handle_signal(
            &mut host,
            WmSignal::OutputSynced {
                output_id: OutputId("winit".to_string()),
                name: "winit".to_string(),
                logical_width: 1280,
                logical_height: 720,
            },
        );

        assert!(matches!(
            events.as_slice(),
            [WmEvent::OutputChange { output }]
                if output.id == OutputId("winit".to_string())
                    && output.logical_width == 1280
                    && output.logical_height == 720
        ));
        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
    }

    #[test]
    fn window_identity_signal_emits_window_identity_change_event() {
        let mut model = WmModel::default();
        model.insert_window(window_id(1), None, None);
        let mut runtime = WmRuntime::new(&mut model);
        let mut host = NoopHost;

        let events = runtime.handle_signal(
            &mut host,
            WmSignal::WindowIdentityChanged {
                window_id: window_id(1),
                title: Some("Terminal".to_string()),
                app_id: Some("foot".to_string()),
                class: Some("foot".to_string()),
                instance: Some("foot".to_string()),
            },
        );

        assert!(matches!(
            events.as_slice(),
            [WmEvent::WindowIdentityChange { window }]
                if window.id == window_id(1)
                    && window.title.as_deref() == Some("Terminal")
                    && window.app_id.as_deref() == Some("foot")
                    && window.class.as_deref() == Some("foot")
                    && window.instance.as_deref() == Some("foot")
        ));
    }
}
