use crate::model::{ManagedWindowState, TopologyState, WindowId, WindowNode, WmState};

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

pub fn update_window_metadata(
    topology: &mut TopologyState,
    wm: &mut WmState,
    window_id: &WindowId,
    title: Option<String>,
    app_id: Option<String>,
) {
    if let Some(window) = topology.windows.get_mut(window_id) {
        window.title = title.clone();
        window.app_id = app_id.clone();
    }

    if let Some(window) = wm.windows.get_mut(window_id) {
        window.title = title;
        window.app_id = app_id;
    }
}

pub fn place_new_window_in_active_workspace(wm: &mut WmState, window_id: WindowId) {
    let workspace_id = wm.active_workspace.clone();
    let output_id = wm.focused_output.clone();

    wm.workspaces
        .get_mut(&workspace_id)
        .expect("active workspace must exist")
        .windows
        .push(window_id.clone());

    wm.windows.insert(
        window_id.clone(),
        ManagedWindowState::tiled(window_id.clone(), workspace_id, output_id),
    );

    wm.focused_window = Some(window_id);
}

pub fn begin_window_removal(topology: &mut TopologyState, wm: &mut WmState, window_id: &WindowId) {
    mark_window_unmapped(topology, wm, window_id);

    for workspace in wm.workspaces.values_mut() {
        workspace.windows.retain(|id| id != window_id);
    }

    if wm.focused_window.as_ref() == Some(window_id) {
        wm.focused_window = None;
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

pub fn mark_window_unmapped(topology: &mut TopologyState, wm: &mut WmState, window_id: &WindowId) {
    if let Some(window) = topology.windows.get_mut(window_id) {
        window.alive = false;
        window.mapped = false;
    }

    if let Some(window) = wm.windows.get_mut(window_id) {
        window.mapped = false;
    }
}

pub fn swap_focused_window_with_next(wm: &mut WmState) {
    let Some(workspace) = wm.workspaces.get_mut(&wm.active_workspace) else {
        return;
    };

    if workspace.windows.len() < 2 {
        return;
    }

    let Some(focused_window) = wm.focused_window.as_ref() else {
        return;
    };

    let Some(index) = workspace.windows.iter().position(|id| id == focused_window) else {
        return;
    };

    let next_index = (index + 1) % workspace.windows.len();
    workspace.windows.swap(index, next_index);
}

pub fn swap_focused_window_with_previous(wm: &mut WmState) {
    let Some(workspace) = wm.workspaces.get_mut(&wm.active_workspace) else {
        return;
    };

    if workspace.windows.len() < 2 {
        return;
    }

    let Some(focused_window) = wm.focused_window.as_ref() else {
        return;
    };

    let Some(index) = workspace.windows.iter().position(|id| id == focused_window) else {
        return;
    };

    let previous_index = if index == 0 {
        workspace.windows.len() - 1
    } else {
        index - 1
    };

    workspace.windows.swap(index, previous_index);
}
