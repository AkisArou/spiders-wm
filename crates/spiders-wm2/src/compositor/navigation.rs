use smithay::utils::{Logical, Rectangle};
use spiders_shared::command::FocusDirection;

use crate::model::WindowId;
use crate::state::SpidersWm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GeometryCandidate {
    window_id: WindowId,
    rect: Rectangle<i32, Logical>,
}

impl SpidersWm {
    pub(crate) fn visible_geometry_candidates(&self) -> Vec<GeometryCandidate> {
        self.visible_managed_window_positions()
            .into_iter()
            .filter_map(|managed_index| {
                let record = &self.managed_windows[managed_index];
                let location = self.space.element_location(&record.window)?;
                Some(GeometryCandidate {
                    window_id: record.id.clone(),
                    rect: Rectangle::new(location, record.window.geometry().size),
                })
            })
            .collect()
    }
}

pub(crate) fn select_directional_focus_candidate(
    candidates: &[GeometryCandidate],
    current_focused_window_id: Option<WindowId>,
    direction: FocusDirection,
) -> Option<WindowId> {
    let current = current_focused_window_id.and_then(|window_id| {
        candidates
            .iter()
            .find(|candidate| candidate.window_id == window_id)
    })?;
    let current_center = rect_center(current.rect);

    candidates
        .iter()
        .filter(|candidate| candidate.window_id != current.window_id)
        .filter_map(|candidate| {
            let candidate_center = rect_center(candidate.rect);
            directional_score(current_center, candidate_center, direction)
                .map(|score| (score, candidate.window_id.clone()))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, window_id)| window_id)
}

pub(crate) fn managed_window_swap_positions(
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

fn rect_center(rect: Rectangle<i32, Logical>) -> (i32, i32) {
    (rect.loc.x + rect.size.w / 2, rect.loc.y + rect.size.h / 2)
}

fn directional_score(
    current_center: (i32, i32),
    candidate_center: (i32, i32),
    direction: FocusDirection,
) -> Option<(i32, i32, i32)> {
    let dx = candidate_center.0 - current_center.0;
    let dy = candidate_center.1 - current_center.1;
    let total_distance = dx.abs() + dy.abs();

    match direction {
        FocusDirection::Left if dx < 0 => Some((total_distance, dy.abs(), -dx)),
        FocusDirection::Right if dx > 0 => Some((total_distance, dy.abs(), dx)),
        FocusDirection::Up if dy < 0 => Some((total_distance, dx.abs(), -dy)),
        FocusDirection::Down if dy > 0 => Some((total_distance, dx.abs(), dy)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::window_id;
    use smithay::utils::{Point, Size};

    fn candidate(id: u64, x: i32, y: i32, w: i32, h: i32) -> GeometryCandidate {
        GeometryCandidate {
            window_id: window_id(id),
            rect: Rectangle::<i32, Logical>::new(Point::from((x, y)), Size::from((w, h))),
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
                FocusDirection::Right
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
            select_directional_focus_candidate(&candidates, Some(window_id(1)), FocusDirection::Up),
            Some(window_id(2))
        );
        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                FocusDirection::Left
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
                FocusDirection::Right
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
