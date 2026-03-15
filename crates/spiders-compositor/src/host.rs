use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::Config;
use spiders_shared::runtime::AuthoringLayoutRuntime;
use spiders_shared::wm::StateSnapshot;
use spiders_wm::{BootstrapEvent, BootstrapFailureTrace, BootstrapRunTrace, StartupRegistration};

use crate::runner::{BootstrapRunner, BootstrapRunnerError};
use crate::scenario::BootstrapScenario;
use crate::transcript::BootstrapTranscript;
use crate::{CompositorApp, LayoutService};

#[derive(Debug)]
pub struct CompositorHost<R> {
    runner: BootstrapRunner<R>,
}

impl<R> CompositorHost<R> {
    pub fn runner(&self) -> &BootstrapRunner<R> {
        &self.runner
    }

    pub fn runner_mut(&mut self) -> &mut BootstrapRunner<R> {
        &mut self.runner
    }

    pub fn app(&self) -> &CompositorApp<R> {
        self.runner.app()
    }

    pub fn bootstrap_trace(&self) -> BootstrapRunTrace {
        self.runner.trace()
    }
}

impl<R: AuthoringLayoutRuntime<Config = Config>> CompositorHost<R> {
    pub fn initialize(
        authoring_layout_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            runner: BootstrapRunner::initialize(
                LayoutService,
                authoring_layout_service,
                config,
                state,
            )?,
        })
    }

    pub fn initialize_with_registration(
        authoring_layout_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
        startup: StartupRegistration,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            runner: BootstrapRunner::initialize_with_registration(
                LayoutService,
                authoring_layout_service,
                config,
                state,
                startup,
            )?,
        })
    }

    pub fn initialize_with_transcript(
        authoring_layout_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
        transcript: &BootstrapTranscript,
    ) -> Result<Self, BootstrapRunnerError> {
        Self::initialize_with_registration(
            authoring_layout_service,
            config,
            state,
            transcript.startup.clone(),
        )
    }

    pub fn apply_bootstrap_event(
        &mut self,
        event: BootstrapEvent,
    ) -> Result<(), BootstrapRunnerError> {
        self.runner.apply_event(event)
    }

    pub fn apply_bootstrap_scenario(
        &mut self,
        scenario: BootstrapScenario,
    ) -> Result<(), BootstrapRunnerError> {
        self.runner.apply_scenario(scenario)
    }

    pub fn apply_bootstrap_scenario_with_trace(
        &mut self,
        scenario: BootstrapScenario,
    ) -> Result<(), BootstrapFailureTrace> {
        self.runner.apply_scenario_with_trace(scenario)
    }

    pub fn apply_bootstrap_transcript(
        &mut self,
        transcript: BootstrapTranscript,
    ) -> Result<(), BootstrapFailureTrace> {
        self.runner.apply_scenario_with_trace(transcript.scenario)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::authoring_layout::AuthoringLayoutService;
    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::QuickJsPreparedLayoutRuntime;
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
                effects_stylesheet: String::new(),
                runtime_graph: None,
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
                logical_x: 0,
                logical_y: 0,
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
                floating_rect: None,
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

    fn host() -> CompositorHost<QuickJsPreparedLayoutRuntime<RuntimeProjectLayoutSourceLoader>> {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-host-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        CompositorHost::initialize(service, config(), state()).unwrap()
    }

    #[test]
    fn host_applies_bootstrap_scenario_and_exposes_trace() {
        let mut host = host();

        host.apply_bootstrap_scenario(
            BootstrapScenario::new()
                .register_output_snapshot(
                    OutputSnapshot {
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
                    },
                    false,
                )
                .register_seat("seat-1", true)
                .register_window_surface("window-w1", "w1", Some(OutputId::from("out-1")))
                .register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1")
                .move_surface_to_output("popup-1", "out-2"),
        )
        .unwrap();

        let trace = host.bootstrap_trace();
        assert_eq!(trace.applied_events.len(), 5);
        assert!(trace.diagnostics.seat_names.contains(&"seat-1".to_string()));
    }

    #[test]
    fn host_replays_bootstrap_transcript() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-host-transcript-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);
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

        let transcript = BootstrapTranscript::new(
            StartupRegistration {
                seats: vec!["seat-0".into(), "seat-1".into()],
                outputs: vec![OutputId::from("out-1"), OutputId::from("out-2")],
                active_seat: Some("seat-1".into()),
                active_output: Some(OutputId::from("out-2")),
            },
            BootstrapScenario::new()
                .register_seat("seat-1", true)
                .register_window_surface("window-w1", "w1", Some(OutputId::from("out-1")))
                .register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1")
                .move_surface_to_output("popup-1", "out-2"),
        );

        let mut host =
            CompositorHost::initialize_with_transcript(service, config(), snapshot, &transcript)
                .unwrap();
        host.apply_bootstrap_transcript(transcript).unwrap();

        let trace = host.bootstrap_trace();
        assert_eq!(trace.applied_events.len(), 4);
        assert!(trace.diagnostics.output_ids.contains(&"out-2".to_string()));
    }
}
