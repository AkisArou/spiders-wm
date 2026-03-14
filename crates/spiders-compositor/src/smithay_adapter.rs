use crate::backend::{
    BackendDiscoveryEvent, BackendOutputSnapshot, BackendSeatSnapshot, BackendSource,
    BackendSurfaceSnapshot, BackendTopologySnapshot,
};
use spiders_runtime::ControllerCommand;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmithayAdapterEvent {
    Seat {
        seat_name: String,
        active: bool,
    },
    SeatLost {
        seat_name: String,
    },
    SeatFocusChanged {
        seat_name: String,
        window_id: Option<String>,
        output_id: Option<String>,
    },
    Output {
        output_id: String,
        active: bool,
    },
    OutputActivated {
        output_id: String,
    },
    OutputLost {
        output_id: String,
    },
    SurfaceUnmapped {
        surface_id: String,
    },
    SurfaceLost {
        surface_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SmithaySeatDescriptor {
    pub seat_name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SmithayOutputDescriptor {
    pub output_id: String,
    pub active: bool,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SmithayAdapter;

impl SmithayAdapter {
    pub fn translate_event(event: SmithayAdapterEvent) -> ControllerCommand {
        match event {
            SmithayAdapterEvent::Seat { seat_name, active } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SeatDiscovered {
                    seat_name,
                    active,
                })
            }
            SmithayAdapterEvent::SeatLost { seat_name } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SeatLost { seat_name })
            }
            SmithayAdapterEvent::SeatFocusChanged {
                seat_name,
                window_id,
                output_id,
            } => ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SeatFocusChanged {
                seat_name,
                window_id: window_id.map(Into::into),
                output_id: output_id.map(Into::into),
            }),
            SmithayAdapterEvent::Output { output_id, active } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::OutputDiscovered {
                    output_id: output_id.into(),
                    active,
                })
            }
            SmithayAdapterEvent::OutputActivated { output_id } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::OutputActivated {
                    output_id: output_id.into(),
                })
            }
            SmithayAdapterEvent::OutputLost { output_id } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::OutputLost {
                    output_id: output_id.into(),
                })
            }
            SmithayAdapterEvent::SurfaceUnmapped { surface_id } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SurfaceUnmapped {
                    surface_id,
                })
            }
            SmithayAdapterEvent::SurfaceLost { surface_id } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SurfaceLost { surface_id })
            }
        }
    }

    pub fn translate_seat_descriptor(seat: SmithaySeatDescriptor) -> BackendSeatSnapshot {
        BackendSeatSnapshot {
            seat_name: seat.seat_name,
            active: seat.active,
        }
    }

    pub fn translate_output_descriptor(output: SmithayOutputDescriptor) -> BackendOutputSnapshot {
        let _ = (output.width, output.height);
        BackendOutputSnapshot {
            output_id: output.output_id.into(),
            active: output.active,
        }
    }

    pub fn translate_snapshot(
        generation: u64,
        seats: Vec<BackendSeatSnapshot>,
        outputs: Vec<BackendOutputSnapshot>,
        surfaces: Vec<BackendSurfaceSnapshot>,
    ) -> ControllerCommand {
        ControllerCommand::DiscoverySnapshot(BackendTopologySnapshot {
            source: BackendSource::Smithay,
            generation,
            seats,
            outputs,
            surfaces,
        })
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::WindowId;

    use super::*;

    #[test]
    fn adapter_translates_seat_event_into_controller_command() {
        let command = SmithayAdapter::translate_event(SmithayAdapterEvent::Seat {
            seat_name: "seat-0".into(),
            active: true,
        });

        assert!(matches!(
            command,
            ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SeatDiscovered { .. })
        ));
    }

    #[test]
    fn adapter_translates_output_event_into_controller_command() {
        let command = SmithayAdapter::translate_event(SmithayAdapterEvent::Output {
            output_id: "out-1".into(),
            active: true,
        });

        assert!(matches!(
            command,
            ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::OutputDiscovered { .. })
        ));
    }

    #[test]
    fn adapter_translates_seat_lost_event_into_controller_command() {
        let command = SmithayAdapter::translate_event(SmithayAdapterEvent::SeatLost {
            seat_name: "seat-0".into(),
        });

        assert!(matches!(
            command,
            ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SeatLost { .. })
        ));
    }

    #[test]
    fn adapter_translates_seat_focus_event_into_controller_command() {
        let command = SmithayAdapter::translate_event(SmithayAdapterEvent::SeatFocusChanged {
            seat_name: "seat-0".into(),
            window_id: Some("w1".into()),
            output_id: Some("out-1".into()),
        });

        assert!(matches!(
            command,
            ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SeatFocusChanged { .. })
        ));
    }

    #[test]
    fn adapter_translates_output_activation_event_into_controller_command() {
        let command = SmithayAdapter::translate_event(SmithayAdapterEvent::OutputActivated {
            output_id: "out-1".into(),
        });

        assert!(matches!(
            command,
            ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::OutputActivated { .. })
        ));
    }

    #[test]
    fn adapter_translates_output_lost_event_into_controller_command() {
        let command = SmithayAdapter::translate_event(SmithayAdapterEvent::OutputLost {
            output_id: "out-1".into(),
        });

        assert!(matches!(
            command,
            ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::OutputLost { .. })
        ));
    }

    #[test]
    fn adapter_translates_surface_unmapped_event_into_controller_command() {
        let command = SmithayAdapter::translate_event(SmithayAdapterEvent::SurfaceUnmapped {
            surface_id: "surface-1".into(),
        });

        assert!(matches!(
            command,
            ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SurfaceUnmapped { .. })
        ));
    }

    #[test]
    fn adapter_translates_descriptors_into_backend_snapshots() {
        let seat = SmithayAdapter::translate_seat_descriptor(SmithaySeatDescriptor {
            seat_name: "seat-0".into(),
            active: true,
        });
        let output = SmithayAdapter::translate_output_descriptor(SmithayOutputDescriptor {
            output_id: "out-1".into(),
            active: true,
            width: 1280,
            height: 720,
        });

        assert_eq!(seat.seat_name, "seat-0");
        assert_eq!(
            output.output_id,
            spiders_shared::ids::OutputId::from("out-1")
        );
    }

    #[test]
    fn adapter_translates_snapshot_into_smithay_sourced_batch() {
        let command = SmithayAdapter::translate_snapshot(
            4,
            vec![BackendSeatSnapshot {
                seat_name: "seat-0".into(),
                active: true,
            }],
            vec![BackendOutputSnapshot {
                output_id: spiders_shared::ids::OutputId::from("out-1"),
                active: true,
            }],
            vec![BackendSurfaceSnapshot::Window {
                surface_id: "window-w1".into(),
                window_id: WindowId::from("w1"),
                output_id: None,
            }],
        );

        match command {
            ControllerCommand::DiscoverySnapshot(snapshot) => {
                assert_eq!(snapshot.source, BackendSource::Smithay);
                assert_eq!(snapshot.generation, 4);
                assert_eq!(snapshot.seats.len(), 1);
                assert_eq!(snapshot.outputs.len(), 1);
                assert_eq!(snapshot.surfaces.len(), 1);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
