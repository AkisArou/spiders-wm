pub use spiders_runtime::{
    BackendDiscoveryEvent, BackendOutputSnapshot, BackendSeatSnapshot, BackendSessionReport,
    BackendSessionState, BackendSnapshotSummary, BackendSource, BackendSurfaceSnapshot,
    BackendTopologySnapshot,
};

#[cfg(test)]
mod tests {
    use spiders_runtime::BootstrapEvent;
    use spiders_shared::ids::{OutputId, WindowId};

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
        assert!(matches!(
            events[3],
            BackendDiscoveryEvent::UnmanagedSurfaceDiscovered { .. }
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
