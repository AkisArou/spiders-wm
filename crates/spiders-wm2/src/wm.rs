use crate::state::{ManagedWindowState, TopologyState, WindowId, WindowNode, WmState};

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
