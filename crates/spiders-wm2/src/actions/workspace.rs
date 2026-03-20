use crate::model::{WindowId, WmState, WorkspaceId, WorkspaceState};

pub fn ensure_workspace(wm: &mut WmState, workspace_id: WorkspaceId) {
    let focused_output = wm.focused_output.clone();

    wm.workspaces
        .entry(workspace_id.clone())
        .or_insert_with(|| WorkspaceState {
            id: workspace_id.clone(),
            name: workspace_id.to_string(),
            output: focused_output,
            windows: Vec::new(),
        });
}

pub fn switch_to_workspace(wm: &mut WmState, workspace_id: WorkspaceId) {
    ensure_workspace(wm, workspace_id.clone());
    wm.active_workspace = workspace_id.clone();
    wm.focused_window = wm
        .workspaces
        .get(&workspace_id)
        .and_then(|workspace| workspace.windows.last().cloned());
}

pub fn move_focused_window_to_workspace(wm: &mut WmState, workspace_id: WorkspaceId) {
    let Some(window_id) = wm.focused_window.clone() else {
        return;
    };

    ensure_workspace(wm, workspace_id.clone());

    for workspace in wm.workspaces.values_mut() {
        workspace.windows.retain(|id| *id != window_id);
    }

    if let Some(window_state) = wm.windows.get_mut(&window_id) {
        window_state.workspace = workspace_id.clone();
        window_state.output = wm
            .workspaces
            .get(&workspace_id)
            .and_then(|workspace| workspace.output.clone());
    } else {
        return;
    }

    wm.workspaces
        .get_mut(&workspace_id)
        .expect("target workspace must exist")
        .windows
        .push(window_id.clone());

    wm.active_workspace = workspace_id;
    wm.focused_window = Some(window_id);
}

