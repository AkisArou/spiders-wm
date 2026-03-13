use spiders_shared::ids::{OutputId, WindowId};

use crate::app::BootstrapEvent;

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
    pub seats: Vec<BackendSeatSnapshot>,
    pub outputs: Vec<BackendOutputSnapshot>,
    pub surfaces: Vec<BackendSurfaceSnapshot>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_event_converts_into_bootstrap_event() {
        let event = BackendDiscoveryEvent::SeatDiscovered {
            seat_name: "seat-1".into(),
            active: true,
        };

        assert_eq!(
            event.into_bootstrap_event(),
            BootstrapEvent::RegisterSeat {
                seat_name: "seat-1".into(),
                active: true,
            }
        );
    }

    #[test]
    fn topology_snapshot_expands_into_discovery_events() {
        let snapshot = BackendTopologySnapshot {
            source: BackendSource::Fixture,
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
        assert!(matches!(
            events[3],
            BackendDiscoveryEvent::UnmanagedSurfaceDiscovered { .. }
        ));
    }
}
