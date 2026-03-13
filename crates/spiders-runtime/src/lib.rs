pub mod session;
pub mod topology;
pub mod wm;

use spiders_shared::ids::{OutputId, WindowId};
use spiders_shared::wm::StateSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootstrapEvent {
    RegisterSeat {
        seat_name: String,
        active: bool,
    },
    RegisterOutput {
        output_id: OutputId,
        active: bool,
    },
    ActivateOutput {
        output_id: OutputId,
    },
    EnableOutput {
        output_id: OutputId,
    },
    DisableOutput {
        output_id: OutputId,
    },
    RemoveOutput {
        output_id: OutputId,
    },
    RegisterWindowSurface {
        surface_id: String,
        window_id: WindowId,
        output_id: Option<OutputId>,
    },
    RegisterPopupSurface {
        surface_id: String,
        output_id: Option<OutputId>,
        parent_surface_id: String,
    },
    RegisterLayerSurface {
        surface_id: String,
        output_id: OutputId,
    },
    RegisterUnmanagedSurface {
        surface_id: String,
    },
    RemoveSurface {
        surface_id: String,
    },
    RemoveWindowSurface {
        window_id: WindowId,
    },
    MoveSurfaceToOutput {
        surface_id: String,
        output_id: OutputId,
    },
    UnmapSurface {
        surface_id: String,
    },
    RemoveSeat {
        seat_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BootstrapScenario {
    events: Vec<BootstrapEvent>,
}

impl BootstrapScenario {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn from_events(events: Vec<BootstrapEvent>) -> Self {
        Self { events }
    }

    pub fn events(&self) -> &[BootstrapEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<BootstrapEvent> {
        self.events
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.events).unwrap()
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<Vec<BootstrapEvent>>(json).map(Self::from_events)
    }

    pub fn register_seat(mut self, seat_name: impl Into<String>, active: bool) -> Self {
        self.events.push(BootstrapEvent::RegisterSeat {
            seat_name: seat_name.into(),
            active,
        });
        self
    }

    pub fn register_output(mut self, output_id: impl Into<OutputId>, active: bool) -> Self {
        self.events.push(BootstrapEvent::RegisterOutput {
            output_id: output_id.into(),
            active,
        });
        self
    }

    pub fn register_window_surface(
        mut self,
        surface_id: impl Into<String>,
        window_id: impl Into<WindowId>,
        output_id: Option<OutputId>,
    ) -> Self {
        self.events.push(BootstrapEvent::RegisterWindowSurface {
            surface_id: surface_id.into(),
            window_id: window_id.into(),
            output_id,
        });
        self
    }

    pub fn register_popup_surface(
        mut self,
        surface_id: impl Into<String>,
        output_id: Option<OutputId>,
        parent_surface_id: impl Into<String>,
    ) -> Self {
        self.events.push(BootstrapEvent::RegisterPopupSurface {
            surface_id: surface_id.into(),
            output_id,
            parent_surface_id: parent_surface_id.into(),
        });
        self
    }

    pub fn register_layer_surface(
        mut self,
        surface_id: impl Into<String>,
        output_id: impl Into<OutputId>,
    ) -> Self {
        self.events.push(BootstrapEvent::RegisterLayerSurface {
            surface_id: surface_id.into(),
            output_id: output_id.into(),
        });
        self
    }

    pub fn register_unmanaged_surface(mut self, surface_id: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::RegisterUnmanagedSurface {
            surface_id: surface_id.into(),
        });
        self
    }

    pub fn move_surface_to_output(
        mut self,
        surface_id: impl Into<String>,
        output_id: impl Into<OutputId>,
    ) -> Self {
        self.events.push(BootstrapEvent::MoveSurfaceToOutput {
            surface_id: surface_id.into(),
            output_id: output_id.into(),
        });
        self
    }

    pub fn unmap_surface(mut self, surface_id: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::UnmapSurface {
            surface_id: surface_id.into(),
        });
        self
    }

    pub fn remove_window_surface(mut self, window_id: impl Into<WindowId>) -> Self {
        self.events.push(BootstrapEvent::RemoveWindowSurface {
            window_id: window_id.into(),
        });
        self
    }

    pub fn remove_surface(mut self, surface_id: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::RemoveSurface {
            surface_id: surface_id.into(),
        });
        self
    }

    pub fn remove_output(mut self, output_id: impl Into<OutputId>) -> Self {
        self.events.push(BootstrapEvent::RemoveOutput {
            output_id: output_id.into(),
        });
        self
    }

    pub fn remove_seat(mut self, seat_name: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::RemoveSeat {
            seat_name: seat_name.into(),
        });
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BootstrapTranscript {
    pub startup: StartupRegistration,
    pub scenario: BootstrapScenario,
}

