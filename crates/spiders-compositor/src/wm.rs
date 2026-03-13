use spiders_shared::api::CompositorEvent;
use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub fn toggle_focused_floating(&mut self) -> Result<CompositorEvent, WmStateError> {
        let window_id = self.focused_window_id()?.clone();
        let window = self
            .snapshot
            .windows
            .iter_mut()
            .find(|window| window.id == window_id)
            .ok_or_else(|| WmStateError::WindowNotFound(window_id.clone()))?;

        window.floating = !window.floating;

        Ok(CompositorEvent::WindowFloatingChange {
            window_id,
            floating: window.floating,
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

        assert!(events
            .iter()
            .any(|event| matches!(event, CompositorEvent::WindowDestroyed { window_id } if window_id == &WindowId::from("w3"))));
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
}
