use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::wm::StateSnapshot;

use crate::app::{BootstrapEvent, StartupRegistration};
use crate::host::CompositorHost;
use crate::runner::{BootstrapFailureTrace, BootstrapRunTrace, BootstrapRunnerError};
use crate::scenario::BootstrapScenario;
use crate::script::BootstrapScript;
use crate::transcript::BootstrapTranscript;
use crate::CompositorApp;

#[derive(Debug)]
pub struct CompositorController<L, R> {
    host: CompositorHost<L, R>,
    phase: ControllerPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerPhase {
    Pending,
    Bootstrapping,
    Running,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ControllerReport {
    pub phase: ControllerPhase,
    pub startup: StartupRegistration,
    pub applied_events: usize,
    pub diagnostics: crate::BootstrapDiagnostics,
}

impl<L, R> CompositorController<L, R> {
    pub fn host(&self) -> &CompositorHost<L, R> {
        &self.host
    }

    pub fn app(&self) -> &CompositorApp<L, R> {
        self.host.app()
    }

    pub fn phase(&self) -> ControllerPhase {
        self.phase
    }

    pub fn bootstrap_trace(&self) -> BootstrapRunTrace {
        self.host.bootstrap_trace()
    }

    pub fn report(&self) -> ControllerReport {
        let trace = self.bootstrap_trace();
        ControllerReport {
            phase: self.phase,
            startup: trace.startup,
            applied_events: trace.applied_events.len(),
            diagnostics: trace.diagnostics,
        }
    }

    pub fn into_host(self) -> CompositorHost<L, R> {
        self.host
    }
}

impl<L: spiders_config::loader::LayoutSourceLoader, R: LayoutRuntime> CompositorController<L, R> {
    pub fn initialize(
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            host: CompositorHost::initialize(runtime_service, config, state)?,
            phase: ControllerPhase::Pending,
        })
    }

    pub fn initialize_with_registration(
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
        startup: StartupRegistration,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            host: CompositorHost::initialize_with_registration(
                runtime_service,
                config,
                state,
                startup,
            )?,
            phase: ControllerPhase::Pending,
        })
    }

    pub fn initialize_with_transcript(
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
        transcript: &BootstrapTranscript,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            host: CompositorHost::initialize_with_transcript(
                runtime_service,
                config,
                state,
                transcript,
            )?,
            phase: ControllerPhase::Pending,
        })
    }

    pub fn initialize_with_script(
        runtime_service: ConfigRuntimeService<L, R>,
        config: Config,
        state: StateSnapshot,
        script: &BootstrapScript,
    ) -> Result<Self, BootstrapRunnerError> {
        match script.startup() {
            Some(startup) => {
                Self::initialize_with_registration(runtime_service, config, state, startup.clone())
            }
            None => Self::initialize(runtime_service, config, state),
        }
    }

    pub fn apply_bootstrap_event(
        &mut self,
        event: BootstrapEvent,
    ) -> Result<(), BootstrapRunnerError> {
        self.phase = ControllerPhase::Bootstrapping;
        match self.host.apply_bootstrap_event(event) {
            Ok(()) => {
                self.phase = ControllerPhase::Running;
                Ok(())
            }
            Err(error) => {
                self.phase = ControllerPhase::Degraded;
                Err(error)
            }
        }
    }

    pub fn apply_bootstrap_scenario(
        &mut self,
        scenario: BootstrapScenario,
    ) -> Result<(), BootstrapRunnerError> {
        self.phase = ControllerPhase::Bootstrapping;
        match self.host.apply_bootstrap_scenario(scenario) {
            Ok(()) => {
                self.phase = ControllerPhase::Running;
                Ok(())
            }
            Err(error) => {
                self.phase = ControllerPhase::Degraded;
                Err(error)
            }
        }
    }

    pub fn apply_bootstrap_transcript(
        &mut self,
        transcript: BootstrapTranscript,
    ) -> Result<(), BootstrapFailureTrace> {
        self.phase = ControllerPhase::Bootstrapping;
        match self.host.apply_bootstrap_transcript(transcript) {
            Ok(()) => {
                self.phase = ControllerPhase::Running;
                Ok(())
            }
            Err(error) => {
                self.phase = ControllerPhase::Degraded;
                Err(error)
            }
        }
    }

    pub fn apply_bootstrap_script(
        &mut self,
        script: BootstrapScript,
    ) -> Result<(), BootstrapFailureTrace> {
        self.phase = ControllerPhase::Bootstrapping;
        let result = match script {
            BootstrapScript::Events(scenario) => {
                self.host.apply_bootstrap_scenario_with_trace(scenario)
            }
            BootstrapScript::Transcript(transcript) => {
                self.host.apply_bootstrap_transcript(transcript)
            }
        };

        match result {
            Ok(()) => {
                self.phase = ControllerPhase::Running;
                Ok(())
            }
            Err(error) => {
                self.phase = ControllerPhase::Degraded;
                Err(error)
            }
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

    fn runtime_service() -> ConfigRuntimeService<
        RuntimeProjectLayoutSourceLoader,
        BoaLayoutRuntime<RuntimeProjectLayoutSourceLoader>,
    > {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-controller-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        ConfigRuntimeService::new(loader, runtime)
    }

    #[test]
    fn controller_replays_event_script() {
        let script = BootstrapScript::Events(
            BootstrapScenario::new()
                .register_seat("seat-1", true)
                .register_window_surface("window-w1", "w1", Some(OutputId::from("out-1"))),
        );
        let mut controller = CompositorController::initialize_with_script(
            runtime_service(),
            config(),
            state(),
            &script,
        )
        .unwrap();

        controller.apply_bootstrap_script(script).unwrap();

        let trace = controller.bootstrap_trace();
        assert_eq!(trace.applied_events.len(), 2);
        assert_eq!(trace.diagnostics.active_seat.as_deref(), Some("seat-1"));
        assert_eq!(controller.phase(), ControllerPhase::Running);
    }

    #[test]
    fn controller_replays_transcript_script() {
        let script = BootstrapScript::Transcript(BootstrapTranscript::new(
            StartupRegistration {
                seats: vec!["seat-0".into(), "seat-1".into()],
                outputs: vec![OutputId::from("out-1")],
                active_seat: Some("seat-1".into()),
                active_output: Some(OutputId::from("out-1")),
            },
            BootstrapScenario::new().register_window_surface(
                "window-w1",
                "w1",
                Some(OutputId::from("out-1")),
            ),
        ));
        let mut controller = CompositorController::initialize_with_script(
            runtime_service(),
            config(),
            state(),
            &script,
        )
        .unwrap();

        controller.apply_bootstrap_script(script).unwrap();

        let trace = controller.bootstrap_trace();
        assert_eq!(trace.applied_events.len(), 1);
        assert_eq!(trace.startup.active_seat.as_deref(), Some("seat-1"));
        assert_eq!(controller.report().phase, ControllerPhase::Running);
    }

    #[test]
    fn controller_marks_degraded_on_failed_script() {
        let script = BootstrapScript::Events(BootstrapScenario::new().remove_output("missing"));
        let mut controller = CompositorController::initialize_with_script(
            runtime_service(),
            config(),
            state(),
            &script,
        )
        .unwrap();

        assert!(controller.apply_bootstrap_script(script).is_err());
        assert_eq!(controller.phase(), ControllerPhase::Degraded);
    }
}
