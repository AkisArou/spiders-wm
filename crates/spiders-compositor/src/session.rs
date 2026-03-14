use spiders_config::model::Config;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::api::{CompositorEvent, FocusDirection, WmAction};
use spiders_shared::ids::{OutputId, WindowId};
use spiders_shared::runtime::{AuthoringRuntime, LayoutSourceLoader};
use spiders_shared::wm::{StateSnapshot, WindowSnapshot};
use spiders_wm::{
    CompositorTopologyState, DomainSession, DomainUpdate, LayerSurfaceMetadata, SurfaceState,
    TopologyError, WmState,
};

use crate::actions::{apply_action, ActionError};
use crate::effects::WindowDecorationPolicy;
use crate::runtime::WindowPlacement;
use crate::runtime::{CompositorRuntimeState, WorkspaceLayoutState};
use crate::titlebar::TitlebarRenderItem;
use crate::{CompositorLayoutError, LayoutService};

#[derive(Debug)]
pub struct CompositorSession<L, R> {
    runtime: CompositorRuntimeState<L, R>,
    domain: DomainSession,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUpdate {
    pub events: Vec<CompositorEvent>,
    pub recomputed_layout: bool,
    pub current_layout: Option<WorkspaceLayoutState>,
    pub decoration_policies: Vec<(WindowId, WindowDecorationPolicy)>,
    pub titlebar_render_plan: Vec<TitlebarRenderItem>,
    pub topology: CompositorTopologyState,
}

impl<L, R> CompositorSession<L, R> {
    pub fn new(
        runtime: CompositorRuntimeState<L, R>,
        wm: WmState,
        topology: CompositorTopologyState,
    ) -> Self {
        Self {
            runtime,
            domain: DomainSession::new(wm, topology),
        }
    }

    pub fn runtime(&self) -> &CompositorRuntimeState<L, R> {
        &self.runtime
    }

    pub fn wm(&self) -> &WmState {
        self.domain.wm()
    }

    pub fn state(&self) -> &StateSnapshot {
        self.domain.state()
    }

    pub fn current_layout(&self) -> Option<&WorkspaceLayoutState> {
        self.runtime.current_layout()
    }

    pub fn topology(&self) -> &CompositorTopologyState {
        self.domain.topology()
    }

    pub fn window_decoration_policies(&self) -> Vec<(WindowId, WindowDecorationPolicy)> {
        self.runtime
            .current_layout()
            .map(|layout| {
                layout
                    .effects
                    .windows
                    .iter()
                    .filter_map(|window| {
                        layout
                            .effects
                            .window_decoration_policy(&window.window_id)
                            .map(|policy| (window.window_id.clone(), policy))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn current_titlebar_render_plan(&self) -> Vec<TitlebarRenderItem> {
        self.runtime.current_titlebar_render_plan()
    }

    pub fn current_window_placements(&self) -> Vec<WindowPlacement> {
        self.runtime.current_window_placements()
    }

    pub fn register_popup_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: Option<OutputId>,
        parent_surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.domain
            .register_popup_surface(surface_id, output_id, parent_surface_id)
    }

    pub fn register_layer_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
    ) -> Result<&SurfaceState, TopologyError> {
        self.domain.register_layer_surface(surface_id, output_id)
    }

    pub fn register_layer_surface_with_metadata(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
        metadata: LayerSurfaceMetadata,
    ) -> Result<&SurfaceState, TopologyError> {
        self.domain
            .register_layer_surface_with_metadata(surface_id, output_id, metadata)
    }

    pub fn register_unmanaged_surface(
        &mut self,
        surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.domain.register_unmanaged_surface(surface_id)
    }

    pub fn register_output_snapshot(&mut self, output: spiders_shared::wm::OutputSnapshot) {
        self.domain.register_output_snapshot(output);
    }

    pub fn register_backend_output_snapshot(&mut self, output: spiders_shared::wm::OutputSnapshot) {
        self.domain.register_backend_output_snapshot(output);
    }

    pub fn register_output_by_id(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.domain.register_output_by_id(output_id)
    }

    pub fn register_startup_seeded_output(
        &mut self,
        output_id: &OutputId,
    ) -> Result<(), TopologyError> {
        self.domain.register_startup_seeded_output(output_id)
    }

    pub fn unregister_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.domain.unregister_output(output_id)
    }

    pub fn unregister_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        self.domain.unregister_seat(seat_name)
    }

    pub fn unregister_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        self.domain.unregister_surface(surface_id)
    }

    pub fn unregister_window_surface(&mut self, window_id: &WindowId) -> Result<(), TopologyError> {
        self.domain.unregister_window_surface(window_id)
    }

    pub fn move_surface_to_output(
        &mut self,
        surface_id: &str,
        output_id: OutputId,
    ) -> Result<(), TopologyError> {
        self.domain.move_surface_to_output(surface_id, output_id)
    }

    pub fn unmap_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        self.domain.unmap_surface(surface_id)
    }

