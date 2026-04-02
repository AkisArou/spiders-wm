use crate::wm::WindowGeometry;
use crate::WindowId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowGeometryCandidate {
    pub window_id: WindowId,
    pub geometry: WindowGeometry,
}

pub fn select_directional_focus_candidate(
    candidates: &[WindowGeometryCandidate],
    current_focused_window_id: Option<WindowId>,
    direction: NavigationDirection,
) -> Option<WindowId> {
    let current = current_focused_window_id.and_then(|window_id| {
        candidates
            .iter()
            .find(|candidate| candidate.window_id == window_id)
    })?;
    let current_center = rect_center(current.geometry);

    candidates
        .iter()
        .filter(|candidate| candidate.window_id != current.window_id)
        .filter_map(|candidate| {
            let candidate_center = rect_center(candidate.geometry);
            directional_score(current_center, candidate_center, direction)
                .map(|score| (score, candidate.window_id.clone()))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, window_id)| window_id)
}

pub fn managed_window_swap_positions(
    window_order: &[WindowId],
    first_window_id: WindowId,
    second_window_id: WindowId,
) -> Option<(usize, usize)> {
    let first_index = window_order
        .iter()
        .position(|window_id| *window_id == first_window_id)?;
    let second_index = window_order
        .iter()
        .position(|window_id| *window_id == second_window_id)?;
    Some((first_index, second_index))
}

fn rect_center(rect: WindowGeometry) -> (i32, i32) {
    (rect.x + rect.width / 2, rect.y + rect.height / 2)
}

fn directional_score(
    current_center: (i32, i32),
    candidate_center: (i32, i32),
    direction: NavigationDirection,
) -> Option<(i32, i32, i32)> {
    let dx = candidate_center.0 - current_center.0;
    let dy = candidate_center.1 - current_center.1;
    let total_distance = dx.abs() + dy.abs();

    match direction {
        NavigationDirection::Left if dx < 0 => Some((total_distance, dy.abs(), -dx)),
        NavigationDirection::Right if dx > 0 => Some((total_distance, dy.abs(), dx)),
        NavigationDirection::Up if dy < 0 => Some((total_distance, dx.abs(), -dy)),
        NavigationDirection::Down if dy > 0 => Some((total_distance, dx.abs(), dy)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_id;

    fn candidate(id: u64, x: i32, y: i32, width: i32, height: i32) -> WindowGeometryCandidate {
        WindowGeometryCandidate {
            window_id: window_id(id),
            geometry: WindowGeometry {
                x,
                y,
                width,
                height,
            },
        }
    }

    #[test]
    fn directional_focus_prefers_nearest_window_in_direction() {
        let candidates = vec![
            candidate(1, 0, 0, 100, 100),
            candidate(2, 140, 10, 100, 100),
            candidate(3, 320, 0, 100, 100),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right
            ),
            Some(window_id(2))
        );
    }

    #[test]
    fn directional_focus_filters_to_requested_axis() {
        let candidates = vec![
            candidate(1, 120, 120, 100, 100),
            candidate(2, 120, 0, 100, 100),
            candidate(3, 260, 120, 100, 100),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Up
            ),
            Some(window_id(2))
        );
        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Left
            ),
            None
        );
    }

    #[test]
    fn directional_focus_prefers_lower_cross_axis_offset() {
        let candidates = vec![
            candidate(1, 100, 100, 100, 100),
            candidate(2, 260, 90, 100, 100),
            candidate(3, 250, 220, 100, 100),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right
            ),
            Some(window_id(2))
        );
    }

    #[test]
    fn managed_window_swap_positions_resolves_both_indices() {
        let window_order = vec![window_id(10), window_id(20), window_id(30)];

        assert_eq!(
            managed_window_swap_positions(&window_order, window_id(10), window_id(30)),
            Some((0, 2))
        );
    }

    #[test]
    fn managed_window_swap_positions_requires_both_windows() {
        let window_order = vec![window_id(10), window_id(20)];

        assert_eq!(
            managed_window_swap_positions(&window_order, window_id(10), window_id(30)),
            None
        );
    }
}