impl BootstrapTranscript {
    pub fn new(startup: StartupRegistration, scenario: BootstrapScenario) -> Self {
        Self { startup, scenario }
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapScriptKind {
    Events,
    Transcript,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BootstrapScript {
    Events(BootstrapScenario),
    Transcript(BootstrapTranscript),
}

impl BootstrapScript {
    pub fn kind(&self) -> BootstrapScriptKind {
        match self {
            Self::Events(_) => BootstrapScriptKind::Events,
            Self::Transcript(_) => BootstrapScriptKind::Transcript,
        }
    }

    pub fn startup(&self) -> Option<&StartupRegistration> {
        match self {
            Self::Events(_) => None,
            Self::Transcript(transcript) => Some(&transcript.startup),
        }
    }

    pub fn scenario(&self) -> &BootstrapScenario {
        match self {
            Self::Events(scenario) => scenario,
            Self::Transcript(transcript) => &transcript.scenario,
        }
    }

    pub fn into_parts(self) -> (Option<StartupRegistration>, BootstrapScenario) {
        match self {
            Self::Events(scenario) => (None, scenario),
            Self::Transcript(transcript) => (Some(transcript.startup), transcript.scenario),
        }
    }

    pub fn to_json_pretty(&self) -> String {
        match self {
            Self::Events(scenario) => scenario.to_json_pretty(),
            Self::Transcript(transcript) => transcript.to_json_pretty(),
        }
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum BootstrapScriptRepr {
            Transcript(BootstrapTranscript),
            Events(Vec<BootstrapEvent>),
        }

        match serde_json::from_str::<BootstrapScriptRepr>(json)? {
            BootstrapScriptRepr::Transcript(transcript) => Ok(Self::Transcript(transcript)),
            BootstrapScriptRepr::Events(events) => {
                Ok(Self::Events(BootstrapScenario::from_events(events)))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendSource {
    Fixture,
    Mock,
    Smithay,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackendSeatSnapshot {
    pub seat_name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackendOutputSnapshot {
    pub output_id: OutputId,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendSurfaceSnapshot {
    Window {
        surface_id: String,
        window_id: WindowId,
        output_id: Option<OutputId>,
    },
    Popup {
        surface_id: String,
        output_id: Option<OutputId>,
        parent_surface_id: String,
    },
    Layer {
        surface_id: String,
        output_id: OutputId,
    },
    Unmanaged {
        surface_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackendTopologySnapshot {
    pub source: BackendSource,
    pub generation: u64,
    pub seats: Vec<BackendSeatSnapshot>,
    pub outputs: Vec<BackendOutputSnapshot>,
    pub surfaces: Vec<BackendSurfaceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackendSnapshotSummary {
    pub seat_count: usize,
    pub output_count: usize,
    pub surface_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BackendSessionReport {
    pub last_source: Option<BackendSource>,
    pub last_generation: Option<u64>,
    pub last_snapshot: Option<BackendSnapshotSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendSessionState {
    last_source: Option<BackendSource>,
    last_generation: Option<u64>,
    last_snapshot: Option<BackendSnapshotSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendDiscoveryEvent {
    SeatDiscovered {
        seat_name: String,
        active: bool,
    },
    SeatLost {
        seat_name: String,
    },
    OutputDiscovered {
        output_id: OutputId,
        active: bool,
    },
    OutputActivated {
        output_id: OutputId,
    },
    OutputLost {
        output_id: OutputId,
    },
    WindowSurfaceDiscovered {
        surface_id: String,
        window_id: WindowId,
        output_id: Option<OutputId>,
    },
    PopupSurfaceDiscovered {
        surface_id: String,
        output_id: Option<OutputId>,
        parent_surface_id: String,
    },
    LayerSurfaceDiscovered {
        surface_id: String,
        output_id: OutputId,
    },
    UnmanagedSurfaceDiscovered {
        surface_id: String,
    },
    SurfaceLost {
        surface_id: String,
    },
}

impl BackendDiscoveryEvent {
    pub fn into_bootstrap_event(self) -> BootstrapEvent {
        match self {
            Self::SeatDiscovered { seat_name, active } => {
                BootstrapEvent::RegisterSeat { seat_name, active }
            }
            Self::SeatLost { seat_name } => BootstrapEvent::RemoveSeat { seat_name },
            Self::OutputDiscovered { output_id, active } => {
                BootstrapEvent::RegisterOutput { output_id, active }
            }
            Self::OutputActivated { output_id } => BootstrapEvent::ActivateOutput { output_id },
            Self::OutputLost { output_id } => BootstrapEvent::RemoveOutput { output_id },
            Self::WindowSurfaceDiscovered {
                surface_id,
                window_id,
                output_id,
            } => BootstrapEvent::RegisterWindowSurface {
                surface_id,
                window_id,
                output_id,
            },
            Self::PopupSurfaceDiscovered {
                surface_id,
                output_id,
                parent_surface_id,
            } => BootstrapEvent::RegisterPopupSurface {
                surface_id,
                output_id,
                parent_surface_id,
            },
            Self::LayerSurfaceDiscovered {
                surface_id,
                output_id,
            } => BootstrapEvent::RegisterLayerSurface {
                surface_id,
                output_id,
            },
            Self::UnmanagedSurfaceDiscovered { surface_id } => {
                BootstrapEvent::RegisterUnmanagedSurface { surface_id }
            }
            Self::SurfaceLost { surface_id } => BootstrapEvent::RemoveSurface { surface_id },
        }
    }
}

impl BackendTopologySnapshot {
    pub fn summary(&self) -> BackendSnapshotSummary {
        BackendSnapshotSummary {
            seat_count: self.seats.len(),
            output_count: self.outputs.len(),
            surface_count: self.surfaces.len(),
        }
    }

    pub fn into_discovery_events(self) -> Vec<BackendDiscoveryEvent> {
        let mut events =
            Vec::with_capacity(self.seats.len() + self.outputs.len() + self.surfaces.len());

        events.extend(
            self.seats
                .into_iter()
                .map(|seat| BackendDiscoveryEvent::SeatDiscovered {
                    seat_name: seat.seat_name,
                    active: seat.active,
                }),
        );
        events.extend(self.outputs.into_iter().map(|output| {
            BackendDiscoveryEvent::OutputDiscovered {
                output_id: output.output_id,
                active: output.active,
            }
        }));
        events.extend(self.surfaces.into_iter().map(|surface| match surface {
            BackendSurfaceSnapshot::Window {
                surface_id,
                window_id,
                output_id,
            } => BackendDiscoveryEvent::WindowSurfaceDiscovered {
                surface_id,
                window_id,
                output_id,
            },
            BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id,
                parent_surface_id,
            } => BackendDiscoveryEvent::PopupSurfaceDiscovered {
                surface_id,
                output_id,
                parent_surface_id,
            },
            BackendSurfaceSnapshot::Layer {
                surface_id,
                output_id,
            } => BackendDiscoveryEvent::LayerSurfaceDiscovered {
                surface_id,
                output_id,
            },
            BackendSurfaceSnapshot::Unmanaged { surface_id } => {
                BackendDiscoveryEvent::UnmanagedSurfaceDiscovered { surface_id }
            }
        }));

        events
    }
}

impl BackendSessionState {
    pub fn report(&self) -> BackendSessionReport {
        BackendSessionReport {
            last_source: self.last_source.clone(),
            last_generation: self.last_generation,
            last_snapshot: self.last_snapshot.clone(),
        }
    }

    pub fn record_snapshot(&mut self, snapshot: &BackendTopologySnapshot) {
        self.last_source = Some(snapshot.source.clone());
        self.last_generation = Some(snapshot.generation);
        self.last_snapshot = Some(snapshot.summary());
    }

    pub fn record_batch(&mut self, source: BackendSource, generation: u64) {
        self.last_source = Some(source);
        self.last_generation = Some(generation);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BootstrapDiagnostics {
    pub active_seat: Option<String>,
    pub active_output: Option<OutputId>,
    pub current_workspace: Option<String>,
    pub focused_window: Option<String>,
    pub seat_names: Vec<String>,
    pub output_ids: Vec<String>,
    pub surface_ids: Vec<String>,
    pub mapped_surface_ids: Vec<String>,
    pub seat_count: usize,
    pub output_count: usize,
    pub surface_count: usize,
    pub mapped_surface_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BootstrapRunTrace {
    pub startup: StartupRegistration,
    pub applied_events: Vec<BootstrapEvent>,
    pub diagnostics: BootstrapDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BootstrapFailureTrace {
    pub startup: StartupRegistration,
    pub applied_events: Vec<BootstrapEvent>,
    pub failed_event: Option<BootstrapEvent>,
    pub diagnostics: Option<BootstrapDiagnostics>,
    pub error: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendSessionReport>,
    pub startup: StartupRegistration,
    pub applied_events: usize,
    pub diagnostics: BootstrapDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerCommand {
    BootstrapScript(BootstrapScript),
    BootstrapEvent(BootstrapEvent),
    DiscoveryEvent(BackendDiscoveryEvent),
    DiscoverySnapshot(BackendTopologySnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ControllerCommandReport {
    pub command: ControllerCommand,
    pub phase: ControllerPhase,
    pub controller: ControllerReport,
}

pub use session::{DomainSession, DomainSessionError, DomainUpdate};
pub use topology::{
    CompositorTopologyState, OutputState, SeatState, SurfaceRole, SurfaceState, TopologyError,
};
pub use wm::{WmState, WmStateError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_round_trips_through_json() {
        let scenario = BootstrapScenario::new()
            .register_seat("seat-1", true)
            .register_window_surface("window-w1", "w1", Some(OutputId::from("out-1")))
            .register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1");

        let json = scenario.to_json_pretty();
        let parsed = BootstrapScenario::from_json_str(&json).unwrap();

        assert_eq!(parsed, scenario);
    }

    #[test]
    fn transcript_round_trips_through_json() {
        let transcript = BootstrapTranscript::new(
            StartupRegistration {
                seats: vec!["seat-0".into(), "seat-1".into()],
                outputs: vec![OutputId::from("out-1")],
                active_seat: Some("seat-1".into()),
                active_output: Some(OutputId::from("out-1")),
            },
            BootstrapScenario::new()
                .register_seat("seat-1", true)
                .register_output("out-1", true),
        );

        let json = transcript.to_json_pretty();
        let parsed = BootstrapTranscript::from_json_str(&json).unwrap();

        assert_eq!(parsed, transcript);
    }

    #[test]
    fn script_round_trips_transcripts() {
        let script = BootstrapScript::Transcript(BootstrapTranscript::new(
            StartupRegistration {
                seats: vec!["seat-0".into(), "seat-1".into()],
                outputs: vec![OutputId::from("out-1")],
                active_seat: Some("seat-1".into()),
                active_output: Some(OutputId::from("out-1")),
            },
            BootstrapScenario::new().register_output("out-1", true),
        ));

        let parsed = BootstrapScript::from_json_str(&script.to_json_pretty()).unwrap();

        assert_eq!(parsed, script);
    }

    #[test]
    fn topology_snapshot_expands_into_discovery_events() {
        let snapshot = BackendTopologySnapshot {
            source: BackendSource::Fixture,
            generation: 7,
            seats: vec![BackendSeatSnapshot {
                seat_name: "seat-1".into(),
                active: true,
            }],
            outputs: vec![BackendOutputSnapshot {
                output_id: OutputId::from("out-1"),
                active: true,
            }],
            surfaces: vec![
                BackendSurfaceSnapshot::Window {
                    surface_id: "window-w1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: Some(OutputId::from("out-1")),
                },
                BackendSurfaceSnapshot::Unmanaged {
                    surface_id: "overlay-1".into(),
                },
            ],
        };

        let events = snapshot.into_discovery_events();

        assert_eq!(events.len(), 4);
        assert!(matches!(
            events[0],
            BackendDiscoveryEvent::SeatDiscovered { .. }
        ));
    }

    #[test]
    fn backend_session_records_snapshot_generation() {
        let snapshot = BackendTopologySnapshot {
            source: BackendSource::Mock,
            generation: 3,
            seats: vec![BackendSeatSnapshot {
                seat_name: "seat-1".into(),
                active: true,
            }],
            outputs: vec![],
            surfaces: vec![],
        };
        let mut session = BackendSessionState::default();

        session.record_snapshot(&snapshot);

        let report = session.report();
        assert_eq!(report.last_source, Some(BackendSource::Mock));
        assert_eq!(report.last_generation, Some(3));
        assert_eq!(report.last_snapshot.unwrap().seat_count, 1);
    }
}
