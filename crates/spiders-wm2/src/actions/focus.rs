use crate::model::{WindowId, WmState};

pub fn focus_window(wm: &mut WmState, window_id: WindowId) -> bool {
    if !wm.windows.contains_key(&window_id) {
        return false;
    }

    if wm.focused_window.as_ref() == Some(&window_id) {
        return false;
    }

    wm.focused_window = Some(window_id);
    true
}

pub fn next_focus_in_active_workspace(wm: &WmState) -> Option<WindowId> {
    let workspace = wm.workspaces.get(&wm.active_workspace)?;
    workspace.windows.last().cloned()
}

pub fn focus_next_window(wm: &mut WmState) {
    let Some(workspace) = wm.workspaces.get(&wm.active_workspace) else {
        return;
    };

    if workspace.windows.is_empty() {
        wm.focused_window = None;
        return;
    }

    let next_index = match wm.focused_window.as_ref() {
        Some(current) => workspace
            .windows
            .iter()
            .position(|id| id == current)
            .map(|index| (index + 1) % workspace.windows.len())
            .unwrap_or(0),
        None => 0,
    };

    wm.focused_window = workspace.windows.get(next_index).cloned();
}

pub fn focus_previous_window(wm: &mut WmState) {
    let Some(workspace) = wm.workspaces.get(&wm.active_workspace) else {
        return;
    };

    if workspace.windows.is_empty() {
        wm.focused_window = None;
        return;
    }

    let previous_index = match wm.focused_window.as_ref() {
        Some(current) => workspace
            .windows
            .iter()
            .position(|id| id == current)
            .map(|index| {
                if index == 0 {
                    workspace.windows.len() - 1
                } else {
                    index - 1
                }
            })
            .unwrap_or(workspace.windows.len() - 1),
        None => workspace.windows.len() - 1,
    };

    wm.focused_window = workspace.windows.get(previous_index).cloned();
}

#[cfg(test)]
mod tests {
    use super::{focus_next_window, focus_previous_window, focus_window};
    use crate::model::{ManagedWindowState, WindowId, WmState};

    fn wm_with_windows(window_ids: &[WindowId]) -> WmState {
        let mut wm = WmState::default();
        let workspace_id = wm.active_workspace.clone();

        wm.workspaces.get_mut(&workspace_id).unwrap().windows = window_ids.to_vec();

        for window_id in window_ids {
            wm.windows.insert(
                window_id.clone(),
                ManagedWindowState::tiled(window_id.clone(), workspace_id.clone(), None),
            );
        }

        wm
    }

    #[test]
    fn focus_next_wraps_to_start() {
        let ids = [
            WindowId::from("1"),
            WindowId::from("2"),
            WindowId::from("3"),
        ];
        let mut wm = wm_with_windows(&ids);
        wm.focused_window = Some(WindowId::from("3"));

        focus_next_window(&mut wm);

        assert_eq!(wm.focused_window, Some(WindowId::from("1")));
    }

    #[test]
    fn focus_previous_wraps_to_end() {
        let ids = [
            WindowId::from("1"),
            WindowId::from("2"),
            WindowId::from("3"),
        ];
        let mut wm = wm_with_windows(&ids);
        wm.focused_window = Some(WindowId::from("1"));

        focus_previous_window(&mut wm);

        assert_eq!(wm.focused_window, Some(WindowId::from("3")));
    }

    #[test]
    fn focus_next_uses_first_window_when_none_focused() {
        let ids = [WindowId::from("10"), WindowId::from("20")];
        let mut wm = wm_with_windows(&ids);
        wm.focused_window = None;

        focus_next_window(&mut wm);

        assert_eq!(wm.focused_window, Some(WindowId::from("10")));
    }

    #[test]
    fn focus_window_reports_real_focus_changes_only() {
        let ids = [WindowId::from("10"), WindowId::from("20")];
        let mut wm = wm_with_windows(&ids);

        assert!(focus_window(&mut wm, WindowId::from("10")));
        assert_eq!(wm.focused_window, Some(WindowId::from("10")));

        assert!(!focus_window(&mut wm, WindowId::from("10")));
        assert!(!focus_window(&mut wm, WindowId::from("missing")));
        assert_eq!(wm.focused_window, Some(WindowId::from("10")));
    }
}
