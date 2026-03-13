use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::wm::StateSnapshot;

use crate::app::{BootstrapEvent, CompositorApp, StartupRegistration};
use crate::topology::TopologyError;
use crate::{CompositorLayoutError, LayoutService};

#[derive(Debug, thiserror::Error)]
pub enum BootstrapRunnerError {
    #[error(transparent)]
    Layout(#[from] CompositorLayoutError),
    #[error(transparent)]
    Topology(#[from] TopologyError),
}

#[derive(Debug)]
pub struct BootstrapRunner<L, R> {
    app: CompositorApp<L, R>,
}

impl<L, R> BootstrapRunner<L, R> {
    pub fn app(&self) -> &CompositorApp<L, R> {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut CompositorApp<L, R> {
        &mut self.app
    }

    pub fn into_app(self) -> CompositorApp<L, R> {
        self.app
    }
}

impl<L: spiders_config::loader::LayoutSourceLoader, R: LayoutRuntime> BootstrapRunner<L, R> {
    pub fn initialize(
        layout_service: LayoutService,
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            app: CompositorApp::initialize(layout_service, runtime_service, config, state)?,
        })
    }

    pub fn initialize_with_registration(
        layout_service: LayoutService,
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
        startup: StartupRegistration,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            app: CompositorApp::initialize_with_registration(
                layout_service,
                runtime_service,
                config,
                state,
                startup,
            )?,
        })
    }

    pub fn apply_event(&mut self, event: BootstrapEvent) -> Result<(), BootstrapRunnerError> {
        self.app.apply_bootstrap_event(event)?;
        Ok(())
    }

    pub fn apply_events<I>(&mut self, events: I) -> Result<(), BootstrapRunnerError>
    where
        I: IntoIterator<Item = BootstrapEvent>,
    {
        for event in events {
            self.apply_event(event)?;
        }
        Ok(())
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

    fn runner() -> BootstrapRunner<
        RuntimeProjectLayoutSourceLoader,
        BoaLayoutRuntime<RuntimeProjectLayoutSourceLoader>,
    > {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-bootstrap-runner-{unique}"));
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

        BootstrapRunner::initialize(LayoutService, service, config(), state()).unwrap()
    }

    #[test]
    fn runner_applies_bootstrap_events_in_order() {
        let mut runner = runner();
        runner
            .app_mut()
            .session
            .register_output_snapshot(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });

        runner
            .apply_events(vec![
                BootstrapEvent::RegisterSeat {
                    seat_name: "seat-1".into(),
                    active: true,
                },
                BootstrapEvent::RegisterWindowSurface {
                    surface_id: "window-w1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: Some(OutputId::from("out-1")),
                },
                BootstrapEvent::RegisterPopupSurface {
                    surface_id: "popup-1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    parent_surface_id: "window-w1".into(),
                },
                BootstrapEvent::MoveSurfaceToOutput {
                    surface_id: "popup-1".into(),
                    output_id: OutputId::from("out-2"),
                },
                BootstrapEvent::RemoveWindowSurface {
                    window_id: WindowId::from("w1"),
                },
            ])
            .unwrap();

        assert_eq!(
            runner.app().topology().active_seat_name.as_deref(),
            Some("seat-1")
        );
        assert!(runner.app().topology().surface("window-w1").is_none());
        assert_eq!(
            runner
                .app()
                .topology()
                .surface("popup-1")
                .unwrap()
                .output_id,
            Some(OutputId::from("out-2"))
        );
    }

    #[test]
    fn runner_initializes_with_custom_registration() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-bootstrap-runner-custom-{unique}"));
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

        let runner = BootstrapRunner::initialize_with_registration(
            LayoutService,
            service,
            config(),
            snapshot,
            StartupRegistration {
                seats: vec!["seat-a".into()],
                outputs: vec![OutputId::from("out-1"), OutputId::from("out-2")],
                active_seat: Some("seat-a".into()),
                active_output: Some(OutputId::from("out-2")),
            },
        )
        .unwrap();

        assert_eq!(
            runner.app().topology().active_seat_name.as_deref(),
            Some("seat-a")
        );
        assert_eq!(
            runner.app().topology().active_output_id,
            Some(OutputId::from("out-2"))
        );
    }
}
