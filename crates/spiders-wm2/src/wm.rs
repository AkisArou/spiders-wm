use crate::{
    state::{
        ManagedWindowState, OutputId, OutputNode, TopologyState, WindowId, WindowNode, WmState,
        WorkspaceId, WorkspaceState,
    },
    wm,
};

pub fn register_window(topology: &mut TopologyState, window_id: WindowId) {
    topology.windows.insert(
        window_id,
        WindowNode {
            alive: true,
            mapped: true,
            title: None,
            app_id: None,
        },
    );
}

pub fn register_output(
    topology: &mut TopologyState,
    wm: &mut WmState,
    output_id: OutputId,
    name: String,
) {
    topology.outputs.insert(
        output_id,
        OutputNode {
            name,
            enabled: true,
        },
    );

    if wm.focused_output.is_none() {
        wm.focused_output = Some(output_id);
    }

    if let Some(active_workspace) = wm.workspaces.get_mut(&wm.active_workspace) {
        if active_workspace.output.is_none() {
            active_workspace.output = Some(output_id);
        }
    }
}

pub fn ensure_workspace(wm: &mut WmState, workspace_id: WorkspaceId) {
    let focused_output = wm.focused_output;

    wm.workspaces
        .entry(workspace_id)
        .or_insert_with(|| WorkspaceState {
            name: workspace_id.0.to_string(),
            output: focused_output,
            windows: Vec::new(),
        });
}

pub fn place_new_window_in_active_workspace(wm: &mut WmState, window_id: WindowId) {
    let workspace_id = wm.active_workspace;

    wm.workspaces
        .get_mut(&workspace_id)
        .expect("active workspace must exist")
        .windows
        .push(window_id);

    wm.windows.insert(
        window_id,
        ManagedWindowState {
            workspace: workspace_id,
            floating: false,
            fullscreen: false,
        },
    );

    wm.focused_window = Some(window_id);
}

pub fn focus_window(wm: &mut WmState, window_id: WindowId) {
    if wm.windows.contains_key(&window_id) {
        wm.focused_window = Some(window_id);
    }
}

pub fn remove_window(topology: &mut TopologyState, wm: &mut WmState, window_id: WindowId) {
    if let Some(window) = topology.windows.get_mut(&window_id) {
        window.alive = false;
        window.mapped = false;
    }

    wm.windows.remove(&window_id);

    for workspace in wm.workspaces.values_mut() {
        workspace.windows.retain(|id| *id != window_id);
    }

    if wm.focused_window == Some(window_id) {
        wm.focused_window = None;
    }
}

pub fn next_focus_in_active_workspace(wm: &WmState) -> Option<WindowId> {
    let workspace = wm.workspaces.get(&wm.active_workspace)?;
    workspace.windows.last().copied()
}

pub fn switch_to_workspace(wm: &mut WmState, workspace_id: WorkspaceId) {
    ensure_workspace(wm, workspace_id);
    wm.active_workspace = workspace_id;
    wm.focused_window = wm
        .workspaces
        .get(&workspace_id)
        .and_then(|workspace| workspace.windows.last().copied());
}

pub fn active_workspace_windows(wm: &WmState) -> Vec<WindowId> {
    wm.workspaces
        .get(&wm.active_workspace)
        .map(|workspace| workspace.windows.clone())
        .unwrap_or_default()
}
