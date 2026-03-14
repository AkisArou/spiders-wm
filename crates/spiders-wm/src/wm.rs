use spiders_shared::api::{CompositorEvent, FocusDirection};
use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
use spiders_shared::layout::LayoutRect;
use spiders_shared::wm::{
    LayoutRef, OutputSnapshot, StateSnapshot, WindowSnapshot, WorkspaceSnapshot,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WmStateError {
    #[error("no current output selected")]
    NoCurrentOutput,
    #[error("no current workspace selected")]
    NoCurrentWorkspace,
    #[error("no focused window")]
    NoFocusedWindow,
    #[error("output not found: {0}")]
    OutputNotFound(OutputId),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(WorkspaceId),
    #[error("window not found: {0}")]
    WindowNotFound(WindowId),
    #[error("tag '{tag}' not found on output {output_id}")]
    TagNotFound { output_id: OutputId, tag: String },
    #[error("workspace {workspace_id} cannot be assigned to missing output {output_id}")]
    InvalidWorkspaceAssignment {
        workspace_id: WorkspaceId,
        output_id: OutputId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct WmState {
    snapshot: StateSnapshot,
}

impl WmState {
    pub fn from_snapshot(snapshot: StateSnapshot) -> Self {
        let mut state = Self { snapshot };
        state.refresh_visible_windows();
        state
    }

    pub fn snapshot(&self) -> &StateSnapshot {
        &self.snapshot
    }

    pub fn current_output_id(&self) -> Result<&OutputId, WmStateError> {
        self.snapshot
            .current_output_id
            .as_ref()
            .ok_or(WmStateError::NoCurrentOutput)
    }

    pub fn current_workspace_id(&self) -> Result<&WorkspaceId, WmStateError> {
        self.snapshot
            .current_workspace_id
            .as_ref()
            .ok_or(WmStateError::NoCurrentWorkspace)
    }

    pub fn focused_window_id(&self) -> Result<&WindowId, WmStateError> {
        self.snapshot
            .focused_window_id
            .as_ref()
            .ok_or(WmStateError::NoFocusedWindow)
    }

    pub fn into_snapshot(self) -> StateSnapshot {
        self.snapshot
    }

    pub fn insert_output(&mut self, output: OutputSnapshot) {
        upsert_by_id(&mut self.snapshot.outputs, output, |entry| entry.id.clone());
    }

    pub fn insert_workspace(&mut self, workspace: WorkspaceSnapshot) {
        upsert_by_id(&mut self.snapshot.workspaces, workspace, |entry| {
            entry.id.clone()
        });
        self.refresh_visible_windows();
    }

    pub fn map_window(&mut self, mut window: WindowSnapshot) -> CompositorEvent {
        window.mapped = true;
        upsert_by_id(&mut self.snapshot.windows, window.clone(), |entry| {
            entry.id.clone()
        });
        self.refresh_visible_windows();

        CompositorEvent::WindowCreated { window }
    }

    pub fn destroy_window(
        &mut self,
        window_id: &WindowId,
    ) -> Result<Vec<CompositorEvent>, WmStateError> {
        let index = self
            .snapshot
            .windows
            .iter()
            .position(|window| &window.id == window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;
        let removed = self.snapshot.windows.remove(index);
        let was_focused = self.snapshot.focused_window_id.as_ref() == Some(window_id);

        if was_focused {
            self.snapshot.focused_window_id = None;
        }

        self.refresh_visible_windows();
        let mut events = vec![CompositorEvent::WindowDestroyed {
            window_id: removed.id.clone(),
        }];

        if was_focused {
            events.push(self.focus_change_event());
        }

        Ok(events)
    }

    pub fn focus_window(&mut self, window_id: &WindowId) -> Result<CompositorEvent, WmStateError> {
        let window = self
            .snapshot
            .windows
            .iter()
            .find(|window| &window.id == window_id)
            .cloned()
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;

        for entry in &mut self.snapshot.windows {
            entry.focused = entry.id == *window_id;
        }

        self.snapshot.focused_window_id = Some(window.id.clone());

        if let Some(workspace_id) = window.workspace_id.as_ref() {
            self.select_workspace(workspace_id)?;
        } else {
            self.refresh_visible_windows();
        }

        if let Some(output_id) = window.output_id.clone() {
            self.snapshot.current_output_id = Some(output_id);
        }

        Ok(self.focus_change_event())
    }

    pub fn view_tag_on_output(
        &mut self,
        output_id: &OutputId,
        tag: &str,
    ) -> Result<Vec<CompositorEvent>, WmStateError> {
        let workspace = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| {
                workspace.output_id.as_ref() == Some(output_id)
                    && workspace.active_tags.iter().any(|active| active == tag)
            })
            .cloned()
            .ok_or_else(|| WmStateError::TagNotFound {
                output_id: output_id.clone(),
                tag: tag.to_owned(),
            })?;

        self.select_workspace(&workspace.id)?;

        let mut events = vec![CompositorEvent::TagChange {
            workspace_id: Some(workspace.id.clone()),
            active_tags: workspace.active_tags.clone(),
        }];

        if self.snapshot.focused_window_id.is_none() {
            events.push(self.focus_change_event());
        }

        Ok(events)
    }

    pub fn set_layout_for_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
        layout_name: &str,
    ) -> Result<CompositorEvent, WmStateError> {
        let workspace = self
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| &workspace.id == workspace_id)
            .ok_or_else(|| WmStateError::WorkspaceNotFound(workspace_id.clone()))?;

        workspace.effective_layout = Some(LayoutRef {
            name: layout_name.to_owned(),
        });

        Ok(CompositorEvent::LayoutChange {
            workspace_id: Some(workspace.id.clone()),
            layout: workspace.effective_layout.clone(),
        })
    }

    pub fn current_workspace(&self) -> Result<&WorkspaceSnapshot, WmStateError> {
        self.current_workspace_id()
            .and_then(|workspace_id| self.workspace_by_id(workspace_id))
    }

    pub fn workspace_by_id(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<&WorkspaceSnapshot, WmStateError> {
        self.snapshot
            .workspace_by_id(workspace_id)
            .ok_or_else(|| WmStateError::WorkspaceNotFound(workspace_id.clone()))
    }

    pub fn toggle_tag_on_current_output(
        &mut self,
        tag: &str,
    ) -> Result<Vec<CompositorEvent>, WmStateError> {
        let output_id = self.current_output_id()?.clone();
        let current_workspace = self.current_workspace()?.clone();

        if current_workspace
            .active_tags
            .iter()
            .any(|active| active == tag)
        {
            return Ok(Vec::new());
        }

        let workspace = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| {
                workspace.output_id.as_ref() == Some(&output_id)
                    && workspace.active_tags.iter().any(|active| active == tag)
            })
            .cloned()
            .ok_or_else(|| WmStateError::TagNotFound {
                output_id: output_id.clone(),
                tag: tag.to_owned(),
            })?;

        self.select_workspace(&workspace.id)?;

        let mut events = vec![CompositorEvent::TagChange {
            workspace_id: Some(workspace.id.clone()),
            active_tags: workspace.active_tags.clone(),
        }];

        if self.snapshot.focused_window_id.is_none() {
            events.push(self.focus_change_event());
        }

        Ok(events)
    }

    pub fn activate_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<CompositorEvent>, WmStateError> {
        self.select_workspace(workspace_id)?;

        let workspace = self.workspace_by_id(workspace_id)?.clone();
        let mut events = vec![CompositorEvent::TagChange {
            workspace_id: Some(workspace.id.clone()),
            active_tags: workspace.active_tags.clone(),
        }];

        events.push(self.focus_change_event());
        Ok(events)
    }

    pub fn assign_workspace_to_output(
        &mut self,
        workspace_id: &WorkspaceId,
        output_id: &OutputId,
    ) -> Result<Vec<CompositorEvent>, WmStateError> {
        if !self
            .snapshot
            .outputs
            .iter()
            .any(|output| &output.id == output_id)
        {
            return Err(WmStateError::InvalidWorkspaceAssignment {
                workspace_id: workspace_id.clone(),
                output_id: output_id.clone(),
            });
        }

        let previous_output_id = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| &workspace.id == workspace_id)
            .and_then(|workspace| workspace.output_id.clone());

        let workspace = self
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| &workspace.id == workspace_id)
            .ok_or_else(|| WmStateError::WorkspaceNotFound(workspace_id.clone()))?;
        workspace.output_id = Some(output_id.clone());

        let should_activate = self.snapshot.current_workspace_id.as_ref() == Some(workspace_id)
            || self
                .snapshot
                .workspaces
                .iter()
                .find(|workspace| &workspace.id == workspace_id)
                .is_some_and(|workspace| workspace.visible);

        if should_activate {
            self.select_workspace(workspace_id)?;
        } else if let Some(output) = self
            .snapshot
            .outputs
            .iter_mut()
            .find(|output| &output.id == output_id)
        {
            if output.current_workspace_id.is_none() {
                output.current_workspace_id = Some(workspace_id.clone());
            }
        }

        if let Some(previous_output_id) = previous_output_id {
            self.ensure_output_has_visible_workspace(&previous_output_id);
        }

        self.refresh_visible_windows();
        self.reconcile_focus();

        let workspace = self.workspace_by_id(workspace_id)?.clone();
        let mut events = vec![CompositorEvent::TagChange {
            workspace_id: Some(workspace.id.clone()),
            active_tags: workspace.active_tags.clone(),
        }];

        if should_activate {
            events.push(self.focus_change_event());
        }

        Ok(events)
    }

    pub fn toggle_focused_floating(&mut self) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let window = self
            .snapshot
            .windows
            .iter_mut()
            .find(|window| window.id == window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;

        window.floating = !window.floating;
        if !window.floating {
            window.floating_rect = None;
        }

        Ok(CompositorEvent::WindowFloatingChange {
            window_id,
            floating: window.floating,
        })
    }

    pub fn set_floating_window_geometry(
        &mut self,
        window_id: &WindowId,
        rect: LayoutRect,
    ) -> Result<CompositorEvent, WmStateError> {
        let center = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        let target_output_id = self
            .snapshot
            .outputs
            .iter()
            .find(|output| output_rect_contains(output, center.0, center.1))
            .map(|output| output.id.clone());
        let target_workspace_id = target_output_id.as_ref().and_then(|output_id| {
            self.snapshot
                .workspaces
                .iter()
                .find(|workspace| {
                    workspace.output_id.as_ref() == Some(output_id) && workspace.visible
                })
                .or_else(|| {
                    self.snapshot
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.output_id.as_ref() == Some(output_id))
                })
                .map(|workspace| workspace.id.clone())
        });
        let window = self
            .snapshot
            .windows
            .iter_mut()
            .find(|window| &window.id == window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;

        window.floating_rect = Some(rect);
        if let Some(output_id) = target_output_id.clone() {
            window.output_id = Some(output_id.clone());
            if window.focused {
                self.snapshot.current_output_id = Some(output_id);
            }
        }
        if let Some(workspace_id) = target_workspace_id.clone() {
            window.workspace_id = Some(workspace_id.clone());
            if window.focused {
                self.select_workspace(&workspace_id)?;
            } else {
                self.refresh_visible_windows();
            }
        }

        Ok(CompositorEvent::WindowGeometryChange {
            window_id: window_id.clone(),
            floating_rect: Some(rect),
            output_id: target_output_id,
            workspace_id: target_workspace_id,
        })
    }

    pub fn toggle_focused_fullscreen(&mut self) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let window = self
            .snapshot
            .windows
            .iter_mut()
            .find(|window| window.id == window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;

        window.fullscreen = !window.fullscreen;

        Ok(CompositorEvent::WindowFullscreenChange {
            window_id,
            fullscreen: window.fullscreen,
        })
    }

    pub fn focus_direction(
        &mut self,
        direction: FocusDirection,
    ) -> Result<CompositorEvent, WmStateError> {
        let current_workspace_id = self.current_workspace_id()?.clone();
        let visible_windows: Vec<_> = self
            .snapshot
            .windows
            .iter()
            .filter(|window| {
                window.mapped
                    && window.workspace_id.as_ref() == Some(&current_workspace_id)
                    && self
                        .snapshot
                        .visible_window_ids
                        .iter()
                        .any(|id| id == &window.id)
            })
            .map(|window| window.id.clone())
            .collect();

        if visible_windows.is_empty() {
            return Err(WmStateError::NoFocusedWindow);
        }

        let current_index = self
            .snapshot
            .focused_window_id
            .as_ref()
            .and_then(|focused| {
                visible_windows
                    .iter()
                    .position(|window_id| window_id == focused)
            })
            .unwrap_or(0);

        let next_index = match direction {
            FocusDirection::Left | FocusDirection::Up => {
                (current_index + visible_windows.len() - 1) % visible_windows.len()
            }
            FocusDirection::Right | FocusDirection::Down => {
                (current_index + 1) % visible_windows.len()
            }
        };

        self.focus_window(&visible_windows[next_index])
    }

    pub fn swap_direction(&mut self, direction: FocusDirection) -> Result<(), WmStateError> {
        let current_workspace_id = self.current_workspace_id()?.clone();
        let visible_windows: Vec<_> = self
            .snapshot
            .windows
            .iter()
            .filter(|window| {
                window.mapped
                    && window.workspace_id.as_ref() == Some(&current_workspace_id)
                    && self
                        .snapshot
                        .visible_window_ids
                        .iter()
                        .any(|id| id == &window.id)
            })
            .map(|window| window.id.clone())
            .collect();

        if visible_windows.len() < 2 {
            return Ok(());
        }

        let current_index = self
            .snapshot
            .focused_window_id
            .as_ref()
            .and_then(|focused| {
                visible_windows
                    .iter()
                    .position(|window_id| window_id == focused)
            })
            .ok_or(WmStateError::NoFocusedWindow)?;

        let swap_index = match direction {
            FocusDirection::Left | FocusDirection::Up => {
                (current_index + visible_windows.len() - 1) % visible_windows.len()
            }
            FocusDirection::Right | FocusDirection::Down => {
                (current_index + 1) % visible_windows.len()
            }
        };

        let current_window_id = &visible_windows[current_index];
        let target_window_id = &visible_windows[swap_index];
        let current_slot = self
            .snapshot
            .windows
            .iter()
            .position(|window| &window.id == current_window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(current_window_id.clone()))?;
        let target_slot = self
            .snapshot
            .windows
            .iter()
            .position(|window| &window.id == target_window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(target_window_id.clone()))?;

        self.snapshot.windows.swap(current_slot, target_slot);
        Ok(())
    }

    pub fn resize_direction(
        &mut self,
        direction: FocusDirection,
    ) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let window = self
            .snapshot
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .cloned()
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;

        let Some(mut rect) = window.floating_rect else {
            return Ok(CompositorEvent::WindowGeometryChange {
                window_id,
                floating_rect: None,
                output_id: window.output_id,
                workspace_id: window.workspace_id,
            });
        };

        let delta = 32.0;
        match direction {
            FocusDirection::Left => rect.width -= delta,
            FocusDirection::Right => rect.width += delta,
            FocusDirection::Up => rect.height -= delta,
            FocusDirection::Down => rect.height += delta,
        }

        rect.width = rect.width.max(160.0);
        rect.height = rect.height.max(96.0);
        self.set_floating_window_geometry(&window_id, rect)
    }

    pub fn resize_tiled_direction(
        &mut self,
        _direction: FocusDirection,
    ) -> Result<(), WmStateError> {
        Ok(())
    }

    pub fn focus_monitor_left(&mut self) -> Result<CompositorEvent, WmStateError> {
        self.focus_monitor_relative(-1)
    }

    pub fn focus_monitor_right(&mut self) -> Result<CompositorEvent, WmStateError> {
        self.focus_monitor_relative(1)
    }

    pub fn send_monitor_left(&mut self) -> Result<CompositorEvent, WmStateError> {
        self.send_monitor_relative(-1)
    }

    pub fn send_monitor_right(&mut self) -> Result<CompositorEvent, WmStateError> {
        self.send_monitor_relative(1)
    }

    fn focus_monitor_relative(&mut self, step: isize) -> Result<CompositorEvent, WmStateError> {
        let target_workspace_id = self.target_workspace_for_relative_output(step)?;
        self.select_workspace(&target_workspace_id)?;
        Ok(self.focus_change_event())
    }

    fn send_monitor_relative(&mut self, step: isize) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let target_workspace_id = self.target_workspace_for_relative_output(step)?;
        let target_workspace = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.id == target_workspace_id)
            .cloned()
            .ok_or_else(|| WmStateError::WorkspaceNotFound(target_workspace_id.clone()))?;
        let target_output_id = target_workspace
            .output_id
            .clone()
            .ok_or_else(|| WmStateError::WorkspaceNotFound(target_workspace_id.clone()))?;
        let target_output = self
            .snapshot
            .outputs
            .iter()
            .find(|output| output.id == target_output_id)
            .cloned()
            .ok_or_else(|| WmStateError::OutputNotFound(target_output_id.clone()))?;
        let current_output_id = self.current_output_id()?.clone();
        let current_output = self
            .snapshot
            .outputs
            .iter()
            .find(|output| output.id == current_output_id)
            .cloned();

        let (floating_rect, focused) = {
            let window = self
                .snapshot
                .windows
                .iter_mut()
                .find(|window| window.id == window_id)
                .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;
            if let (Some(current_output), Some(rect)) =
                (current_output, window.floating_rect.as_mut())
            {
                rect.x += (target_output.logical_x - current_output.logical_x) as f32;
                rect.y += (target_output.logical_y - current_output.logical_y) as f32;
            }
            window.output_id = Some(target_output_id.clone());
            window.workspace_id = Some(target_workspace_id.clone());
            (window.floating_rect, window.focused)
        };
        if focused {
            self.select_workspace(&target_workspace_id)?;
        } else {
            self.refresh_visible_windows();
        }
        Ok(CompositorEvent::WindowGeometryChange {
            window_id,
            floating_rect,
            output_id: Some(target_output_id),
            workspace_id: Some(target_workspace_id),
        })
    }

    fn target_workspace_for_relative_output(
        &self,
        step: isize,
    ) -> Result<WorkspaceId, WmStateError> {
        let current_output_id = self.current_output_id()?.clone();
        let mut outputs = self.snapshot.outputs.clone();
        outputs.sort_by_key(|output| (output.logical_x, output.logical_y, output.name.clone()));

        let current_index = outputs
            .iter()
            .position(|output| output.id == current_output_id)
            .ok_or_else(|| WmStateError::OutputNotFound(current_output_id.clone()))?;
        let target_index =
            (current_index as isize + step).rem_euclid(outputs.len() as isize) as usize;
        let target_output_id = outputs[target_index].id.clone();
        let target_workspace_id = self
            .snapshot
            .outputs
            .iter()
            .find(|output| output.id == target_output_id)
            .and_then(|output| output.current_workspace_id.clone())
            .or_else(|| {
                self.snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.output_id.as_ref() == Some(&target_output_id))
                    .map(|workspace| workspace.id.clone())
            })
            .ok_or_else(|| WmStateError::OutputNotFound(target_output_id.clone()))?;

        Ok(target_workspace_id)
    }

    pub fn move_direction(
        &mut self,
        direction: FocusDirection,
    ) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let window = self
            .snapshot
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .cloned()
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;
        if let Some(mut rect) = window.floating_rect {
            let delta = 32.0;
            match direction {
                FocusDirection::Left => rect.x -= delta,
                FocusDirection::Right => rect.x += delta,
                FocusDirection::Up => rect.y -= delta,
                FocusDirection::Down => rect.y += delta,
            }
            self.set_floating_window_geometry(&window_id, rect)
        } else {
            self.swap_direction(direction)?;
            Ok(self.focus_change_event())
        }
    }

    pub fn tag_focused_window(&mut self, tag: &str) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let window = self
            .snapshot
            .windows
            .iter_mut()
            .find(|window| window.id == window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;
        window.tags = vec![tag.to_owned()];
        Ok(CompositorEvent::WindowTagChange {
            window_id,
            tags: window.tags.clone(),
        })
    }

    pub fn toggle_tag_focused_window(
        &mut self,
        tag: &str,
    ) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let window = self
            .snapshot
            .windows
            .iter_mut()
            .find(|window| window.id == window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;
        if let Some(index) = window.tags.iter().position(|candidate| candidate == tag) {
            window.tags.remove(index);
        } else {
            window.tags.push(tag.to_owned());
        }
        Ok(CompositorEvent::WindowTagChange {
            window_id,
            tags: window.tags.clone(),
        })
    }

    fn select_workspace(&mut self, workspace_id: &WorkspaceId) -> Result<(), WmStateError> {
        let workspace = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| &workspace.id == workspace_id)
            .cloned()
            .ok_or_else(|| WmStateError::WorkspaceNotFound(workspace_id.clone()))?;
        let output_id = workspace.output_id.clone();

        self.snapshot.current_workspace_id = Some(workspace.id.clone());
        if let Some(output_id) = output_id.clone() {
            self.snapshot.current_output_id = Some(output_id.clone());

            for output in &mut self.snapshot.outputs {
                if output.id == output_id {
                    output.current_workspace_id = Some(workspace.id.clone());
                }
            }
        }

        for entry in &mut self.snapshot.workspaces {
            if output_id.is_some() && entry.output_id == output_id {
                let selected = entry.id == workspace.id;
                entry.visible = selected;
                entry.focused = selected;
            } else if output_id.is_none() && entry.id == workspace.id {
                entry.visible = true;
                entry.focused = true;
            } else {
                entry.focused = false;
            }
        }

        self.refresh_visible_windows();
        self.reconcile_focus();
        Ok(())
    }

    fn refresh_visible_windows(&mut self) {
        let visible_workspace_ids: Vec<_> = self
            .snapshot
            .workspaces
            .iter()
            .filter(|workspace| workspace.visible)
            .map(|workspace| workspace.id.clone())
            .collect();

        self.snapshot.visible_window_ids =
            self.snapshot
                .windows
                .iter()
                .filter(|window| {
                    window.mapped
                        && window.workspace_id.as_ref().is_some_and(|workspace_id| {
                            visible_workspace_ids.contains(workspace_id)
                        })
                })
                .map(|window| window.id.clone())
                .collect();
    }

    fn reconcile_focus(&mut self) {
        let focused_visible = self
            .snapshot
            .focused_window_id
            .as_ref()
            .is_some_and(|window_id| {
                self.snapshot
                    .visible_window_ids
                    .iter()
                    .any(|visible| visible == window_id)
            });

        if !focused_visible {
            self.snapshot.focused_window_id = None;
            for window in &mut self.snapshot.windows {
                window.focused = false;
            }
        }
    }

    fn ensure_output_has_visible_workspace(&mut self, output_id: &OutputId) {
        let has_visible =
            self.snapshot.workspaces.iter().any(|workspace| {
                workspace.output_id.as_ref() == Some(output_id) && workspace.visible
            });

        if has_visible {
            if let Some(output) = self
                .snapshot
                .outputs
                .iter_mut()
                .find(|output| &output.id == output_id)
            {
                output.current_workspace_id = self
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| {
                        workspace.output_id.as_ref() == Some(output_id) && workspace.visible
                    })
                    .map(|workspace| workspace.id.clone());
            }
            return;
        }

        if let Some(workspace_id) = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.output_id.as_ref() == Some(output_id))
            .map(|workspace| workspace.id.clone())
        {
            for workspace in &mut self.snapshot.workspaces {
                if workspace.output_id.as_ref() == Some(output_id) {
                    workspace.visible = workspace.id == workspace_id;
                    if self.snapshot.current_workspace_id.as_ref() != Some(&workspace.id) {
                        workspace.focused = false;
                    }
                }
            }

            if let Some(output) = self
                .snapshot
                .outputs
                .iter_mut()
                .find(|output| &output.id == output_id)
            {
                output.current_workspace_id = Some(workspace_id);
            }
        } else if let Some(output) = self
            .snapshot
            .outputs
            .iter_mut()
            .find(|output| &output.id == output_id)
        {
            output.current_workspace_id = None;
        }
    }

    fn focus_change_event(&self) -> CompositorEvent {
        CompositorEvent::FocusChange {
            focused_window_id: self.snapshot.focused_window_id.clone(),
            current_output_id: self.snapshot.current_output_id.clone(),
            current_workspace_id: self.snapshot.current_workspace_id.clone(),
        }
    }
}

