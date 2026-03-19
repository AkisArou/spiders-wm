use crate::model::{OutputId, OutputNode, TopologyState, WmState};

pub fn register_output(
    topology: &mut TopologyState,
    wm: &mut WmState,
    output_id: OutputId,
    name: String,
    logical_size: (u32, u32),
) {
    topology.outputs.insert(
        output_id.clone(),
        OutputNode {
            id: output_id.clone(),
            name,
            enabled: true,
            current_workspace: Some(wm.active_workspace.clone()),
            logical_size,
        },
    );

    if wm.focused_output.is_none() {
        wm.focused_output = Some(output_id.clone());
    }

    if let Some(active_workspace) = wm.workspaces.get_mut(&wm.active_workspace) {
        if active_workspace.output.is_none() {
            active_workspace.output = Some(output_id);
        }
    }
}

pub fn update_output_logical_size(
    topology: &mut TopologyState,
    output_id: &OutputId,
    logical_size: (u32, u32),
) -> bool {
    let Some(output) = topology.outputs.get_mut(output_id) else {
        return false;
    };

    if output.logical_size == logical_size {
        return false;
    }

    output.logical_size = logical_size;
    true
}

#[cfg(test)]
mod tests {
    use super::{register_output, update_output_logical_size};
    use crate::model::{OutputId, TopologyState, WmState, WorkspaceId, WorkspaceState};

    #[test]
    fn update_output_logical_size_changes_topology_when_size_differs() {
        let mut topology = TopologyState::default();
        let mut wm = WmState::default();
        wm.active_workspace = WorkspaceId::from("ws-1");
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: None,
                windows: vec![],
            },
        );

        register_output(
            &mut topology,
            &mut wm,
            OutputId::from("out-1"),
            "winit".into(),
            (1280, 720),
        );

        assert!(update_output_logical_size(
            &mut topology,
            &OutputId::from("out-1"),
            (1920, 1080),
        ));
        assert_eq!(
            topology
                .outputs
                .get(&OutputId::from("out-1"))
                .unwrap()
                .logical_size,
            (1920, 1080)
        );
    }

    #[test]
    fn register_output_uses_provided_logical_size() {
        let mut topology = TopologyState::default();
        let mut wm = WmState::default();
        wm.active_workspace = WorkspaceId::from("ws-1");
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: None,
                windows: vec![],
            },
        );

        register_output(
            &mut topology,
            &mut wm,
            OutputId::from("out-1"),
            "winit".into(),
            (1920, 1080),
        );

        assert_eq!(
            topology
                .outputs
                .get(&OutputId::from("out-1"))
                .unwrap()
                .logical_size,
            (1920, 1080)
        );
    }

    #[test]
    fn update_output_logical_size_is_noop_for_same_size() {
        let mut topology = TopologyState::default();
        let mut wm = WmState::default();
        wm.active_workspace = WorkspaceId::from("ws-1");
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: None,
                windows: vec![],
            },
        );

        register_output(
            &mut topology,
            &mut wm,
            OutputId::from("out-1"),
            "winit".into(),
            (1280, 720),
        );

        assert!(!update_output_logical_size(
            &mut topology,
            &OutputId::from("out-1"),
            (1280, 720),
        ));
    }
}