    pub fn register_window_surface(
        &mut self,
        surface_id: impl Into<String>,
        window_id: WindowId,
        output_id: Option<OutputId>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.domain
            .register_window_surface(surface_id, window_id, output_id)
    }

    pub fn register_seat(&mut self, seat_name: impl Into<String>) -> &str {
        self.domain.register_seat(seat_name)
    }

    pub fn activate_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        self.domain.activate_seat(seat_name)
    }

    pub fn activate_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.domain.activate_output(output_id)
    }

    pub fn focus_seat(
        &mut self,
        seat_name: &str,
        window_id: Option<WindowId>,
        output_id: Option<OutputId>,
    ) -> Result<(), TopologyError> {
        self.domain.focus_seat(seat_name, window_id, output_id)
    }

    pub fn disable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.domain.disable_output(output_id)
    }

    pub fn enable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.domain.enable_output(output_id)
    }
}

impl<L: LayoutSourceLoader<Config>, R: AuthoringRuntime<Config = Config>> CompositorSession<L, R> {
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
        let topology = CompositorTopologyState::from_snapshot(wm.snapshot());

        Ok(Self::new(runtime, wm, topology))
    }

    pub fn apply_action(&mut self, action: &WmAction) -> Result<SessionUpdate, ActionError> {
        match action {
            WmAction::FocusDirection { direction } => self.focus_direction(*direction),
            WmAction::ToggleFloating => self.toggle_focused_floating(),
            WmAction::ToggleFullscreen => self.toggle_focused_fullscreen(),
            _ => {
                let outcome = apply_action(&mut self.runtime, self.domain.wm_mut(), action)?;
                Ok(SessionUpdate {
                    events: outcome.events,
                    recomputed_layout: outcome.recomputed_layout,
                    current_layout: self.runtime.current_layout().cloned(),
                    decoration_policies: self.window_decoration_policies(),
                    titlebar_render_plan: self.current_titlebar_render_plan(),
                    topology: self.domain.topology().clone(),
                })
            }
        }
    }

    pub fn map_window(&mut self, window: WindowSnapshot) -> Result<SessionUpdate, ActionError> {
        let update = self.domain.map_window(window).map_err(map_domain_error)?;
        self.runtime
            .update_from_wm_state(self.domain.state().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(update))
    }

    pub fn focus_window(&mut self, window_id: &WindowId) -> Result<SessionUpdate, ActionError> {
        let update = self
            .domain
            .focus_window(window_id)
            .map_err(map_domain_error)?;
        self.runtime
            .update_from_wm_state(self.domain.state().clone());
        Ok(self.session_update(update))
    }

    pub fn destroy_window(&mut self, window_id: &WindowId) -> Result<SessionUpdate, ActionError> {
        let update = self
            .domain
            .destroy_window(window_id)
            .map_err(map_domain_error)?;
        self.runtime
            .update_from_wm_state(self.domain.state().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(update))
    }

    pub fn toggle_focused_floating(&mut self) -> Result<SessionUpdate, ActionError> {
        let update = self
            .domain
            .toggle_focused_floating()
            .map_err(map_domain_error)?;
        self.runtime
            .update_from_wm_state(self.domain.state().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(update))
    }

    pub fn toggle_focused_fullscreen(&mut self) -> Result<SessionUpdate, ActionError> {
        let update = self
            .domain
            .toggle_focused_fullscreen()
            .map_err(map_domain_error)?;
        self.runtime
            .update_from_wm_state(self.domain.state().clone());
        self.runtime.recompute_current_layout()?;
        Ok(self.session_update(update))
    }

    pub fn focus_direction(
        &mut self,
        direction: FocusDirection,
    ) -> Result<SessionUpdate, ActionError> {
        let order = self.focus_ordered_window_ids();
        let update = self
            .domain
            .focus_direction_with_order(&order, direction)
            .map_err(map_domain_error)?;
        self.runtime
            .update_from_wm_state(self.domain.state().clone());
        Ok(self.session_update(update))
    }

    pub fn register_output(&mut self, output_id: OutputId) -> Result<(), TopologyError> {
        self.register_output_by_id(&output_id)
    }

    pub fn window_surface(&self, window_id: &WindowId) -> Option<&SurfaceState> {
        self.domain
            .topology()
            .surfaces
            .iter()
            .find(|surface| surface.window_id.as_ref() == Some(window_id))
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

    fn session_update(&self, outcome: DomainUpdate) -> SessionUpdate {
        SessionUpdate {
            events: outcome.events,
            recomputed_layout: outcome.recomputed_layout,
            current_layout: self.runtime.current_layout().cloned(),
            decoration_policies: self.window_decoration_policies(),
            titlebar_render_plan: self.current_titlebar_render_plan(),
            topology: outcome.topology,
        }
    }
}

