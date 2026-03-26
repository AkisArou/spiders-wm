use crate::model::wm::WmModel;
use crate::model::WindowId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseSelection {
    pub closing_window_id: Option<WindowId>,
}

pub fn mark_focused_window_closing(model: &mut WmModel) -> Option<WindowId> {
    let focused_id = model
        .focused_window_id
        .filter(|window_id| model.windows.contains_key(window_id));

    if focused_id != model.focused_window_id {
        model.set_window_focused(None);
    }

    if let Some(window_id) = focused_id {
        model.set_window_closing(window_id, true);
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

    model.set_window_identity(window_id, title, app_id);
    Some(window_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_focused_window_closing() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(2), None, None);
        model.set_window_focused(Some(WindowId(2)));

        let closing_id = mark_focused_window_closing(&mut model);

        assert_eq!(closing_id, Some(WindowId(2)));
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.closing), Some(false));
        assert_eq!(model.windows.get(&WindowId(2)).map(|window| window.closing), Some(true));
        assert_eq!(model.focused_window_id, Some(WindowId(2)));
    }

    #[test]
    fn request_close_focused_window_returns_selection() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(4), None, None);
        model.set_window_focused(Some(WindowId(4)));

        let selection = request_close_focused_window(&mut model);

        assert_eq!(
            selection,
            CloseSelection {
                closing_window_id: Some(WindowId(4)),
            }
        );
    }

    #[test]
    fn no_focused_window_means_no_closing_change() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);

        let closing_id = mark_focused_window_closing(&mut model);

        assert_eq!(closing_id, None);
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.closing), Some(false));
    }

    #[test]
    fn stale_focused_window_is_cleared() {
        let mut model = WmModel::default();
        model.focused_window_id = Some(WindowId(99));

        let closing_id = mark_focused_window_closing(&mut model);

        assert_eq!(closing_id, None);
        assert_eq!(model.focused_window_id, None);
    }

    #[test]
    fn syncs_window_identity_for_known_window() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(7), None, None);

        let updated = sync_window_identity(
            &mut model,
            WindowId(7),
            Some("Notes".to_string()),
            Some("org.example.notes".to_string()),
        );

        assert_eq!(updated, Some(WindowId(7)));
        let window = model.windows.get(&WindowId(7)).expect("window missing");
        assert_eq!(window.title.as_deref(), Some("Notes"));
        assert_eq!(window.app_id.as_deref(), Some("org.example.notes"));
    }

    #[test]
    fn ignores_identity_sync_for_unknown_window() {
        let mut model = WmModel::default();

        let updated = sync_window_identity(
            &mut model,
            WindowId(77),
            Some("Ghost".to_string()),
            Some("ghost.app".to_string()),
        );

        assert_eq!(updated, None);
    }
}