pub fn active_workspace_windows(wm: &WmState) -> Vec<WindowId> {
    wm.workspaces
        .get(&wm.active_workspace)
        .map(|workspace| workspace.windows.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{move_focused_window_to_workspace, switch_to_workspace};
    use crate::{
        actions::sync_active_workspace_to_output,
        model::{
            ManagedWindowState, OutputId, OutputNode, TopologyState, WindowId, WmState, WorkspaceId,
        },
    };

    #[test]
    fn move_focused_window_updates_both_workspaces_and_focus() {
        let mut wm = WmState::default();
        let source = WorkspaceId::from("1");
        let target = WorkspaceId::from("2");
        let moved = WindowId::from("7");
        let other = WindowId::from("8");

        wm.workspaces.get_mut(&source).unwrap().windows = vec![moved.clone(), other.clone()];
        wm.windows.insert(
            moved.clone(),
            ManagedWindowState::tiled(moved.clone(), source.clone(), None),
        );
        wm.windows.insert(
            other.clone(),
            ManagedWindowState::tiled(other.clone(), source.clone(), None),
        );
        wm.focused_window = Some(moved.clone());

        move_focused_window_to_workspace(&mut wm, target.clone());

        assert_eq!(wm.active_workspace, target);
        assert_eq!(wm.focused_window, Some(moved.clone()));
        assert_eq!(
            wm.workspaces.get(&source).unwrap().windows,
            vec![other.clone()]
        );
        assert_eq!(
            wm.workspaces.get(&target).unwrap().windows,
            vec![moved.clone()]
        );
        assert_eq!(wm.windows.get(&moved).unwrap().workspace, target);
    }

    #[test]
    fn switch_to_workspace_focuses_last_window() {
        let mut wm = WmState::default();
        let target = WorkspaceId::from("3");
        let first = WindowId::from("11");
        let last = WindowId::from("12");

        switch_to_workspace(&mut wm, target.clone());
        wm.workspaces.get_mut(&target).unwrap().windows = vec![first.clone(), last.clone()];
        wm.windows.insert(
            first.clone(),
            ManagedWindowState::tiled(first.clone(), target.clone(), None),
        );
        wm.windows.insert(
            last.clone(),
            ManagedWindowState::tiled(last.clone(), target.clone(), None),
        );

        switch_to_workspace(&mut wm, target.clone());

        assert_eq!(wm.active_workspace, target);
        assert_eq!(wm.focused_window, Some(last));
    }

    #[test]
    fn switch_to_workspace_and_sync_updates_focused_output_and_output_workspace() {
        let mut wm = WmState::default();
        let mut topology = TopologyState::default();
        let target = WorkspaceId::from("2");

        wm.focused_output = Some(OutputId::from("out-1"));
        wm.workspaces
            .get_mut(&WorkspaceId::from("1"))
            .unwrap()
            .output = Some(OutputId::from("out-1"));
        wm.workspaces.insert(
            target.clone(),
            crate::model::WorkspaceState {
                id: target.clone(),
                name: "2".into(),
                output: Some(OutputId::from("out-2")),
                windows: vec![],
            },
        );
        topology.outputs.insert(
            OutputId::from("out-1"),
            OutputNode {
                id: OutputId::from("out-1"),
                name: "one".into(),
                enabled: true,
                current_workspace: Some(WorkspaceId::from("1")),
                logical_size: (1280, 720),
            },
        );
        topology.outputs.insert(
            OutputId::from("out-2"),
            OutputNode {
                id: OutputId::from("out-2"),
                name: "two".into(),
                enabled: true,
                current_workspace: None,
                logical_size: (1280, 720),
            },
        );

        switch_to_workspace(&mut wm, target.clone());
        assert!(sync_active_workspace_to_output(&mut topology, &mut wm));

        assert_eq!(wm.focused_output, Some(OutputId::from("out-2")));
        assert_eq!(
            topology
                .outputs
                .get(&OutputId::from("out-2"))
                .unwrap()
                .current_workspace,
            Some(target)
        );
    }

    #[test]
    fn move_focused_window_to_workspace_and_sync_keeps_target_output_aligned() {
        let mut wm = WmState::default();
        let mut topology = TopologyState::default();
        let source = WorkspaceId::from("1");
        let target = WorkspaceId::from("2");
        let moved = WindowId::from("7");

        wm.focused_output = Some(OutputId::from("out-1"));
        wm.workspaces.get_mut(&source).unwrap().output = Some(OutputId::from("out-1"));
        wm.workspaces.insert(
            target.clone(),
            crate::model::WorkspaceState {
                id: target.clone(),
                name: "2".into(),
                output: Some(OutputId::from("out-2")),
                windows: vec![],
            },
        );
        topology.outputs.insert(
            OutputId::from("out-1"),
            OutputNode {
                id: OutputId::from("out-1"),
                name: "one".into(),
                enabled: true,
                current_workspace: Some(source.clone()),
                logical_size: (1280, 720),
            },
        );
        topology.outputs.insert(
            OutputId::from("out-2"),
            OutputNode {
                id: OutputId::from("out-2"),
                name: "two".into(),
                enabled: true,
                current_workspace: None,
                logical_size: (1280, 720),
            },
        );
        wm.workspaces.get_mut(&source).unwrap().windows = vec![moved.clone()];
        wm.windows.insert(
            moved.clone(),
            ManagedWindowState::tiled(moved.clone(), source.clone(), Some(OutputId::from("out-1"))),
        );
        wm.focused_window = Some(moved.clone());

        move_focused_window_to_workspace(&mut wm, target.clone());
        assert!(sync_active_workspace_to_output(&mut topology, &mut wm));

        assert_eq!(wm.focused_output, Some(OutputId::from("out-2")));
        assert_eq!(
            wm.windows.get(&moved).unwrap().output,
            Some(OutputId::from("out-2"))
        );
        assert_eq!(
            topology
                .outputs
                .get(&OutputId::from("out-2"))
                .unwrap()
                .current_workspace,
            Some(target)
        );
    }
}
