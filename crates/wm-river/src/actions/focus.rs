use spiders_core::WindowId;
use spiders_core::command::FocusDirection;

use crate::model::WmState;

pub fn active_workspace_window_ids(state: &WmState, window_stack: &[WindowId]) -> Vec<WindowId> {
    window_stack
        .iter()
        .filter(|window_id| {
            state.windows.get(*window_id).is_some_and(|window| {
                state.current_workspace_id.as_ref().is_none_or(|workspace_id| {
                    window.workspace_ids.iter().any(|id| id == workspace_id)
                })
            })
        })
        .cloned()
        .collect()
}

pub fn focus_target_in_direction(
    _state: &WmState,
    active_window_ids: &[WindowId],
    direction: FocusDirection,
    focused_window_id: Option<&WindowId>,
) -> Option<WindowId> {
    if active_window_ids.is_empty() {
        return None;
    }

    let fallback = active_window_ids.last();
    let focused_index = focused_window_id
        .or(fallback)
        .and_then(|focused_id| active_window_ids.iter().position(|id| id == focused_id))
        .unwrap_or(active_window_ids.len().saturating_sub(1));

    let target_index = match direction {
        FocusDirection::Left | FocusDirection::Up => {
            if focused_index == 0 {
                active_window_ids.len() - 1
            } else {
                focused_index - 1
            }
        }
        FocusDirection::Right | FocusDirection::Down => {
            if focused_index + 1 >= active_window_ids.len() { 0 } else { focused_index + 1 }
        }
    };

    active_window_ids.get(target_index).cloned()
}

pub fn top_window_id(active_window_ids: &[WindowId]) -> Option<WindowId> {
    active_window_ids.last().cloned()
}

pub fn directional_neighbor_window_id(
    state: &WmState,
    active_window_ids: &[WindowId],
    focused_window_id: &WindowId,
    direction: FocusDirection,
) -> Option<WindowId> {
    let focused = state.windows.get(focused_window_id)?;
    let focused_cx = focused.x + focused.width / 2;
    let focused_cy = focused.y + focused.height / 2;

    active_window_ids
        .iter()
        .filter(|window_id| *window_id != focused_window_id)
        .filter_map(|window_id| {
            let window = state.windows.get(window_id)?;
            let cx = window.x + window.width / 2;
            let cy = window.y + window.height / 2;
            let dx = cx - focused_cx;
            let dy = cy - focused_cy;

            let valid = match direction {
                FocusDirection::Left => dx < 0,
                FocusDirection::Right => dx > 0,
                FocusDirection::Up => dy < 0,
                FocusDirection::Down => dy > 0,
            };
            if !valid {
                return None;
            }

            let primary = match direction {
                FocusDirection::Left | FocusDirection::Right => dx.abs(),
                FocusDirection::Up | FocusDirection::Down => dy.abs(),
            };
            let secondary = match direction {
                FocusDirection::Left | FocusDirection::Right => dy.abs(),
                FocusDirection::Up | FocusDirection::Down => dx.abs(),
            };

            Some((window_id.clone(), primary, secondary))
        })
        .min_by_key(|(_, primary, secondary)| (*primary, *secondary))
        .map(|(window_id, _, _)| window_id)
}
