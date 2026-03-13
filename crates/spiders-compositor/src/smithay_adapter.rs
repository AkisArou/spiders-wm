use crate::backend::{
    BackendDiscoveryEvent, BackendSeatSnapshot, BackendSource, BackendSurfaceSnapshot,
    BackendTopologySnapshot,
};
use spiders_runtime::ControllerCommand;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmithayAdapterEvent {
    Seat { seat_name: String, active: bool },
    SurfaceLost { surface_id: String },
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
            SmithayAdapterEvent::SurfaceLost { surface_id } => {
                ControllerCommand::DiscoveryEvent(BackendDiscoveryEvent::SurfaceLost { surface_id })
            }
        }
    }

    pub fn translate_snapshot(
        generation: u64,
        seats: Vec<BackendSeatSnapshot>,
        surfaces: Vec<BackendSurfaceSnapshot>,
    ) -> ControllerCommand {
        ControllerCommand::DiscoverySnapshot(BackendTopologySnapshot {
            source: BackendSource::Smithay,
            generation,
            seats,
            outputs: Vec::new(),
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
    fn adapter_translates_snapshot_into_smithay_sourced_batch() {
        let command = SmithayAdapter::translate_snapshot(
            4,
            vec![BackendSeatSnapshot {
                seat_name: "seat-0".into(),
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
                assert_eq!(snapshot.surfaces.len(), 1);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
