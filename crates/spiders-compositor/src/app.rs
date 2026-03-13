use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::ids::OutputId;
use spiders_shared::wm::StateSnapshot;

use crate::session::CompositorSession;
use crate::topology::CompositorTopologyState;
use crate::{CompositorLayoutError, LayoutService};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupRegistration {
    pub seats: Vec<String>,
    pub outputs: Vec<OutputId>,
}

impl Default for StartupRegistration {
    fn default() -> Self {
        Self {
            seats: vec!["seat-0".into()],
            outputs: Vec::new(),
        }
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
}

impl<L: spiders_config::loader::LayoutSourceLoader, R: LayoutRuntime> CompositorApp<L, R> {
    pub fn initialize(
        layout_service: LayoutService,
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, CompositorLayoutError> {
        let mut session =
            CompositorSession::initialize(layout_service, runtime_service, config, state.clone())?;
        let startup = StartupRegistration {
            seats: vec!["seat-0".into()],
            outputs: state
                .outputs
                .iter()
                .map(|output| output.id.clone())
                .collect(),
        };

        for seat in &startup.seats {
            session.register_seat(seat.clone());
        }
        for output in &startup.outputs {
            let _ = session.register_output(output.clone());
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
        assert!(app.topology().seat("seat-0").is_some());
        assert!(app.topology().output(&OutputId::from("out-1")).is_some());
        assert_eq!(
            app.state().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );
    }
}