fn upsert_by_id<T, I, F>(entries: &mut Vec<T>, value: T, id: F)
where
    I: PartialEq,
    F: Fn(&T) -> I,
{
    if let Some(index) = entries.iter().position(|entry| id(entry) == id(&value)) {
        entries[index] = value;
    } else {
        entries.push(value);
    }
}

fn output_rect_contains(output: &OutputSnapshot, x: f32, y: f32) -> bool {
    let left = output.logical_x as f32;
    let top = output.logical_y as f32;
    let right = left + output.logical_width as f32;
    let bottom = top + output.logical_height as f32;
    x >= left && x < right && y >= top && y < bottom
}

#[cfg(test)]
mod tests {
    use spiders_shared::api::CompositorEvent;
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![
                WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-1"),
                    name: "1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["1".into()],
                    focused: true,
                    visible: true,
                    effective_layout: Some(LayoutRef {
                        name: "master-stack".into(),
                    }),
                },
                WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-2"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["2".into()],
                    focused: false,
                    visible: false,
                    effective_layout: Some(LayoutRef {
                        name: "stack".into(),
                    }),
                },
            ],
            windows: vec![
                WindowSnapshot {
                    id: WindowId::from("w1"),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("firefox".into()),
                    title: Some("Firefox".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: false,
                    floating_rect: None,
                    fullscreen: false,
                    focused: true,
                    urgent: false,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-1")),
                    tags: vec!["1".into()],
                },
                WindowSnapshot {
                    id: WindowId::from("w2"),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("alacritty".into()),
                    title: Some("Terminal".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: false,
                    floating_rect: None,
                    fullscreen: false,
                    focused: false,
                    urgent: false,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-2")),
                    tags: vec!["2".into()],
                },
            ],
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into(), "2".into()],
        }
    }

    #[test]
    fn wm_state_focuses_window_and_updates_current_workspace() {
        let mut state = WmState::from_snapshot(state());

        let event = state.focus_window(&WindowId::from("w2")).unwrap();

        assert_eq!(
            state.snapshot().focused_window_id,
            Some(WindowId::from("w2"))
        );
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            state.snapshot().current_output_id,
            Some(OutputId::from("out-1"))
        );
        assert!(matches!(
            event,
            CompositorEvent::FocusChange {
                focused_window_id: Some(_),
                current_workspace_id: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn wm_state_views_tag_and_recomputes_visibility() {
        let mut state = WmState::from_snapshot(state());

        let events = state
            .view_tag_on_output(&OutputId::from("out-1"), "2")
            .unwrap();

        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            state.snapshot().visible_window_ids,
            vec![WindowId::from("w2")]
        );
        assert_eq!(state.snapshot().focused_window_id, None);
        assert!(events.iter().any(|event| matches!(
            event,
            CompositorEvent::TagChange {
                workspace_id: Some(id),
                ..
            } if id == &WorkspaceId::from("ws-2")
        )));
    }

    #[test]
    fn wm_state_maps_and_destroys_windows() {
        let mut state = WmState::from_snapshot(state());
        let event = state.map_window(WindowSnapshot {
            id: WindowId::from("w3"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("discord".into()),
            title: Some("Discord".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: false,
            floating: false,
            floating_rect: None,
            fullscreen: false,
            focused: false,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            tags: vec!["1".into()],
        });

        assert!(matches!(event, CompositorEvent::WindowCreated { .. }));
        assert!(state
            .snapshot()
            .windows
            .iter()
            .any(|window| window.id == WindowId::from("w3") && window.mapped));

        let events = state.destroy_window(&WindowId::from("w3")).unwrap();

        assert!(events.iter().any(|event| matches!(
            event,
            CompositorEvent::WindowDestroyed { window_id } if window_id == &WindowId::from("w3")
        )));
        assert!(state
            .snapshot()
            .windows
            .iter()
            .all(|window| window.id != WindowId::from("w3")));
    }

    #[test]
    fn wm_state_sets_workspace_layout() {
        let mut state = WmState::from_snapshot(state());

        let event = state
            .set_layout_for_workspace(&WorkspaceId::from("ws-2"), "columns")
            .unwrap();

        assert!(matches!(
            event,
            CompositorEvent::LayoutChange {
                workspace_id: Some(_),
                layout: Some(_),
            }
        ));
        assert_eq!(
            state
                .snapshot()
                .workspace_by_id(&WorkspaceId::from("ws-2"))
                .unwrap()
                .effective_layout
                .as_ref()
                .map(|layout| layout.name.as_str()),
            Some("columns")
        );
    }

    #[test]
    fn wm_state_toggle_tag_on_current_output_is_noop_for_visible_tag() {
        let mut state = WmState::from_snapshot(state());

        let events = state.toggle_tag_on_current_output("1").unwrap();

        assert!(events.is_empty());
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );
    }

    #[test]
    fn wm_state_toggle_tag_on_current_output_switches_to_hidden_tag() {
        let mut state = WmState::from_snapshot(state());

        let events = state.toggle_tag_on_current_output("2").unwrap();

        assert!(events.iter().any(|event| matches!(
            event,
            CompositorEvent::TagChange {
                workspace_id: Some(id),
                ..
            } if id == &WorkspaceId::from("ws-2")
        )));
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
    }

    #[test]
    fn wm_state_activates_workspace_by_id() {
        let mut state = WmState::from_snapshot(state());

        let events = state
            .activate_workspace(&WorkspaceId::from("ws-2"))
            .unwrap();

        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            state.snapshot().visible_window_ids,
            vec![WindowId::from("w2")]
        );
        assert!(events.iter().any(|event| matches!(
            event,
            CompositorEvent::FocusChange {
                current_workspace_id: Some(id),
                ..
            } if id == &WorkspaceId::from("ws-2")
        )));
    }

    #[test]
    fn wm_state_assigns_workspace_to_new_output() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        let mut state = WmState::from_snapshot(snapshot);

        let events = state
            .assign_workspace_to_output(&WorkspaceId::from("ws-2"), &OutputId::from("out-2"))
            .unwrap();

        assert_eq!(
            state
                .snapshot()
                .workspace_by_id(&WorkspaceId::from("ws-2"))
                .unwrap()
                .output_id,
            Some(OutputId::from("out-2"))
        );
        assert!(events.iter().any(|event| matches!(
            event,
            CompositorEvent::TagChange {
                workspace_id: Some(id),
                ..
            } if id == &WorkspaceId::from("ws-2")
        )));
    }

    #[test]
    fn wm_state_reassigns_floating_window_when_geometry_crosses_output() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 1920,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-2")),
        });
        snapshot.workspaces[1].output_id = Some(OutputId::from("out-2"));
        snapshot.workspaces[1].visible = true;
        snapshot.windows[0].floating = true;
        snapshot.windows[0].floating_rect = Some(LayoutRect {
            x: 50.0,
            y: 60.0,
            width: 800.0,
            height: 600.0,
        });

        let mut state = WmState::from_snapshot(snapshot);
        let event = state
            .set_floating_window_geometry(
                &WindowId::from("w1"),
                LayoutRect {
                    x: 2200.0,
                    y: 120.0,
                    width: 800.0,
                    height: 600.0,
                },
            )
            .unwrap();

        assert_eq!(
            state.snapshot().windows[0].output_id,
            Some(OutputId::from("out-2"))
        );
        assert_eq!(
            state.snapshot().windows[0].workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            state.snapshot().current_output_id,
            Some(OutputId::from("out-2"))
        );
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            event,
            CompositorEvent::WindowGeometryChange {
                window_id: WindowId::from("w1"),
                floating_rect: Some(LayoutRect {
                    x: 2200.0,
                    y: 120.0,
                    width: 800.0,
                    height: 600.0,
                }),
                output_id: Some(OutputId::from("out-2")),
                workspace_id: Some(WorkspaceId::from("ws-2")),
            }
        );
    }

    #[test]
    fn wm_state_focus_monitor_right_selects_adjacent_output_workspace() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 1920,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-2")),
        });
        snapshot.workspaces[1].output_id = Some(OutputId::from("out-2"));
        snapshot.workspaces[1].visible = true;

        let mut state = WmState::from_snapshot(snapshot);
        let event = state.focus_monitor_right().unwrap();

        assert_eq!(
            state.snapshot().current_output_id,
            Some(OutputId::from("out-2"))
        );
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert!(matches!(
            event,
            CompositorEvent::FocusChange {
                current_output_id: Some(_),
                current_workspace_id: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn wm_state_resize_direction_updates_focused_floating_geometry() {
        let mut snapshot = state();
        snapshot.windows[0].floating = true;
        snapshot.windows[0].floating_rect = Some(LayoutRect {
            x: 20.0,
            y: 30.0,
            width: 400.0,
            height: 300.0,
        });

        let mut state = WmState::from_snapshot(snapshot);
        let event = state.resize_direction(FocusDirection::Right).unwrap();

        assert_eq!(
            state.snapshot().windows[0].floating_rect,
            Some(LayoutRect {
                x: 20.0,
                y: 30.0,
                width: 432.0,
                height: 300.0,
            })
        );
        assert!(matches!(
            event,
            CompositorEvent::WindowGeometryChange {
                floating_rect: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn wm_state_activate_workspace_clears_other_output_focus_flags() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-2")),
        });
        snapshot.workspaces.push(WorkspaceSnapshot {
            id: WorkspaceId::from("ws-2"),
            name: "2".into(),
            output_id: Some(OutputId::from("out-2")),
            active_tags: vec!["2".into()],
            focused: false,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "stack".into(),
            }),
        });
        let mut state = WmState::from_snapshot(snapshot);

        state
            .activate_workspace(&WorkspaceId::from("ws-2"))
            .unwrap();

        assert!(
            state
                .snapshot()
                .workspaces
                .iter()
                .find(|workspace| workspace.id == WorkspaceId::from("ws-2"))
                .unwrap()
                .focused
        );
        assert!(
            !state
                .snapshot()
                .workspaces
                .iter()
                .find(|workspace| workspace.id == WorkspaceId::from("ws-1"))
                .unwrap()
                .focused
        );
    }

    #[test]
    fn wm_state_focus_direction_cycles_visible_workspace_windows() {
        let mut snapshot = state();
        snapshot.windows.push(WindowSnapshot {
            id: WindowId::from("w3"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("thunar".into()),
            title: Some("Files".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            floating: false,
            floating_rect: None,
            fullscreen: false,
            focused: false,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            tags: vec!["1".into()],
        });
        snapshot.visible_window_ids = vec![WindowId::from("w1"), WindowId::from("w3")];
        let mut state = WmState::from_snapshot(snapshot);

        let event = state.focus_direction(FocusDirection::Right).unwrap();

        assert!(matches!(
            event,
            CompositorEvent::FocusChange {
                focused_window_id: Some(window_id),
                ..
            } if window_id == WindowId::from("w3")
        ));
        assert_eq!(
            state.snapshot().focused_window_id,
            Some(WindowId::from("w3"))
        );
    }
}
