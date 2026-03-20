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

pub fn sync_active_workspace_to_output(topology: &mut TopologyState, wm: &mut WmState) -> bool {
    let active_workspace_id = wm.active_workspace.clone();
    let workspace_output = wm
        .workspaces
        .get(&active_workspace_id)
        .and_then(|workspace| workspace.output.clone());

    let target_output_id = workspace_output.or_else(|| wm.focused_output.clone());
    let Some(output_id) = target_output_id else {
        return false;
    };

    let mut changed = false;

    if wm.focused_output.as_ref() != Some(&output_id) {
        wm.focused_output = Some(output_id.clone());
        changed = true;
    }

    if let Some(workspace) = wm.workspaces.get_mut(&active_workspace_id) {
        if workspace.output.as_ref() != Some(&output_id) {
            workspace.output = Some(output_id.clone());
            changed = true;
        }
    }

    if let Some(output) = topology.outputs.get_mut(&output_id) {
        if output.current_workspace.as_ref() != Some(&active_workspace_id) {
            output.current_workspace = Some(active_workspace_id);
            changed = true;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::{register_output, sync_active_workspace_to_output, update_output_logical_size};
    use crate::model::{OutputId, OutputNode, TopologyState, WmState, WorkspaceId, WorkspaceState};

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

    #[test]
    fn sync_active_workspace_to_output_uses_existing_workspace_output() {
        let mut topology = TopologyState::default();
        let mut wm = WmState::default();
        wm.active_workspace = WorkspaceId::from("ws-1");
        wm.focused_output = Some(OutputId::from("out-2"));
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: Some(OutputId::from("out-1")),
                windows: vec![],
            },
        );
        topology.outputs.insert(
            OutputId::from("out-1"),
            OutputNode {
                id: OutputId::from("out-1"),
                name: "one".into(),
                enabled: true,
                current_workspace: None,
                logical_size: (1280, 720),
            },
        );

        assert!(sync_active_workspace_to_output(&mut topology, &mut wm));
        assert_eq!(wm.focused_output, Some(OutputId::from("out-1")));
        assert_eq!(
            topology
                .outputs
                .get(&OutputId::from("out-1"))
                .unwrap()
                .current_workspace,
            Some(WorkspaceId::from("ws-1"))
        );
    }

    #[test]
    fn sync_active_workspace_to_output_assigns_focused_output_when_workspace_unset() {
        let mut topology = TopologyState::default();
        let mut wm = WmState::default();
        wm.active_workspace = WorkspaceId::from("ws-1");
        wm.focused_output = Some(OutputId::from("out-1"));
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: None,
                windows: vec![],
            },
        );
        topology.outputs.insert(
            OutputId::from("out-1"),
            OutputNode {
                id: OutputId::from("out-1"),
                name: "one".into(),
                enabled: true,
                current_workspace: None,
                logical_size: (1280, 720),
            },
        );

        assert!(sync_active_workspace_to_output(&mut topology, &mut wm));
        assert_eq!(
            wm.workspaces
                .get(&WorkspaceId::from("ws-1"))
                .unwrap()
                .output,
            Some(OutputId::from("out-1"))
        );
        assert_eq!(
            topology
                .outputs
                .get(&OutputId::from("out-1"))
                .unwrap()
                .current_workspace,
            Some(WorkspaceId::from("ws-1"))
        );
    }
}
