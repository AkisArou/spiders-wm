pub use spiders_wm::{
    BackendDiscoveryEvent, BackendOutputSnapshot, BackendSeatSnapshot, BackendSessionReport,
    BackendSessionState, BackendSnapshotSummary, BackendSource, BackendSurfaceSnapshot,
    BackendTopologySnapshot,
};

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WindowId};
    use spiders_shared::wm::OutputTransform;
    use spiders_wm::{
        BootstrapEvent, LayerExclusiveZone, LayerKeyboardInteractivity, LayerSurfaceMetadata,
        LayerSurfaceTier,
    };

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

        let layer_event = BackendDiscoveryEvent::LayerSurfaceDiscovered {
            surface_id: "layer-1".into(),
            output_id: OutputId::from("out-1"),
            metadata: LayerSurfaceMetadata {
                namespace: "panel".into(),
                tier: LayerSurfaceTier::Top,
                keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                exclusive_zone: LayerExclusiveZone::Exclusive(30),
            },
        };

        assert_eq!(
            layer_event.into_bootstrap_event(),
            BootstrapEvent::RegisterLayerSurface {
                surface_id: "layer-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(30),
                },
            }
        );

        let output_event = BackendDiscoveryEvent::OutputSnapshotDiscovered {
            output: spiders_shared::wm::OutputSnapshot {
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
            active: false,
        };

        assert!(matches!(
            output_event.into_bootstrap_event(),
            BootstrapEvent::RegisterOutputSnapshot { .. }
        ));
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
                snapshot: spiders_shared::wm::OutputSnapshot {
                    id: OutputId::from("out-1"),
                    name: "HDMI-A-1".into(),
                    logical_x: 0,
                    logical_y: 0,
                    logical_width: 1920,
                    logical_height: 1080,
                    scale: 1,
                    transform: OutputTransform::Normal,
                    enabled: true,
                    current_workspace_id: None,
                },
                active: true,
            }],
            surfaces: vec![
                BackendSurfaceSnapshot::Window {
                    surface_id: "window-w1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: Some(OutputId::from("out-1")),
                },
                BackendSurfaceSnapshot::Layer {
                    surface_id: "layer-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(30),
                    },
                },
                BackendSurfaceSnapshot::Unmanaged {
                    surface_id: "overlay-1".into(),
                },
            ],
        };

        let events = snapshot.into_discovery_events();

        assert_eq!(events.len(), 5);
        assert!(matches!(
            events[0],
            BackendDiscoveryEvent::SeatDiscovered { .. }
        ));
        assert!(matches!(
            events[3],
            BackendDiscoveryEvent::LayerSurfaceDiscovered { .. }
        ));
        assert!(matches!(
            events[4],
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
