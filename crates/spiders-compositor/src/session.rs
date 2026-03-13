use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::api::{CompositorEvent, FocusDirection, WmAction};
use spiders_shared::ids::WindowId;
use spiders_shared::wm::{StateSnapshot, WindowSnapshot};

use crate::actions::{apply_action, ActionError, ActionOutcome};
use crate::runtime::{CompositorRuntimeState, WorkspaceLayoutState};
use crate::wm::WmState;
use crate::{CompositorLayoutError, LayoutService};

#[derive(Debug)]
pub struct CompositorSession<L, R> {
    runtime: CompositorRuntimeState<L, R>,
    wm: WmState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUpdate {
    pub events: Vec<CompositorEvent>,
    pub recomputed_layout: bool,
    pub current_layout: Option<WorkspaceLayoutState>,
}

impl<L, R> CompositorSession<L, R> {
    pub fn new(runtime: CompositorRuntimeState<L, R>, wm: WmState) -> Self {
        Self { runtime, wm }
    }

    pub fn runtime(&self) -> &CompositorRuntimeState<L, R> {
        &self.runtime
    }

    pub fn wm(&self) -> &WmState {
        &self.wm
    }

    pub fn state(&self) -> &StateSnapshot {
        self.wm.snapshot()
    }

    pub fn current_layout(&self) -> Option<&WorkspaceLayoutState> {
        self.runtime.current_layout()
    }
}

impl<L: spiders_config::loader::LayoutSourceLoader, R: LayoutRuntime> CompositorSession<L, R> {
    pub fn initialize(
        layout_service: LayoutService,
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, CompositorLayoutError> {
        let runtime = crate::runtime::initialize_runtime_state(
            layout_service,
            runtime_service,
            config,
            state.clone(),
        )?;
        let wm = WmState::from_snapshot(state);

        Ok(Self::new(runtime, wm))
    }

    pub fn apply_action(&mut self, action: &WmAction) -> Result<SessionUpdate, ActionError> {
        match action {
            WmAction::FocusDirection { direction } => self.focus_direction(*direction),
            WmAction::ToggleFloating => self.toggle_focused_floating(),
            WmAction::ToggleFullscreen => self.toggle_focused_fullscreen(),
            _ => {
                let outcome = apply_action(&mut self.runtime, &mut self.wm, action)?;
                Ok(self.session_update(outcome))
            }
        }
    }

    pub fn map_window(&mut self, window: WindowSnapshot) -> Result<SessionUpdate, ActionError> {
        let event = self.wm.map_window(window);
        self.runtime
            .update_from_wm_state(self.wm.snapshot().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(ActionOutcome {
            events: vec![event],
            recomputed_layout: true,
        }))
    }

    pub fn focus_window(&mut self, window_id: &WindowId) -> Result<SessionUpdate, ActionError> {
        let event = self.wm.focus_window(window_id)?;
        self.runtime
            .update_from_wm_state(self.wm.snapshot().clone());
        Ok(self.session_update(ActionOutcome {
            events: vec![event],
            recomputed_layout: false,
        }))
    }

    pub fn destroy_window(&mut self, window_id: &WindowId) -> Result<SessionUpdate, ActionError> {
        let events = self.wm.destroy_window(window_id)?;
        self.runtime
            .update_from_wm_state(self.wm.snapshot().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(ActionOutcome {
            events,
            recomputed_layout: true,
        }))
    }

    pub fn toggle_focused_floating(&mut self) -> Result<SessionUpdate, ActionError> {
        let event = self.wm.toggle_focused_floating()?;
        self.runtime
            .update_from_wm_state(self.wm.snapshot().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(ActionOutcome {
            events: vec![event],
            recomputed_layout: true,
        }))
    }

    pub fn toggle_focused_fullscreen(&mut self) -> Result<SessionUpdate, ActionError> {
        let event = self.wm.toggle_focused_fullscreen()?;
        self.runtime
            .update_from_wm_state(self.wm.snapshot().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(ActionOutcome {
            events: vec![event],
            recomputed_layout: true,
        }))
    }

    pub fn focus_direction(
        &mut self,
        direction: FocusDirection,
    ) -> Result<SessionUpdate, ActionError> {
        let order = self.focus_ordered_window_ids();
        let focused = self.wm.focused_window_id()?.clone();
        let current_index = order
            .iter()
            .position(|window_id| window_id == &focused)
            .unwrap_or(0);
        let next_index = match direction {
            FocusDirection::Left | FocusDirection::Up => {
                (current_index + order.len() - 1) % order.len()
            }
            FocusDirection::Right | FocusDirection::Down => (current_index + 1) % order.len(),
        };

        self.focus_window(&order[next_index])
    }

    fn focus_ordered_window_ids(&self) -> Vec<WindowId> {
        let mut ordered = Vec::new();

        if let Some(layout) = self.current_layout() {
            for node in layout.response.root.window_nodes() {
                if let Some(window_id) = match node {
                    spiders_shared::layout::LayoutSnapshotNode::Window { window_id, .. } => {
                        window_id.clone()
                    }
                    _ => None,
                } {
                    if !ordered.iter().any(|id| id == &window_id) {
                        ordered.push(window_id);
                    }
                }
            }
        }

        if ordered.is_empty() {
            return self.state().visible_window_ids.clone();
        }

        ordered
    }

    fn session_update(&self, outcome: ActionOutcome) -> SessionUpdate {
        SessionUpdate {
            events: outcome.events,
            recomputed_layout: outcome.recomputed_layout,
            current_layout: self.runtime.current_layout().cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_config::runtime::BoaLayoutRuntime;
    use spiders_config::service::ConfigRuntimeService;
    use spiders_shared::api::{CompositorEvent, FocusDirection, WmAction};
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    fn config() -> Config {
        Config {
            layouts: vec![
                LayoutDefinition {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                },
                LayoutDefinition {
                    name: "columns".into(),
                    module: "layouts/columns.js".into(),
                    stylesheet: String::new(),
                },
            ],
            ..Config::default()
        }
    }

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 800,
                logical_height: 600,
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
                        name: "columns".into(),
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

    fn session() -> CompositorSession<
        RuntimeProjectLayoutSourceLoader,
        BoaLayoutRuntime<RuntimeProjectLayoutSourceLoader>,
    > {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-session-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'priority', match: 'app_id=\"thunar\"' }, { type: 'slot', id: 'rest' }] })",
        )
        .unwrap();
        fs::write(
            runtime_root.join("layouts/columns.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);

        CompositorSession::initialize(LayoutService, service, config(), state()).unwrap()
    }

    #[test]
    fn session_applies_layout_action_and_updates_runtime_and_wm_state() {
        let mut session = session();

        let update = session
            .apply_action(&WmAction::SetLayout {
                name: "columns".into(),
            })
            .unwrap();

        assert!(update.recomputed_layout);
        assert!(update
            .events
            .iter()
            .any(|event| matches!(event, CompositorEvent::LayoutChange { .. })));
        assert_eq!(
            update
                .current_layout
                .as_ref()
                .and_then(|layout| layout.request.layout_name.as_deref()),
            Some("columns")
        );
        assert_eq!(
            session
                .wm()
                .current_workspace()
                .unwrap()
                .effective_layout
                .as_ref()
                .map(|layout| layout.name.as_str()),
            Some("columns")
        );
        assert_eq!(
            session
                .runtime()
                .current_layout()
                .and_then(|layout| layout.request.layout_name.as_deref()),
            Some("columns")
        );
    }

    #[test]
    fn session_applies_tag_switch_action_and_updates_snapshot() {
        let mut session = session();

        let update = session
            .apply_action(&WmAction::ToggleViewTag { tag: "2".into() })
            .unwrap();

        assert!(update.recomputed_layout);
        assert_eq!(
            session.state().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            update
                .current_layout
                .as_ref()
                .and_then(|layout| layout.request.layout_name.as_deref()),
            Some("columns")
        );
    }

    #[test]
    fn session_focus_window_updates_wm_state_without_relayout() {
        let mut session = session();

        let update = session.focus_window(&WindowId::from("w1")).unwrap();

        assert!(!update.recomputed_layout);
        assert!(update.events.iter().any(|event| matches!(
            event,
            CompositorEvent::FocusChange {
                focused_window_id: Some(window_id),
                ..
            } if window_id == &WindowId::from("w1")
        )));
        assert_eq!(
            session.state().focused_window_id,
            Some(WindowId::from("w1"))
        );
    }

    #[test]
    fn session_map_window_recomputes_layout_state() {
        let mut session = session();

        let update = session
            .map_window(WindowSnapshot {
                id: WindowId::from("w3"),
                shell: ShellKind::XdgToplevel,
                app_id: Some("thunar".into()),
                title: Some("Files".into()),
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
            })
            .unwrap();

        assert!(update.recomputed_layout);
        assert!(update
            .events
            .iter()
            .any(|event| matches!(event, CompositorEvent::WindowCreated { .. })));
        assert!(session
            .state()
            .windows
            .iter()
            .any(|window| window.id == WindowId::from("w3") && window.mapped));
        assert!(update.current_layout.is_some());
    }

    #[test]
    fn session_destroy_window_recomputes_layout_state() {
        let mut session = session();

        let update = session.destroy_window(&WindowId::from("w1")).unwrap();

        assert!(update.recomputed_layout);
        assert!(update.events.iter().any(|event| matches!(
            event,
            CompositorEvent::WindowDestroyed { window_id } if window_id == &WindowId::from("w1")
        )));
        assert!(session
            .state()
            .windows
            .iter()
            .all(|window| window.id != WindowId::from("w1")));
    }

    #[test]
    fn session_focus_direction_action_updates_focus() {
        let mut session = session();
        let _ = session.map_window(WindowSnapshot {
            id: WindowId::from("w3"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("thunar".into()),
            title: Some("Files".into()),
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
        let _ = session.map_window(WindowSnapshot {
            id: WindowId::from("w4"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("foot".into()),
            title: Some("Foot".into()),
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

        let update = session
            .apply_action(&WmAction::FocusDirection {
                direction: FocusDirection::Right,
            })
            .unwrap();

        assert!(!update.recomputed_layout);
        assert_eq!(
            session.state().focused_window_id,
            Some(WindowId::from("w4"))
        );
    }

    #[test]
    fn session_toggle_fullscreen_recomputes_layout_state() {
        let mut session = session();

        let update = session.toggle_focused_fullscreen().unwrap();

        assert!(update.recomputed_layout);
        assert!(update.events.iter().any(|event| matches!(
            event,
            CompositorEvent::WindowFullscreenChange { window_id, fullscreen }
                if window_id == &WindowId::from("w1") && *fullscreen
        )));
        assert!(session
            .state()
            .windows
            .iter()
            .any(|window| window.id == WindowId::from("w1") && window.fullscreen));
    }

    #[test]
    fn session_toggle_floating_recomputes_layout_state() {
        let mut session = session();

        let update = session.toggle_focused_floating().unwrap();

        assert!(update.recomputed_layout);
        assert!(update.events.iter().any(|event| matches!(
            event,
            CompositorEvent::WindowFloatingChange { window_id, floating }
                if window_id == &WindowId::from("w1") && *floating
        )));
        assert!(session
            .state()
            .windows
            .iter()
            .any(|window| window.id == WindowId::from("w1") && window.floating));
    }
}
