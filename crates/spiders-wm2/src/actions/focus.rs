use crate::model::WindowId;
use crate::model::wm::WmModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusUpdate {
    Unchanged,
    Set(Option<WindowId>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusSelection {
    pub focused_window_id: Option<WindowId>,
}

pub fn set_focused_window(model: &mut WmModel, focused_id: Option<WindowId>) -> Option<WindowId> {
    let focused_id = focused_id.filter(|window_id| model.windows.contains_key(window_id));
    model.set_window_focused(focused_id);
    focused_id
}

pub fn focus_next_window(model: &mut WmModel) -> Option<WindowId> {
    let next_focus = match model.focused_window_id {
        Some(current_id) => model
            .windows
            .keys()
            .copied()
            .find(|window_id| *window_id > current_id)
            .or_else(|| model.windows.keys().next().copied()),
        None => model.windows.keys().next().copied(),
    };

    set_focused_window(model, next_focus)
}

pub fn focus_previous_window(model: &mut WmModel) -> Option<WindowId> {
    let previous_focus = match model.focused_window_id {
        Some(current_id) => model
            .windows
            .keys()
            .copied()
            .rev()
            .find(|window_id| *window_id < current_id)
            .or_else(|| model.windows.keys().next_back().copied()),
        None => model.windows.keys().next_back().copied(),
    };

    set_focused_window(model, previous_focus)
}

pub fn request_focus_window(model: &mut WmModel, window_id: Option<WindowId>) -> FocusSelection {
    FocusSelection {
        focused_window_id: set_focused_window(model, window_id),
    }
}

pub fn request_focus_next_window(model: &mut WmModel) -> FocusSelection {
    FocusSelection {
        focused_window_id: focus_next_window(model),
    }
}

pub fn request_focus_previous_window(model: &mut WmModel) -> FocusSelection {
    FocusSelection {
        focused_window_id: focus_previous_window(model),
    }
}

pub fn remove_window(model: &mut WmModel, removed_id: WindowId) -> FocusUpdate {
    let removed_was_focused = model.focused_window_id == Some(removed_id);
    model.remove_window(removed_id);

    if !removed_was_focused {
        return FocusUpdate::Unchanged;
    }

    let next_focus = model.windows.keys().next_back().copied();
    let next_focus = set_focused_window(model, next_focus);
    FocusUpdate::Set(next_focus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focusing_unknown_window_clears_focus() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.set_window_focused(Some(WindowId(1)));

        let resolved = set_focused_window(&mut model, Some(WindowId(99)));

        assert_eq!(resolved, None);
        assert_eq!(model.focused_window_id, None);
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.focused), Some(false));
    }

    #[test]
    fn focusing_known_window_marks_only_that_window_focused() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(2), None, None);

        let resolved = set_focused_window(&mut model, Some(WindowId(2)));

        assert_eq!(resolved, Some(WindowId(2)));
        assert_eq!(model.focused_window_id, Some(WindowId(2)));
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.focused), Some(false));
        assert_eq!(model.windows.get(&WindowId(2)).map(|window| window.focused), Some(true));
    }

    #[test]
    fn removing_non_focused_window_keeps_existing_focus() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(2), None, None);
        model.set_window_focused(Some(WindowId(2)));

        let update = remove_window(&mut model, WindowId(1));

        assert_eq!(update, FocusUpdate::Unchanged);
        assert_eq!(model.focused_window_id, Some(WindowId(2)));
        assert!(!model.windows.contains_key(&WindowId(1)));
    }

    #[test]
    fn removing_focused_window_selects_latest_remaining_window() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(2), None, None);
        model.insert_window(WindowId(3), None, None);
        model.set_window_focused(Some(WindowId(2)));

        let update = remove_window(&mut model, WindowId(2));

        assert_eq!(update, FocusUpdate::Set(Some(WindowId(3))));
        assert_eq!(model.focused_window_id, Some(WindowId(3)));
        assert_eq!(model.windows.get(&WindowId(1)).map(|window| window.focused), Some(false));
        assert_eq!(model.windows.get(&WindowId(3)).map(|window| window.focused), Some(true));
    }

    #[test]
    fn removing_last_focused_window_clears_focus() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(4), None, None);
        model.set_window_focused(Some(WindowId(4)));

        let update = remove_window(&mut model, WindowId(4));

        assert_eq!(update, FocusUpdate::Set(None));
        assert_eq!(model.focused_window_id, None);
        assert!(model.windows.is_empty());
    }

    #[test]
    fn focusing_next_window_advances_and_wraps() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(3), None, None);
        model.insert_window(WindowId(8), None, None);
        model.set_window_focused(Some(WindowId(3)));

        let next = focus_next_window(&mut model);
        assert_eq!(next, Some(WindowId(8)));

        let wrapped = focus_next_window(&mut model);
        assert_eq!(wrapped, Some(WindowId(1)));
    }

    #[test]
    fn focusing_previous_window_rewinds_and_wraps() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(3), None, None);
        model.insert_window(WindowId(8), None, None);
        model.set_window_focused(Some(WindowId(3)));

        let previous = focus_previous_window(&mut model);
        assert_eq!(previous, Some(WindowId(1)));

        let wrapped = focus_previous_window(&mut model);
        assert_eq!(wrapped, Some(WindowId(8)));
    }

    #[test]
    fn request_focus_window_returns_selection() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(2), None, None);

        let selection = request_focus_window(&mut model, Some(WindowId(2)));

        assert_eq!(
            selection,
            FocusSelection {
                focused_window_id: Some(WindowId(2)),
            }
        );
    }

    #[test]
    fn request_focus_next_window_returns_selection() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(2), None, None);
        model.set_window_focused(Some(WindowId(1)));

        let selection = request_focus_next_window(&mut model);

        assert_eq!(
            selection,
            FocusSelection {
                focused_window_id: Some(WindowId(2)),
            }
        );
    }

    #[test]
    fn request_focus_previous_window_returns_selection() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);
        model.insert_window(WindowId(2), None, None);
        model.set_window_focused(Some(WindowId(2)));

        let selection = request_focus_previous_window(&mut model);

        assert_eq!(
            selection,
            FocusSelection {
                focused_window_id: Some(WindowId(1)),
            }
        );
    }
}