fn map_topology_error(error: TopologyError) -> ActionError {
    ActionError::Layout(CompositorLayoutError::Runtime(
        spiders_shared::runtime::RuntimeError::Other {
            message: error.to_string(),
        },
    ))
}

fn map_domain_error(error: spiders_wm::DomainSessionError) -> ActionError {
    match error {
        spiders_wm::DomainSessionError::Wm(error) => ActionError::WmState(error),
        spiders_wm::DomainSessionError::Topology(error) => map_topology_error(error),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_config::service::ConfigRuntimeService;
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::BoaLayoutRuntime;
    use spiders_shared::api::{CompositorEvent, FocusDirection, WmAction};
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };
    use spiders_wm::SurfaceRole;

    use super::*;

    fn config() -> Config {
        Config {
            layouts: vec![
                LayoutDefinition {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                    effects_stylesheet: String::new(),
                    runtime_source: None,
                },
                LayoutDefinition {
                    name: "columns".into(),
                    module: "layouts/columns.js".into(),
                    stylesheet: String::new(),
                    effects_stylesheet: String::new(),
                    runtime_source: None,
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
                logical_x: 0,
                logical_y: 0,
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

        let mut session =
            CompositorSession::initialize(LayoutService, service, config(), state()).unwrap();
        session.register_seat("seat-0");
        session
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
        assert!(update.topology.seat("seat-0").is_some());
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
                floating_rect: None,
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
        assert_eq!(
            session.window_surface(&WindowId::from("w3")).unwrap().id,
            "window-w3"
        );
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
        assert!(session.window_surface(&WindowId::from("w1")).is_none());
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
            floating_rect: None,
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
            floating_rect: None,
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
        assert_eq!(
            session.topology().seat("seat-0").unwrap().focused_window_id,
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

    #[test]
    fn session_update_exposes_decoration_policies() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-session-effects-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();
        fs::write(
            runtime_root.join("layouts/columns.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let mut config = config();
        config.layouts[0].effects_stylesheet =
            "window { appearance: none; } window::titlebar { background: #111; }".into();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);
        let mut session =
            CompositorSession::initialize(LayoutService, service, config, state()).unwrap();
        session.register_seat("seat-0");

        let update = session.toggle_focused_fullscreen().unwrap();

        assert!(update
            .decoration_policies
            .iter()
            .any(|(window_id, policy)| window_id == &WindowId::from("w1")
                && !policy.decorations_visible));
    }

    #[test]
    fn session_registers_snapshot_outputs_in_topology() {
        let mut session = session();

        session.register_output(OutputId::from("out-1")).unwrap();

        assert!(session
            .topology()
            .output(&OutputId::from("out-1"))
            .is_some());
    }

    #[test]
    fn session_activates_and_toggles_outputs() {
        let mut session = session();
        session.register_output_snapshot(spiders_shared::wm::OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: spiders_shared::wm::OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });

        session.activate_output(&OutputId::from("out-2")).unwrap();
        session.disable_output(&OutputId::from("out-2")).unwrap();
        session.enable_output(&OutputId::from("out-2")).unwrap();

        assert_eq!(
            session.topology().active_output_id,
            Some(OutputId::from("out-1"))
        );
        assert!(
            session
                .topology()
                .output(&OutputId::from("out-2"))
                .unwrap()
                .snapshot
                .enabled
        );
    }

    #[test]
    fn session_tracks_active_seat_changes() {
        let mut session = session();
        session.register_seat("seat-1");

        session.activate_seat("seat-1").unwrap();

        assert_eq!(
            session.topology().active_seat_name.as_deref(),
            Some("seat-1")
        );
        assert!(session.topology().seat("seat-1").unwrap().active);
    }

    #[test]
    fn session_registers_popup_layer_and_unmanaged_surfaces() {
        let mut session = session();

        session
            .register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1")
            .unwrap();
        session
            .register_layer_surface("layer-1", OutputId::from("out-1"))
            .unwrap();
        session.register_unmanaged_surface("overlay-1").unwrap();

        assert_eq!(
            session.topology().surface("popup-1").unwrap().role,
            SurfaceRole::Popup
        );
        assert_eq!(
            session.topology().surface("layer-1").unwrap().role,
            SurfaceRole::Layer
        );
        assert_eq!(
            session.topology().surface("overlay-1").unwrap().role,
            SurfaceRole::Unmanaged
        );
    }

    #[test]
    fn session_moves_and_unmaps_registered_surfaces() {
        let mut session = session();
        session.register_output_snapshot(spiders_shared::wm::OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: spiders_shared::wm::OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        session
            .register_layer_surface("layer-1", OutputId::from("out-1"))
            .unwrap();

        session
            .move_surface_to_output("layer-1", OutputId::from("out-2"))
            .unwrap();
        session.unmap_surface("layer-1").unwrap();

        assert_eq!(
            session.topology().surface("layer-1").unwrap().output_id,
            Some(OutputId::from("out-2"))
        );
        assert!(!session.topology().surface("layer-1").unwrap().mapped);
    }
}
