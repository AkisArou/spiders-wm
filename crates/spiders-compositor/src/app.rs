use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::ids::OutputId;
use spiders_shared::wm::StateSnapshot;

use crate::session::CompositorSession;
use crate::topology::{CompositorTopologyState, SurfaceState, TopologyError};
use crate::{CompositorLayoutError, LayoutService};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupRegistration {
    pub seats: Vec<String>,
    pub outputs: Vec<OutputId>,
    pub active_seat: Option<String>,
    pub active_output: Option<OutputId>,
}

impl Default for StartupRegistration {
    fn default() -> Self {
        Self {
            seats: vec!["seat-0".into()],
            outputs: Vec::new(),
            active_seat: Some("seat-0".into()),
            active_output: None,
        }
    }
}

impl StartupRegistration {
    pub fn from_state(state: &StateSnapshot) -> Self {
        let mut registration = Self::default();
        registration.outputs = state
            .outputs
            .iter()
            .map(|output| output.id.clone())
            .collect();
        registration.active_output = state
            .current_output_id
            .clone()
            .or_else(|| registration.outputs.first().cloned());
        registration
    }
}

#[derive(Debug)]
pub struct CompositorApp<L, R> {
    pub session: CompositorSession<L, R>,
    pub startup: StartupRegistration,
}

impl<L, R> CompositorApp<L, R> {
    pub fn topology(&self) -> &CompositorTopologyState {
        self.session.topology()
    }

    pub fn state(&self) -> &StateSnapshot {
        self.session.state()
    }

    pub fn register_popup_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: Option<OutputId>,
        parent_surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.session
            .register_popup_surface(surface_id, output_id, parent_surface_id)
    }

    pub fn register_layer_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
    ) -> Result<&SurfaceState, TopologyError> {
        self.session.register_layer_surface(surface_id, output_id)
    }

    pub fn register_unmanaged_surface(
        &mut self,
        surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.session.register_unmanaged_surface(surface_id)
    }

    pub fn move_surface_to_output(
        &mut self,
        surface_id: &str,
        output_id: OutputId,
    ) -> Result<(), TopologyError> {
        self.session.move_surface_to_output(surface_id, output_id)
    }

    pub fn unmap_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        self.session.unmap_surface(surface_id)
    }

    pub fn activate_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        self.session.activate_seat(seat_name)
    }

    pub fn activate_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.session.activate_output(output_id)
    }

    pub fn disable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.session.disable_output(output_id)
    }

    pub fn enable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.session.enable_output(output_id)
    }
}

impl<L: spiders_config::loader::LayoutSourceLoader, R: LayoutRuntime> CompositorApp<L, R> {
    pub fn initialize(
        layout_service: LayoutService,
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, CompositorLayoutError> {
        Self::initialize_with_registration(
            layout_service,
            runtime_service,
            config,
            state.clone(),
            StartupRegistration::from_state(&state),
        )
    }

    pub fn initialize_with_registration(
        layout_service: LayoutService,
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
        startup: StartupRegistration,
    ) -> Result<Self, CompositorLayoutError> {
        let mut session =
            CompositorSession::initialize(layout_service, runtime_service, config, state)?;

        for seat in &startup.seats {
            session.register_seat(seat.clone());
        }
        for output in &startup.outputs {
            let _ = session.register_output(output.clone());
        }
        if let Some(active_seat) = startup.active_seat.as_deref() {
            let _ = session.activate_seat(active_seat);
        }
        if let Some(active_output) = startup.active_output.as_ref() {
            let _ = session.activate_output(active_output);
        }

        Ok(Self { session, startup })
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
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    fn config() -> Config {
        Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: String::new(),
            }],
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
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![WindowSnapshot {
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
            }],
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn app_initializes_session_and_registers_startup_topology() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);

        let app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();

        assert_eq!(app.startup.seats, vec!["seat-0".to_string()]);
        assert_eq!(app.startup.outputs, vec![OutputId::from("out-1")]);
        assert_eq!(app.startup.active_seat.as_deref(), Some("seat-0"));
        assert_eq!(app.startup.active_output, Some(OutputId::from("out-1")));
        assert!(app.topology().seat("seat-0").is_some());
        assert!(app.topology().output(&OutputId::from("out-1")).is_some());
        assert_eq!(app.topology().active_seat_name.as_deref(), Some("seat-0"));
        assert_eq!(
            app.topology().active_output_id,
            Some(OutputId::from("out-1"))
        );
        assert_eq!(
            app.state().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );
    }

    #[test]
    fn app_initializes_with_custom_startup_registration() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-custom-startup-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });

        let app = CompositorApp::initialize_with_registration(
            LayoutService,
            service,
            config(),
            snapshot,
            StartupRegistration {
                seats: vec!["seat-a".into(), "seat-b".into()],
                outputs: vec![OutputId::from("out-1"), OutputId::from("out-2")],
                active_seat: Some("seat-b".into()),
                active_output: Some(OutputId::from("out-2")),
            },
        )
        .unwrap();

        assert_eq!(app.topology().active_seat_name.as_deref(), Some("seat-b"));
        assert_eq!(
            app.topology().active_output_id,
            Some(OutputId::from("out-2"))
        );
    }

    #[test]
    fn app_registers_and_moves_backend_agnostic_surfaces() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-surface-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);

        let mut app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();
        app.session.register_output_snapshot(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });

        app.register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1")
            .unwrap();
        app.register_layer_surface("layer-1", OutputId::from("out-1"))
            .unwrap();
        app.register_unmanaged_surface("overlay-1").unwrap();
        app.move_surface_to_output("layer-1", OutputId::from("out-2"))
            .unwrap();
        app.unmap_surface("popup-1").unwrap();

        assert_eq!(
            app.topology()
                .surface("popup-1")
                .unwrap()
                .parent_surface_id
                .as_deref(),
            Some("window-w1")
        );
        assert!(!app.topology().surface("popup-1").unwrap().mapped);
        assert_eq!(
            app.topology().surface("layer-1").unwrap().output_id,
            Some(OutputId::from("out-2"))
        );
        assert_eq!(
            app.topology().surface("overlay-1").unwrap().role,
            crate::SurfaceRole::Unmanaged
        );
    }

    #[test]
    fn app_tracks_output_and_seat_lifecycle_changes() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-seat-output-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);

        let mut app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();
        app.session.register_output_snapshot(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        app.session.register_seat("seat-1");

        app.activate_output(&OutputId::from("out-2")).unwrap();
        app.disable_output(&OutputId::from("out-2")).unwrap();
        app.enable_output(&OutputId::from("out-2")).unwrap();
        app.activate_seat("seat-1").unwrap();

        assert!(app.topology().active_output().is_none());
        assert!(
            app.topology()
                .output(&OutputId::from("out-2"))
                .unwrap()
                .snapshot
                .enabled
        );
        assert_eq!(app.topology().active_seat_name.as_deref(), Some("seat-1"));
    }
}
