use spiders_config::model::Config;
use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_shared::runtime::AuthoringLayoutRuntime;
use spiders_shared::wm::StateSnapshot;
use spiders_wm::{
    BootstrapDiagnostics, BootstrapEvent, BootstrapFailureTrace, BootstrapRunTrace,
    BootstrapScenario, StartupRegistration,
};

use crate::app::CompositorApp;
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
pub struct BootstrapRunner<R> {
    app: CompositorApp<R>,
    applied_events: Vec<BootstrapEvent>,
}

impl<R> BootstrapRunner<R> {
    pub fn app(&self) -> &CompositorApp<R> {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut CompositorApp<R> {
        &mut self.app
    }

    pub fn into_app(self) -> CompositorApp<R> {
        self.app
    }

    pub fn diagnostics(&self) -> BootstrapDiagnostics {
        BootstrapDiagnostics {
            active_seat: self.app.topology().active_seat_name.clone(),
            active_output: self.app.topology().active_output_id.clone(),
            current_workspace: self
                .app
                .state()
                .current_workspace_id
                .as_ref()
                .map(ToString::to_string),
            focused_window: self
                .app
                .state()
                .focused_window_id
                .as_ref()
                .map(ToString::to_string),
            seat_names: self
                .app
                .topology()
                .seats
                .iter()
                .map(|seat| seat.name.clone())
                .collect(),
            output_ids: self
                .app
                .topology()
                .outputs
                .iter()
                .map(|output| output.snapshot.id.to_string())
                .collect(),
            surface_ids: self
                .app
                .topology()
                .surfaces
                .iter()
                .map(|surface| surface.id.clone())
                .collect(),
            mapped_surface_ids: self
                .app
                .topology()
                .surfaces
                .iter()
                .filter(|surface| surface.mapped)
                .map(|surface| surface.id.clone())
                .collect(),
            seat_count: self.app.topology().seats.len(),
            output_count: self.app.topology().outputs.len(),
            surface_count: self.app.topology().surfaces.len(),
            mapped_surface_count: self
                .app
                .topology()
                .surfaces
                .iter()
                .filter(|surface| surface.mapped)
                .count(),
        }
    }

    pub fn trace(&self) -> BootstrapRunTrace {
        BootstrapRunTrace {
            startup: self.app.startup.clone(),
            applied_events: self.applied_events.clone(),
            diagnostics: self.diagnostics(),
        }
    }

    pub fn failure_trace(
        &self,
        failed_event: Option<BootstrapEvent>,
        error: impl ToString,
    ) -> BootstrapFailureTrace {
        BootstrapFailureTrace {
            startup: self.app.startup.clone(),
            applied_events: self.applied_events.clone(),
            failed_event,
            diagnostics: Some(self.diagnostics()),
            error: error.to_string(),
        }
    }
}

impl<R: AuthoringLayoutRuntime<Config = Config>> BootstrapRunner<R> {
    pub fn initialize(
        layout_service: LayoutService,
        authoring_layout_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            app: CompositorApp::initialize(layout_service, authoring_layout_service, config, state)?,
            applied_events: Vec::new(),
        })
    }

    pub fn initialize_with_registration(
        layout_service: LayoutService,
        authoring_layout_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
        startup: StartupRegistration,
    ) -> Result<Self, BootstrapRunnerError> {
        Ok(Self {
            app: CompositorApp::initialize_with_registration(
                layout_service,
                authoring_layout_service,
                config,
                state,
                startup,
            )?,
            applied_events: Vec::new(),
        })
    }

    pub fn apply_event(&mut self, event: BootstrapEvent) -> Result<(), BootstrapRunnerError> {
        self.app.apply_bootstrap_event(event.clone())?;
        self.applied_events.push(event);
        Ok(())
    }

    pub fn apply_event_with_trace(
        &mut self,
        event: BootstrapEvent,
    ) -> Result<(), BootstrapFailureTrace> {
        self.apply_event(event.clone())
            .map_err(|error| self.failure_trace(Some(event), error))
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

    pub fn apply_events_with_trace<I>(&mut self, events: I) -> Result<(), BootstrapFailureTrace>
    where
        I: IntoIterator<Item = BootstrapEvent>,
    {
        for event in events {
            self.apply_event_with_trace(event)?;
        }
        Ok(())
    }

    pub fn apply_scenario(
        &mut self,
        scenario: BootstrapScenario,
    ) -> Result<(), BootstrapRunnerError> {
        self.apply_events(scenario.into_events())
    }

    pub fn apply_scenario_with_trace(
        &mut self,
        scenario: BootstrapScenario,
    ) -> Result<(), BootstrapFailureTrace> {
        self.apply_events_with_trace(scenario.into_events())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_config::authoring_layout::AuthoringLayoutService;
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

    fn runner() -> BootstrapRunner<QuickJsPreparedLayoutRuntime<RuntimeProjectLayoutSourceLoader>> {
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
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

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
                logical_x: 0,
                logical_y: 0,
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
        assert!(runner.app().topology().surface("popup-1").is_none());
        let trace = runner.trace();
        assert_eq!(trace.applied_events.len(), 5);
        assert_eq!(trace.diagnostics.active_seat.as_deref(), Some("seat-1"));
        assert_eq!(trace.diagnostics.current_workspace.as_deref(), Some("ws-1"));
        assert_eq!(trace.diagnostics.focused_window.as_deref(), Some("w1"));
        assert!(trace.diagnostics.seat_names.contains(&"seat-1".to_string()));
        assert!(trace.diagnostics.output_ids.contains(&"out-2".to_string()));
        assert!(!trace
            .diagnostics
            .surface_ids
            .contains(&"popup-1".to_string()));
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
        assert_eq!(
            runner.trace().startup.active_seat.as_deref(),
            Some("seat-a")
        );
    }

    #[test]
    fn runner_reports_failure_trace_with_partial_progress() {
        let mut runner = runner();

        let error = runner
            .apply_events_with_trace(vec![
                BootstrapEvent::RegisterSeat {
                    seat_name: "seat-1".into(),
                    active: true,
                },
                BootstrapEvent::RemoveOutput {
                    output_id: OutputId::from("missing-output"),
                },
            ])
            .unwrap_err();

        assert_eq!(error.applied_events.len(), 1);
        assert!(matches!(
            error.failed_event,
            Some(BootstrapEvent::RemoveOutput { .. })
        ));
        assert!(error.error.contains("output not found"));
        assert_eq!(
            error
                .diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.active_seat.as_deref()),
            Some("seat-1")
        );
    }

    #[test]
    fn runner_applies_in_memory_scenario() {
        let mut runner = runner();

        let scenario = BootstrapScenario::new()
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
            .move_surface_to_output("popup-1", "out-2")
            .unmap_surface("popup-1");

        runner.apply_scenario(scenario).unwrap();

        let trace = runner.trace();
        assert_eq!(trace.applied_events.len(), 6);
        assert!(trace.diagnostics.seat_names.contains(&"seat-1".to_string()));
        assert!(trace
            .diagnostics
            .surface_ids
            .contains(&"popup-1".to_string()));
        assert!(!trace
            .diagnostics
            .mapped_surface_ids
            .contains(&"popup-1".to_string()));
    }
}
