use spiders_config::model::Config;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::api::WmAction;
use spiders_shared::ids::{OutputId, WorkspaceId};
use spiders_shared::runtime::AuthoringLayoutRuntime;
use spiders_shared::wm::StateSnapshot;
use spiders_wm::{
    BootstrapEvent, BootstrapFailureTrace, BootstrapRunTrace, BootstrapScenario, BootstrapScript,
    BootstrapTranscript, ControllerCommand, ControllerCommandReport, ControllerPhase,
    ControllerReport, StartupRegistration,
};

use crate::backend::{
    BackendDiscoveryEvent, BackendSessionState, BackendSource, BackendTopologySnapshot,
};
use crate::host::CompositorHost;
use crate::runner::BootstrapRunnerError;
use crate::CompositorApp;

#[derive(Debug)]
pub struct CompositorController<R> {
    host: CompositorHost<R>,
    phase: ControllerPhase,
    backend: BackendSessionState,
}

impl<R> CompositorController<R> {
    pub fn host(&self) -> &CompositorHost<R> {
        &self.host
    }

    pub fn app(&self) -> &CompositorApp<R> {
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
            backend: Some(self.backend.report()),
            startup: trace.startup,
            applied_events: trace.applied_events.len(),
            diagnostics: trace.diagnostics,
        }
    }

    pub fn into_host(self) -> CompositorHost<R> {
        self.host
    }

    pub fn state_snapshot(&self) -> StateSnapshot {
        self.host.app().state().clone()
    }
}

