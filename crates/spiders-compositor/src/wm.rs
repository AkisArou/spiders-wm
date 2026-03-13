pub use spiders_runtime::{WmState, WmStateError};

#[cfg(test)]
mod tests {
    use spiders_shared::api::CompositorEvent;
    use spiders_shared::api::FocusDirection;
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![
                WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-1"),
                    name: "1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["1".into()],
                    focused: true,
                    visible: true,
                    effective_layout: Some(LayoutRef {
                        name: "master-stack".into(),
                    }),
                },
                WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-2"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["2".into()],
                    focused: false,
                    visible: false,
                    effective_layout: Some(LayoutRef {
                        name: "stack".into(),
                    }),
                },
            ],
            windows: vec![
                WindowSnapshot {
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
                },
                WindowSnapshot {
                    id: WindowId::from("w2"),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("alacritty".into()),
                    title: Some("Terminal".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: false,
                    fullscreen: false,
                    focused: false,
                    urgent: false,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-2")),
                    tags: vec!["2".into()],
                },
            ],
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into(), "2".into()],
        }
    }

    #[test]
    fn wm_state_focuses_window_and_updates_current_workspace() {
        let mut state = WmState::from_snapshot(state());

        let event = state.focus_window(&WindowId::from("w2")).unwrap();

        assert_eq!(
            state.snapshot().focused_window_id,
            Some(WindowId::from("w2"))
        );
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            state.snapshot().current_output_id,
            Some(OutputId::from("out-1"))
        );
        assert!(matches!(
            event,
            CompositorEvent::FocusChange {
                focused_window_id: Some(_),
                current_workspace_id: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn wm_state_views_tag_and_recomputes_visibility() {
        let mut state = WmState::from_snapshot(state());

        let events = state
            .view_tag_on_output(&OutputId::from("out-1"), "2")
            .unwrap();

        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
        assert_eq!(
            state.snapshot().visible_window_ids,
            vec![WindowId::from("w2")]
        );
        assert_eq!(state.snapshot().focused_window_id, None);
        assert!(events.iter().any(|event| matches!(
            event,
            CompositorEvent::TagChange {
                workspace_id: Some(id),
                ..
            } if id == &WorkspaceId::from("ws-2")
        )));
    }

    #[test]
    fn wm_state_maps_and_destroys_windows() {
        let mut state = WmState::from_snapshot(state());
        let event = state.map_window(WindowSnapshot {
            id: WindowId::from("w3"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("discord".into()),
            title: Some("Discord".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: false,
            floating: false,
            fullscreen: false,
            focused: false,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            tags: vec!["1".into()],
        });

        assert!(matches!(event, CompositorEvent::WindowCreated { .. }));
        assert!(state
            .snapshot()
            .windows
            .iter()
            .any(|window| window.id == WindowId::from("w3") && window.mapped));

        let events = state.destroy_window(&WindowId::from("w3")).unwrap();

        assert!(events
            .iter()
            .any(|event| matches!(event, CompositorEvent::WindowDestroyed { window_id } if window_id == &WindowId::from("w3"))));
        assert!(state
            .snapshot()
            .windows
            .iter()
            .all(|window| window.id != WindowId::from("w3")));
    }

    #[test]
    fn wm_state_sets_workspace_layout() {
        let mut state = WmState::from_snapshot(state());

        let event = state
            .set_layout_for_workspace(&WorkspaceId::from("ws-2"), "columns")
            .unwrap();

        assert!(matches!(
            event,
            CompositorEvent::LayoutChange {
                workspace_id: Some(_),
                layout: Some(_),
            }
        ));
        assert_eq!(
            state
                .snapshot()
                .workspace_by_id(&WorkspaceId::from("ws-2"))
                .unwrap()
                .effective_layout
                .as_ref()
                .map(|layout| layout.name.as_str()),
            Some("columns")
        );
    }

    #[test]
    fn wm_state_toggle_tag_on_current_output_is_noop_for_visible_tag() {
        let mut state = WmState::from_snapshot(state());

        let events = state.toggle_tag_on_current_output("1").unwrap();

        assert!(events.is_empty());
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );
    }

    #[test]
    fn wm_state_toggle_tag_on_current_output_switches_to_hidden_tag() {
        let mut state = WmState::from_snapshot(state());

        let events = state.toggle_tag_on_current_output("2").unwrap();

        assert!(events.iter().any(|event| matches!(
            event,
            CompositorEvent::TagChange {
                workspace_id: Some(id),
                ..
            } if id == &WorkspaceId::from("ws-2")
        )));
        assert_eq!(
            state.snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-2"))
        );
    }

    #[test]
    fn wm_state_focus_direction_cycles_visible_workspace_windows() {
        let mut snapshot = state();
        snapshot.windows.push(WindowSnapshot {
            id: WindowId::from("w3"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("thunar".into()),
            title: Some("Files".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            floating: false,
            fullscreen: false,
            focused: false,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            tags: vec!["1".into()],
        });
        snapshot.visible_window_ids = vec![WindowId::from("w1"), WindowId::from("w3")];
        let mut state = WmState::from_snapshot(snapshot);

        let event = state.focus_direction(FocusDirection::Right).unwrap();

        assert!(matches!(
            event,
            CompositorEvent::FocusChange {
                focused_window_id: Some(window_id),
                ..
            } if window_id == WindowId::from("w3")
        ));
        assert_eq!(
            state.snapshot().focused_window_id,
            Some(WindowId::from("w3"))
        );
    }
}
