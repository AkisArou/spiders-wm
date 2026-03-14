pub use spiders_runtime::{
    CompositorTopologyState, OutputState, SeatState, SurfaceRole, SurfaceState, TopologyError,
};

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot};

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
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
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: None,
            }],
            windows: vec![],
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn topology_initializes_outputs_from_snapshot() {
        let topology = CompositorTopologyState::from_snapshot(&state());

        assert_eq!(topology.outputs.len(), 1);
        assert_eq!(topology.outputs[0].snapshot.id, OutputId::from("out-1"));
    }

    #[test]
    fn topology_maps_window_surfaces_per_output() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());

        topology
            .map_window_surface(
                "surface-1",
                WindowId::from("w1"),
                Some(OutputId::from("out-1")),
            )
            .unwrap();

        assert_eq!(
            topology
                .output(&OutputId::from("out-1"))
                .unwrap()
                .mapped_surface_ids,
            vec!["surface-1".to_string()]
        );
        assert_eq!(
            topology.surface("surface-1").unwrap().window_id,
            Some(WindowId::from("w1"))
        );
    }

    #[test]
    fn topology_tracks_seat_focus() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology.register_seat("seat-0");

        topology
            .focus_seat_window(
                "seat-0",
                Some(WindowId::from("w1")),
                Some(OutputId::from("out-1")),
            )
            .unwrap();

        let seat = topology.seat("seat-0").unwrap();
        assert_eq!(seat.focused_window_id, Some(WindowId::from("w1")));
        assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));
        assert!(seat.active);
    }

    #[test]
    fn topology_activates_and_disables_outputs() {
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
        let mut topology = CompositorTopologyState::from_snapshot(&snapshot);

        topology.activate_output(&OutputId::from("out-2")).unwrap();
        topology.disable_output(&OutputId::from("out-2")).unwrap();
        topology.enable_output(&OutputId::from("out-2")).unwrap();

        assert_eq!(topology.active_output_id, Some(OutputId::from("out-1")));
        assert!(
            topology
                .output(&OutputId::from("out-2"))
                .unwrap()
                .snapshot
                .enabled
        );
    }

    #[test]
    fn topology_tracks_active_seat_selection() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology.register_seat("seat-0");
        topology.register_seat("seat-1");

        topology.activate_seat("seat-1").unwrap();

        assert_eq!(topology.active_seat_name.as_deref(), Some("seat-1"));
        assert!(topology.seat("seat-1").unwrap().active);
        assert!(!topology.seat("seat-0").unwrap().active);
    }

    #[test]
    fn topology_registers_popup_and_unmanaged_surfaces() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());

        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                Some("window-w1".into()),
            )
            .unwrap();
        topology
            .register_surface("overlay-1", SurfaceRole::Unmanaged, None, None)
            .unwrap();

        assert_eq!(
            topology.surface("popup-1").unwrap().role,
            SurfaceRole::Popup
        );
        assert_eq!(
            topology
                .surface("popup-1")
                .unwrap()
                .parent_surface_id
                .as_deref(),
            Some("window-w1")
        );
        assert_eq!(
            topology.surface("overlay-1").unwrap().role,
            SurfaceRole::Unmanaged
        );
    }

    #[test]
    fn topology_updates_surface_output_attachment() {
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
        let mut topology = CompositorTopologyState::from_snapshot(&snapshot);
        topology
            .register_surface(
                "layer-1",
                SurfaceRole::Layer,
                Some(OutputId::from("out-1")),
                None,
            )
            .unwrap();

        topology
            .update_surface_attachment("layer-1", Some(OutputId::from("out-2")))
            .unwrap();

        assert_eq!(
            topology.surface("layer-1").unwrap().output_id,
            Some(OutputId::from("out-2"))
        );
        assert!(topology
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
        assert_eq!(
            topology
                .output(&OutputId::from("out-2"))
                .unwrap()
                .mapped_surface_ids,
            vec!["layer-1".to_string()]
        );
    }

    #[test]
    fn topology_unregisters_output_and_clears_links() {
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
        let mut topology = CompositorTopologyState::from_snapshot(&snapshot);
        topology.register_seat("seat-0");
        topology.activate_output(&OutputId::from("out-2")).unwrap();
        topology
            .focus_seat_window(
                "seat-0",
                Some(WindowId::from("w1")),
                Some(OutputId::from("out-2")),
            )
            .unwrap();
        topology
            .register_surface(
                "layer-1",
                SurfaceRole::Layer,
                Some(OutputId::from("out-2")),
                None,
            )
            .unwrap();

        topology
            .unregister_output(&OutputId::from("out-2"))
            .unwrap();

        assert!(topology.output(&OutputId::from("out-2")).is_none());
        assert_eq!(topology.active_output_id, Some(OutputId::from("out-1")));
        assert_eq!(topology.surface("layer-1").unwrap().output_id, None);
        assert_eq!(
            topology.seat("seat-0").unwrap().focused_output_id,
            Some(OutputId::from("out-1"))
        );
    }

    #[test]
    fn topology_unregisters_seats_and_surfaces() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology.register_seat("seat-0");
        topology.activate_seat("seat-0").unwrap();
        topology
            .map_window_surface(
                "window-w1",
                WindowId::from("w1"),
                Some(OutputId::from("out-1")),
            )
            .unwrap();

        topology.unregister_seat("seat-0").unwrap();
        topology
            .unregister_window_surface(&WindowId::from("w1"))
            .unwrap();

        assert!(topology.seat("seat-0").is_none());
        assert!(topology.surface("window-w1").is_none());
        assert_eq!(topology.active_seat_name, None);
    }

    #[test]
    fn topology_unregisters_generic_surfaces() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                None,
            )
            .unwrap();

        topology.unregister_surface("popup-1").unwrap();

        assert!(topology.surface("popup-1").is_none());
        assert!(topology
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
    }
}