impl<R: AuthoringLayoutRuntime<Config = Config>> CompositorController<R> {
    pub fn initialize(
        runtime_service: ConfigRuntimeService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            host: CompositorHost::initialize(runtime_service, config, state)?,
            phase: ControllerPhase::Pending,
            backend: BackendSessionState::default(),
        })
    }

    pub fn initialize_with_registration(
        runtime_service: ConfigRuntimeService<R>,
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
            backend: BackendSessionState::default(),
        })
    }

    pub fn initialize_with_transcript(
        runtime_service: ConfigRuntimeService<R>,
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
            backend: BackendSessionState::default(),
        })
    }

    pub fn initialize_with_script(
        runtime_service: ConfigRuntimeService<R>,
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

    pub fn apply_discovery_event(
        &mut self,
        event: BackendDiscoveryEvent,
    ) -> Result<(), BootstrapRunnerError> {
        self.backend.record_batch(BackendSource::Mock, 0);
        self.apply_bootstrap_event(event.into_bootstrap_event())
    }

    pub fn apply_discovery_snapshot(
        &mut self,
        snapshot: BackendTopologySnapshot,
    ) -> Result<(), BootstrapRunnerError> {
        self.phase = ControllerPhase::Bootstrapping;
        self.backend.record_snapshot(&snapshot);

        for event in snapshot.into_discovery_events() {
            if let Err(error) = self
                .host
                .apply_bootstrap_event(event.into_bootstrap_event())
            {
                self.phase = ControllerPhase::Degraded;
                return Err(error);
            }
        }

        self.phase = ControllerPhase::Running;
        Ok(())
    }

    pub fn apply_command(
        &mut self,
        command: ControllerCommand,
    ) -> Result<ControllerCommandReport, ControllerCommandError> {
        match command.clone() {
            ControllerCommand::BootstrapScript(script) => self
                .apply_bootstrap_script(script)
                .map_err(ControllerCommandError::BootstrapFailure)?,
            ControllerCommand::BootstrapEvent(event) => self
                .apply_bootstrap_event(event)
                .map_err(ControllerCommandError::Runner)?,
            ControllerCommand::DiscoveryEvent(event) => self
                .apply_discovery_event(event)
                .map_err(ControllerCommandError::Runner)?,
            ControllerCommand::DiscoverySnapshot(snapshot) => self
                .apply_discovery_snapshot(snapshot)
                .map_err(ControllerCommandError::Runner)?,
        }

        Ok(ControllerCommandReport {
            command,
            phase: self.phase,
            controller: self.report(),
        })
    }

    pub fn apply_ipc_action(
        &mut self,
        action: &WmAction,
    ) -> Result<crate::session::SessionUpdate, crate::actions::ActionError> {
        let update = self
            .host
            .runner_mut()
            .app_mut()
            .session
            .apply_action(action)?;
        self.phase = ControllerPhase::Running;
        Ok(update)
    }

    pub fn activate_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Result<crate::session::SessionUpdate, crate::actions::ActionError> {
        self.apply_ipc_action(&WmAction::ActivateWorkspace {
            workspace_id: workspace_id.clone(),
        })
    }

    pub fn assign_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
        output_id: &OutputId,
    ) -> Result<crate::session::SessionUpdate, crate::actions::ActionError> {
        self.apply_ipc_action(&WmAction::AssignWorkspace {
            workspace_id: workspace_id.clone(),
            output_id: output_id.clone(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ControllerCommandError {
    #[error(transparent)]
    Runner(#[from] BootstrapRunnerError),
    #[error("bootstrap command failed")]
    BootstrapFailure(BootstrapFailureTrace),
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_config::service::ConfigRuntimeService;
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::BoaLayoutRuntime;
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
                runtime_source: None,
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

    fn runtime_service() -> ConfigRuntimeService<BoaLayoutRuntime<RuntimeProjectLayoutSourceLoader>>
    {
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
        ConfigRuntimeService::new(runtime)
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

    #[test]
    fn controller_accepts_backend_discovery_event() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::SeatDiscovered {
                seat_name: "seat-backend".into(),
                active: true,
            })
            .unwrap();

        assert_eq!(controller.phase(), ControllerPhase::Running);
        assert_eq!(
            controller
                .bootstrap_trace()
                .diagnostics
                .active_seat
                .as_deref(),
            Some("seat-backend")
        );
    }

    #[test]
    fn controller_command_returns_command_report() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        let report = controller
            .apply_command(ControllerCommand::DiscoveryEvent(
                BackendDiscoveryEvent::OutputActivated {
                    output_id: OutputId::from("out-1"),
                },
            ))
            .unwrap();

        assert_eq!(report.phase, ControllerPhase::Running);
        assert_eq!(report.controller.applied_events, 1);
    }

    #[test]
    fn controller_accepts_surface_unmapped_discovery_event() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::WindowSurfaceDiscovered {
                surface_id: "window-w1".into(),
                window_id: WindowId::from("w1"),
                output_id: Some(OutputId::from("out-1")),
            })
            .unwrap();
        controller
            .apply_discovery_event(BackendDiscoveryEvent::SurfaceUnmapped {
                surface_id: "window-w1".into(),
            })
            .unwrap();

        let surface = controller
            .app()
            .session()
            .topology()
            .surface("window-w1")
            .unwrap();
        assert!(!surface.mapped);
        assert!(controller
            .app()
            .session()
            .topology()
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
    }

    #[test]
    fn controller_accepts_seat_focus_discovery_event() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::SeatDiscovered {
                seat_name: "seat-backend".into(),
                active: true,
            })
            .unwrap();
        controller
            .apply_discovery_event(BackendDiscoveryEvent::SeatFocusChanged {
                seat_name: "seat-backend".into(),
                window_id: Some(WindowId::from("w-focused")),
                output_id: Some(OutputId::from("out-1")),
            })
            .unwrap();

        let seat = controller
            .app()
            .session()
            .topology()
            .seat("seat-backend")
            .unwrap();
        assert_eq!(seat.focused_window_id, Some(WindowId::from("w-focused")));
        assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));
    }

    #[test]
    fn controller_cascades_parent_surface_unmap_to_popup_children() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::WindowSurfaceDiscovered {
                surface_id: "window-w1".into(),
                window_id: WindowId::from("w1"),
                output_id: Some(OutputId::from("out-1")),
            })
            .unwrap();
        controller
            .apply_discovery_event(BackendDiscoveryEvent::PopupSurfaceDiscovered {
                surface_id: "popup-w1".into(),
                output_id: Some(OutputId::from("out-1")),
                parent_surface_id: "window-w1".into(),
            })
            .unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::SurfaceUnmapped {
                surface_id: "window-w1".into(),
            })
            .unwrap();

        let topology = controller.app().session().topology();
        assert!(!topology.surface("window-w1").unwrap().mapped);
        assert!(!topology.surface("popup-w1").unwrap().mapped);
    }

    #[test]
    fn controller_cascades_parent_surface_removal_to_popup_children() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::WindowSurfaceDiscovered {
                surface_id: "window-w1".into(),
                window_id: WindowId::from("w1"),
                output_id: Some(OutputId::from("out-1")),
            })
            .unwrap();
        controller
            .apply_discovery_event(BackendDiscoveryEvent::PopupSurfaceDiscovered {
                surface_id: "popup-w1".into(),
                output_id: Some(OutputId::from("out-1")),
                parent_surface_id: "window-w1".into(),
            })
            .unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::SurfaceLost {
                surface_id: "window-w1".into(),
            })
            .unwrap();

        let topology = controller.app().session().topology();
        assert!(topology.surface("window-w1").is_none());
        assert!(topology.surface("popup-w1").is_none());
    }

    #[test]
    fn controller_preserves_layer_output_for_popup_parented_to_layer_surface() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_event(BackendDiscoveryEvent::LayerSurfaceDiscovered {
                surface_id: "layer-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: spiders_wm::LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: spiders_wm::LayerSurfaceTier::Top,
                    keyboard_interactivity: spiders_wm::LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: spiders_wm::LayerExclusiveZone::Exclusive(24),
                },
            })
            .unwrap();
        controller
            .apply_discovery_event(BackendDiscoveryEvent::PopupSurfaceDiscovered {
                surface_id: "popup-layer-1".into(),
                output_id: Some(OutputId::from("out-1")),
                parent_surface_id: "layer-1".into(),
            })
            .unwrap();

        let topology = controller.app().session().topology();
        let popup = topology.surface("popup-layer-1").unwrap();
        assert_eq!(popup.parent_surface_id.as_deref(), Some("layer-1"));
        assert_eq!(popup.output_id, Some(OutputId::from("out-1")));
    }

    #[test]
    fn controller_accepts_backend_topology_snapshot() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_snapshot(BackendTopologySnapshot {
                source: crate::backend::BackendSource::Mock,
                generation: 0,
                seats: vec![crate::backend::BackendSeatSnapshot {
                    seat_name: "seat-batch".into(),
                    active: true,
                }],
                outputs: vec![crate::backend::BackendOutputSnapshot {
                    snapshot: OutputSnapshot {
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
                    },
                    active: true,
                }],
                surfaces: vec![crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "window-w1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: Some(OutputId::from("out-1")),
                }],
            })
            .unwrap();

        let report = controller.report();
        assert_eq!(report.phase, ControllerPhase::Running);
        assert_eq!(
            report
                .backend
                .as_ref()
                .and_then(|backend| backend.last_source.clone()),
            Some(BackendSource::Mock)
        );
        assert_eq!(
            report
                .backend
                .as_ref()
                .and_then(|backend| backend.last_generation),
            Some(0)
        );
        assert_eq!(report.applied_events, 3);
        assert!(report
            .diagnostics
            .seat_names
            .iter()
            .any(|seat| seat == "seat-batch"));
    }

    #[test]
    fn controller_report_tracks_backend_snapshot_metadata() {
        let mut controller =
            CompositorController::initialize(runtime_service(), config(), state()).unwrap();

        controller
            .apply_discovery_snapshot(BackendTopologySnapshot {
                source: BackendSource::Smithay,
                generation: 9,
                seats: vec![crate::backend::BackendSeatSnapshot {
                    seat_name: "seat-smithay".into(),
                    active: true,
                }],
                outputs: vec![],
                surfaces: vec![],
            })
            .unwrap();

        let report = controller.report();
        assert_eq!(
            report
                .backend
                .as_ref()
                .and_then(|backend| backend.last_source.clone()),
            Some(BackendSource::Smithay)
        );
        assert_eq!(
            report
                .backend
                .as_ref()
                .and_then(|backend| backend.last_generation),
            Some(9)
        );
        assert_eq!(
            report
                .backend
                .as_ref()
                .and_then(|backend| backend.last_snapshot.clone())
                .unwrap()
                .seat_count,
            1
        );
    }
}
