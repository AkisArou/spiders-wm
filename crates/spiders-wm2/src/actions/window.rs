use crate::model::wm::WmModel;
use crate::model::{WindowId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseSelection {
    pub closing_window_id: Option<WindowId>,
}

pub fn mark_focused_window_closing(model: &mut WmModel) -> Option<WindowId> {
    let focused_id = model
        .focused_window_id
        .clone()
        .filter(|window_id| model.windows.contains_key(window_id));

    if focused_id != model.focused_window_id {
        model.set_window_focused(None);
    }

    if let Some(window_id) = focused_id.as_ref() {
        model.set_window_closing(window_id.clone(), true);
    }

    focused_id
}

pub fn request_close_focused_window(model: &mut WmModel) -> CloseSelection {
    CloseSelection {
        closing_window_id: mark_focused_window_closing(model),
    }
}

pub fn sync_window_identity(
    model: &mut WmModel,
    window_id: WindowId,
    title: Option<String>,
    app_id: Option<String>,
) -> Option<WindowId> {
    if !model.windows.contains_key(&window_id) {
        return None;
    }

    model.set_window_identity(window_id.clone(), title, app_id);
    Some(window_id)
}

pub fn assign_focused_window_to_workspace<I>(
    model: &mut WmModel,
    workspace_id: WorkspaceId,
    window_order: I,
) -> Option<WindowId>
where
    I: IntoIterator<Item = WindowId>,
{
    let focused_window_id = model
        .focused_window_id
        .clone()
        .filter(|window_id| model.windows.contains_key(window_id));
    let Some(focused_window_id) = focused_window_id else {
        return model.focused_window_id.clone();
    };

    model.set_window_workspace(focused_window_id.clone(), Some(workspace_id.clone()));

    let next_focused_window_id = if model.current_workspace_id.as_ref() == Some(&workspace_id) {
        Some(focused_window_id)
    } else {
        model.preferred_focus_window_on_current_workspace(window_order)
    };
    model.set_window_focused(next_focused_window_id.clone());
    next_focused_window_id
}

pub fn toggle_assign_focused_window_to_workspace<I>(
    model: &mut WmModel,
    workspace_id: WorkspaceId,
    window_order: I,
) -> Option<WindowId>
where
    I: IntoIterator<Item = WindowId>,
{
    assign_focused_window_to_workspace(model, workspace_id, window_order)
}

pub fn toggle_focused_window_floating(model: &mut WmModel) -> Option<WindowId> {
    let focused_window_id = model
        .focused_window_id
        .clone()
        .filter(|window_id| model.windows.contains_key(window_id));
    let Some(focused_window_id) = focused_window_id else {
        return None;
    };

    let next_floating = model
        .windows
        .get(&focused_window_id)
        .map(|window| !window.floating)
        .unwrap_or(false);
    model.set_window_floating(focused_window_id.clone(), next_floating);
    Some(focused_window_id)
}

pub fn toggle_focused_window_fullscreen(model: &mut WmModel) -> Option<WindowId> {
    let focused_window_id = model
        .focused_window_id
        .clone()
        .filter(|window_id| model.windows.contains_key(window_id));
    let Some(focused_window_id) = focused_window_id else {
        return None;
    };

    let next_fullscreen = model
        .windows
        .get(&focused_window_id)
        .map(|window| !window.fullscreen)
        .unwrap_or(false);

    let window_ids = model.windows.keys().cloned().collect::<Vec<_>>();
    for window_id in window_ids {
        model.set_window_fullscreen(window_id, false);
    }
    model.set_window_fullscreen(focused_window_id.clone(), next_fullscreen);

    Some(focused_window_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::window_id;

    #[test]
    fn marks_focused_window_closing() {
        let mut model = WmModel::default();
        model.insert_window(window_id(1), None, None);
        model.insert_window(window_id(2), None, None);
        model.set_window_focused(Some(window_id(2)));

        let closing_id = mark_focused_window_closing(&mut model);

        assert_eq!(closing_id, Some(window_id(2)));
        assert_eq!(model.windows.get(&window_id(1)).map(|window| window.closing), Some(false));
        assert_eq!(model.windows.get(&window_id(2)).map(|window| window.closing), Some(true));
        assert_eq!(model.focused_window_id, Some(window_id(2)));
    }

    #[test]
    fn request_close_focused_window_returns_selection() {
        let mut model = WmModel::default();
        model.insert_window(window_id(4), None, None);
        model.set_window_focused(Some(window_id(4)));

        let selection = request_close_focused_window(&mut model);

        assert_eq!(
            selection,
            CloseSelection {
                closing_window_id: Some(window_id(4)),
            }
        );
    }

    #[test]
    fn no_focused_window_means_no_closing_change() {
        let mut model = WmModel::default();
        model.insert_window(window_id(1), None, None);

        let closing_id = mark_focused_window_closing(&mut model);

        assert_eq!(closing_id, None);
        assert_eq!(model.windows.get(&window_id(1)).map(|window| window.closing), Some(false));
    }

    #[test]
    fn stale_focused_window_is_cleared() {
        let mut model = WmModel::default();
        model.focused_window_id = Some(window_id(99));

        let closing_id = mark_focused_window_closing(&mut model);

        assert_eq!(closing_id, None);
        assert_eq!(model.focused_window_id, None);
    }

    #[test]
    fn syncs_window_identity_for_known_window() {
        let mut model = WmModel::default();
        model.insert_window(window_id(7), None, None);

        let updated = sync_window_identity(
            &mut model,
            window_id(7),
            Some("Notes".to_string()),
            Some("org.example.notes".to_string()),
        );

        assert_eq!(updated, Some(window_id(7)));
        let window = model.windows.get(&window_id(7)).expect("window missing");
        assert_eq!(window.title.as_deref(), Some("Notes"));
        assert_eq!(window.app_id.as_deref(), Some("org.example.notes"));
    }

    #[test]
    fn ignores_identity_sync_for_unknown_window() {
        let mut model = WmModel::default();

        let updated = sync_window_identity(
            &mut model,
            window_id(77),
            Some("Ghost".to_string()),
            Some("ghost.app".to_string()),
        );

        assert_eq!(updated, None);
    }

    #[test]
    fn assigning_focused_window_to_other_workspace_updates_workspace_and_refocuses() {
        let mut model = WmModel::default();
        model.upsert_workspace(WorkspaceId("1".to_string()), "1".to_string());
        model.upsert_workspace(WorkspaceId("2".to_string()), "2".to_string());
        model.set_current_workspace(WorkspaceId("1".to_string()));
        model.insert_window(window_id(1), Some(WorkspaceId("1".to_string())), None);
        model.insert_window(window_id(2), Some(WorkspaceId("1".to_string())), None);
        model.set_window_focused(Some(window_id(2)));

        let next_focus = assign_focused_window_to_workspace(
            &mut model,
            WorkspaceId("2".to_string()),
            [window_id(1), window_id(2)],
        );

        assert_eq!(
            model.windows.get(&window_id(2)).and_then(|window| window.workspace_id.clone()),
            Some(WorkspaceId("2".to_string()))
        );
        assert_eq!(next_focus, Some(window_id(1)));
        assert_eq!(model.focused_window_id, Some(window_id(1)));
    }

    #[test]
    fn assigning_focused_window_to_current_workspace_keeps_focus() {
        let mut model = WmModel::default();
        model.upsert_workspace(WorkspaceId("1".to_string()), "1".to_string());
        model.set_current_workspace(WorkspaceId("1".to_string()));
        model.insert_window(window_id(4), Some(WorkspaceId("1".to_string())), None);
        model.set_window_focused(Some(window_id(4)));

        let next_focus = assign_focused_window_to_workspace(
            &mut model,
            WorkspaceId("1".to_string()),
            [window_id(4)],
        );

        assert_eq!(next_focus, Some(window_id(4)));
        assert_eq!(model.focused_window_id, Some(window_id(4)));
    }

    #[test]
    fn toggling_focused_window_floating_flips_the_flag() {
        let mut model = WmModel::default();
        model.insert_window(window_id(12), None, None);
        model.set_window_focused(Some(window_id(12)));

        let toggled = toggle_focused_window_floating(&mut model);
        assert_eq!(toggled, Some(window_id(12)));
        assert_eq!(
            model.windows.get(&window_id(12)).map(|window| window.floating),
            Some(true)
        );

        let toggled_again = toggle_focused_window_floating(&mut model);
        assert_eq!(toggled_again, Some(window_id(12)));
        assert_eq!(
            model.windows.get(&window_id(12)).map(|window| window.floating),
            Some(false)
        );
    }

    #[test]
    fn toggling_focused_window_floating_without_focus_is_noop() {
        let mut model = WmModel::default();
        model.insert_window(window_id(13), None, None);

        let toggled = toggle_focused_window_floating(&mut model);

        assert_eq!(toggled, None);
        assert_eq!(
            model.windows.get(&window_id(13)).map(|window| window.floating),
            Some(false)
        );
    }

    #[test]
    fn toggling_focused_window_fullscreen_flips_the_flag() {
        let mut model = WmModel::default();
        model.insert_window(window_id(14), None, None);
        model.set_window_focused(Some(window_id(14)));

        let toggled = toggle_focused_window_fullscreen(&mut model);

        assert_eq!(toggled, Some(window_id(14)));
        assert_eq!(
            model.windows.get(&window_id(14)).map(|window| window.fullscreen),
            Some(true)
        );
    }

    #[test]
    fn toggling_focused_window_fullscreen_clears_other_fullscreen_windows() {
        let mut model = WmModel::default();
        model.insert_window(window_id(14), None, None);
        model.insert_window(window_id(15), None, None);
        model.set_window_fullscreen(window_id(14), true);
        model.set_window_focused(Some(window_id(15)));

        let toggled = toggle_focused_window_fullscreen(&mut model);

        assert_eq!(toggled, Some(window_id(15)));
        assert_eq!(
            model.windows.get(&window_id(14)).map(|window| window.fullscreen),
            Some(false)
        );
        assert_eq!(
            model.windows.get(&window_id(15)).map(|window| window.fullscreen),
            Some(true)
        );
    }

    #[test]
    fn toggling_focused_window_fullscreen_without_focus_is_noop() {
        let mut model = WmModel::default();
        model.insert_window(window_id(16), None, None);

        let toggled = toggle_focused_window_fullscreen(&mut model);

        assert_eq!(toggled, None);
        assert_eq!(
            model.windows.get(&window_id(16)).map(|window| window.fullscreen),
            Some(false)
        );
    }
}