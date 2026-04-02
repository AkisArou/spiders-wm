use std::collections::BTreeMap;

use crate::focus::{FocusAxis, FocusBranchKey, FocusTree};
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
    pub scope_path: Vec<String>,
}

pub fn select_directional_focus_candidate(
    candidates: &[WindowGeometryCandidate],
    current_focused_window_id: Option<WindowId>,
    direction: NavigationDirection,
    remembered_focus_by_scope: &BTreeMap<String, WindowId>,
    focus_tree: Option<&FocusTree>,
) -> Option<WindowId> {
    let current = current_focused_window_id.and_then(|window_id| {
        candidates
            .iter()
            .find(|candidate| candidate.window_id == window_id)
    })?;

    if let Some(focus_tree) = focus_tree
        && let Some(window_id) = select_directional_focus_candidate_from_tree(
            current,
            direction,
            remembered_focus_by_scope,
            focus_tree,
        )
    {
        return Some(window_id);
    }

    for scope_depth in (0..current.scope_path.len()).rev() {
        let scope_key = &current.scope_path[scope_depth];
        let mut branches = scope_branches(candidates, scope_key, scope_depth);
        let Some(axis) = infer_split_axis(&branches) else {
            continue;
        };

        if !direction_matches_axis(direction, axis) || branches.len() < 2 {
            continue;
        }

        sort_scope_branches(&mut branches, axis);
        let current_branch = current_branch_key(current, scope_depth);
        let Some(current_index) = branches
            .iter()
            .position(|branch| branch.key == current_branch)
        else {
            continue;
        };

        let Some(target_index) = wrapped_branch_index(current_index, branches.len(), direction)
        else {
            continue;
        };

        let Some(target_branch) = branches.get(target_index) else {
            continue;
        };

        if let Some(window_id) = resolve_branch_target(
            candidates,
            target_branch,
            direction,
            remembered_focus_by_scope,
        ) {
            return Some(window_id);
        }
    }

    select_geometric_candidate(candidates, current, direction)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
struct ScopeBranch<'a> {
    key: FocusBranchKey,
    geometry: WindowGeometry,
    descendants: Vec<&'a WindowGeometryCandidate>,
    scope_depth: Option<usize>,
}

fn select_directional_focus_candidate_from_tree(
    current: &WindowGeometryCandidate,
    direction: NavigationDirection,
    remembered_focus_by_scope: &BTreeMap<String, WindowId>,
    focus_tree: &FocusTree,
) -> Option<WindowId> {
    let scope_path = focus_tree.scope_path(&current.window_id)?;

    for scope_depth in (0..scope_path.len()).rev() {
        let scope_key = &scope_path[scope_depth];
        let Some(navigation) = focus_tree.navigation(scope_key) else {
            continue;
        };

        if !direction_matches_focus_axis(direction, navigation.axis)
            || navigation.branches.len() < 2
        {
            continue;
        }

        let current_branch = current_branch_key(current, scope_depth);
        let Some(current_index) = navigation
            .branches
            .iter()
            .position(|branch| branch == &current_branch)
        else {
            continue;
        };

        let Some(target_index) =
            wrapped_branch_index(current_index, navigation.branches.len(), direction)
        else {
            continue;
        };

        let Some(target_branch) = navigation.branches.get(target_index) else {
            continue;
        };

        if let Some(window_id) = resolve_tree_branch_target(
            focus_tree,
            target_branch,
            direction,
            remembered_focus_by_scope,
        ) {
            return Some(window_id);
        }
    }

    None
}

fn scope_branches<'a>(
    candidates: &'a [WindowGeometryCandidate],
    scope_key: &str,
    scope_depth: usize,
) -> Vec<ScopeBranch<'a>> {
    let mut branches: Vec<ScopeBranch<'a>> = Vec::new();

    for candidate in candidates.iter().filter(|candidate| {
        candidate
            .scope_path
            .get(scope_depth)
            .is_some_and(|candidate_scope| candidate_scope == scope_key)
    }) {
        let key = if candidate.scope_path.len() > scope_depth + 1 {
            FocusBranchKey::Scope(candidate.scope_path[scope_depth + 1].clone())
        } else {
            FocusBranchKey::Window(candidate.window_id.clone())
        };

        if let Some(branch) = branches.iter_mut().find(|branch| branch.key == key) {
            branch.geometry = union_geometry(branch.geometry, candidate.geometry);
            branch.descendants.push(candidate);
            continue;
        }

        branches.push(ScopeBranch {
            scope_depth: match &key {
                FocusBranchKey::Scope(_) => Some(scope_depth + 1),
                FocusBranchKey::Window(_) => None,
            },
            key,
            geometry: candidate.geometry,
            descendants: vec![candidate],
        });
    }

    branches
}

fn infer_split_axis(branches: &[ScopeBranch<'_>]) -> Option<SplitAxis> {
    if branches.len() < 2 {
        return None;
    }

    let mut min_center_x = i32::MAX;
    let mut max_center_x = i32::MIN;
    let mut min_center_y = i32::MAX;
    let mut max_center_y = i32::MIN;

    for branch in branches {
        let center = rect_center(branch.geometry);
        min_center_x = min_center_x.min(center.0);
        max_center_x = max_center_x.max(center.0);
        min_center_y = min_center_y.min(center.1);
        max_center_y = max_center_y.max(center.1);
    }

    let x_span = max_center_x - min_center_x;
    let y_span = max_center_y - min_center_y;

    if x_span == 0 && y_span == 0 {
        None
    } else if x_span >= y_span {
        Some(SplitAxis::Horizontal)
    } else {
        Some(SplitAxis::Vertical)
    }
}

fn direction_matches_axis(direction: NavigationDirection, axis: SplitAxis) -> bool {
    matches!(
        (direction, axis),
        (
            NavigationDirection::Left | NavigationDirection::Right,
            SplitAxis::Horizontal,
        ) | (
            NavigationDirection::Up | NavigationDirection::Down,
            SplitAxis::Vertical,
        )
    )
}

fn sort_scope_branches(branches: &mut [ScopeBranch<'_>], axis: SplitAxis) {
    branches.sort_by_key(|branch| match axis {
        SplitAxis::Horizontal => (branch.geometry.x, branch.geometry.y),
        SplitAxis::Vertical => (branch.geometry.y, branch.geometry.x),
    });
}

fn current_branch_key(current: &WindowGeometryCandidate, scope_depth: usize) -> FocusBranchKey {
    if current.scope_path.len() > scope_depth + 1 {
        FocusBranchKey::Scope(current.scope_path[scope_depth + 1].clone())
    } else {
        FocusBranchKey::Window(current.window_id.clone())
    }
}

fn wrapped_branch_index(
    current_index: usize,
    branch_count: usize,
    direction: NavigationDirection,
) -> Option<usize> {
    if branch_count < 2 {
        return None;
    }

    Some(match direction {
        NavigationDirection::Left | NavigationDirection::Up => {
            current_index.checked_sub(1).unwrap_or(branch_count - 1)
        }
        NavigationDirection::Right | NavigationDirection::Down => {
            if current_index + 1 < branch_count {
                current_index + 1
            } else {
                0
            }
        }
    })
}

fn direction_matches_focus_axis(direction: NavigationDirection, axis: FocusAxis) -> bool {
    matches!(
        (direction, axis),
        (
            NavigationDirection::Left | NavigationDirection::Right,
            FocusAxis::Horizontal,
        ) | (
            NavigationDirection::Up | NavigationDirection::Down,
            FocusAxis::Vertical,
        )
    )
}

fn resolve_tree_branch_target(
    focus_tree: &FocusTree,
    branch: &FocusBranchKey,
    direction: NavigationDirection,
    remembered_focus_by_scope: &BTreeMap<String, WindowId>,
) -> Option<WindowId> {
    match branch {
        FocusBranchKey::Window(window_id) => Some(window_id.clone()),
        FocusBranchKey::Scope(scope_key) => {
            if let Some(remembered_window_id) = remembered_focus_by_scope.get(scope_key)
                && focus_tree
                    .descendants(scope_key)
                    .is_some_and(|descendants| descendants.contains(remembered_window_id))
            {
                return Some(remembered_window_id.clone());
            }

            default_focus_in_tree_scope(focus_tree, scope_key, direction, remembered_focus_by_scope)
        }
    }
}

fn default_focus_in_tree_scope(
    focus_tree: &FocusTree,
    scope_key: &str,
    direction: NavigationDirection,
    remembered_focus_by_scope: &BTreeMap<String, WindowId>,
) -> Option<WindowId> {
    let Some(navigation) = focus_tree.navigation(scope_key) else {
        let descendants = focus_tree.descendants(scope_key)?;
        return match direction {
            NavigationDirection::Left | NavigationDirection::Up => descendants.last().cloned(),
            NavigationDirection::Right | NavigationDirection::Down => descendants.first().cloned(),
        };
    };

    let branch = match direction {
        NavigationDirection::Left | NavigationDirection::Up => navigation.branches.last(),
        NavigationDirection::Right | NavigationDirection::Down => navigation.branches.first(),
    }?;

    resolve_tree_branch_target(focus_tree, branch, direction, remembered_focus_by_scope)
}

fn resolve_branch_target(
    candidates: &[WindowGeometryCandidate],
    branch: &ScopeBranch<'_>,
    direction: NavigationDirection,
    remembered_focus_by_scope: &BTreeMap<String, WindowId>,
) -> Option<WindowId> {
    match &branch.key {
        FocusBranchKey::Window(window_id) => Some(window_id.clone()),
        FocusBranchKey::Scope(scope_key) => {
            if let Some(remembered_window_id) = remembered_focus_by_scope.get(scope_key)
                && branch
                    .descendants
                    .iter()
                    .any(|candidate| candidate.window_id == *remembered_window_id)
            {
                return Some(remembered_window_id.clone());
            }

            default_focus_in_scope(
                candidates,
                scope_key,
                branch.scope_depth?,
                direction,
                remembered_focus_by_scope,
            )
        }
    }
}

fn default_focus_in_scope(
    candidates: &[WindowGeometryCandidate],
    scope_key: &str,
    scope_depth: usize,
    direction: NavigationDirection,
    remembered_focus_by_scope: &BTreeMap<String, WindowId>,
) -> Option<WindowId> {
    let mut branches = scope_branches(candidates, scope_key, scope_depth);
    if branches.is_empty() {
        return None;
    }

    branches.sort_by_key(|branch| match direction {
        NavigationDirection::Left | NavigationDirection::Right => {
            (branch.geometry.x, branch.geometry.y)
        }
        NavigationDirection::Up | NavigationDirection::Down => {
            (branch.geometry.y, branch.geometry.x)
        }
    });

    let branch = match direction {
        NavigationDirection::Left | NavigationDirection::Up => branches.last(),
        NavigationDirection::Right | NavigationDirection::Down => branches.first(),
    }?;

    resolve_branch_target(candidates, branch, direction, remembered_focus_by_scope)
}

fn select_geometric_candidate(
    candidates: &[WindowGeometryCandidate],
    current: &WindowGeometryCandidate,
    direction: NavigationDirection,
) -> Option<WindowId> {
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

fn rect_center(rect: WindowGeometry) -> (i32, i32) {
    (rect.x + rect.width / 2, rect.y + rect.height / 2)
}

fn union_geometry(left: WindowGeometry, right: WindowGeometry) -> WindowGeometry {
    let x1 = left.x.min(right.x);
    let y1 = left.y.min(right.y);
    let x2 = (left.x + left.width).max(right.x + right.width);
    let y2 = (left.y + left.height).max(right.y + right.height);

    WindowGeometry {
        x: x1,
        y: y1,
        width: x2 - x1,
        height: y2 - y1,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::window_id;

    fn candidate(
        id: u64,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        scope_path: &[&str],
    ) -> WindowGeometryCandidate {
        WindowGeometryCandidate {
            window_id: window_id(id),
            geometry: WindowGeometry {
                x,
                y,
                width,
                height,
            },
            scope_path: scope_path.iter().map(|scope| (*scope).to_string()).collect(),
        }
    }

    #[test]
    fn directional_focus_prefers_nearest_window_in_direction() {
        let candidates = vec![
            candidate(1, 0, 0, 100, 100, &["$workspace", "main"]),
            candidate(2, 140, 10, 100, 100, &["$workspace", "main"]),
            candidate(3, 320, 0, 100, 100, &["$workspace", "stack"]),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right,
                &BTreeMap::new(),
                None,
            ),
            Some(window_id(2))
        );
    }

    #[test]
    fn directional_focus_cycles_within_requested_axis() {
        let candidates = vec![
            candidate(1, 120, 120, 100, 100, &["$workspace", "main"]),
            candidate(2, 120, 0, 100, 100, &["$workspace", "main"]),
            candidate(3, 260, 120, 100, 100, &["$workspace", "stack"]),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Up,
                &BTreeMap::new(),
                None,
            ),
            Some(window_id(2))
        );
        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Left,
                &BTreeMap::new(),
                None,
            ),
            Some(window_id(3))
        );
    }

    #[test]
    fn directional_focus_prefers_lower_cross_axis_offset() {
        let candidates = vec![
            candidate(1, 100, 100, 100, 100, &["$workspace", "main"]),
            candidate(2, 260, 90, 100, 100, &["$workspace", "main"]),
            candidate(3, 250, 220, 100, 100, &["$workspace", "stack"]),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right,
                &BTreeMap::new(),
                None,
            ),
            Some(window_id(2))
        );
    }

    #[test]
    fn directional_focus_prefers_same_group_before_climbing() {
        let candidates = vec![
            candidate(1, 100, 100, 100, 100, &["$workspace", "main"]),
            candidate(2, 280, 105, 100, 100, &["$workspace", "main"]),
            candidate(3, 220, 100, 100, 100, &["$workspace", "stack"]),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right,
                &BTreeMap::new(),
                None,
            ),
            Some(window_id(2))
        );
    }

    #[test]
    fn directional_focus_climbs_to_parent_scope_when_group_has_no_match() {
        let candidates = vec![
            candidate(1, 100, 100, 100, 100, &["$workspace", "main"]),
            candidate(2, 100, 260, 100, 100, &["$workspace", "main"]),
            candidate(3, 260, 100, 100, 100, &["$workspace", "stack"]),
        ];

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right,
                &BTreeMap::new(),
                None,
            ),
            Some(window_id(3))
        );
    }

    #[test]
    fn directional_focus_descends_into_remembered_nested_branch() {
        let candidates = vec![
            candidate(1, 0, 0, 100, 400, &["$workspace"]),
            candidate(2, 100, 0, 100, 200, &["$workspace", "right"]),
            candidate(3, 100, 200, 50, 200, &["$workspace", "right", "bottom"]),
            candidate(4, 150, 200, 50, 200, &["$workspace", "right", "bottom"]),
        ];
        let mut remembered = BTreeMap::new();

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right,
                &remembered,
                None,
            ),
            Some(window_id(2))
        );

        remembered.insert("$workspace".to_string(), window_id(2));
        remembered.insert("right".to_string(), window_id(2));

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(2)),
                NavigationDirection::Down,
                &remembered,
                None,
            ),
            Some(window_id(3))
        );

        remembered.insert("$workspace".to_string(), window_id(4));
        remembered.insert("right".to_string(), window_id(4));
        remembered.insert("bottom".to_string(), window_id(4));

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(2)),
                NavigationDirection::Down,
                &remembered,
                None,
            ),
            Some(window_id(4))
        );

        remembered.insert("$workspace".to_string(), window_id(3));
        remembered.insert("right".to_string(), window_id(3));
        remembered.insert("bottom".to_string(), window_id(3));

        assert_eq!(
            select_directional_focus_candidate(
                &candidates,
                Some(window_id(1)),
                NavigationDirection::Right,
                &remembered,
                None,
            ),
            Some(window_id(3))
        );
    }

    #[test]
    fn directional_focus_replays_sway_memory_sequence() {
        use crate::focus::{FocusAxis, FocusBranchKey, FocusScopeNavigation, FocusTree};

        fn step(
            candidates: &[WindowGeometryCandidate],
            focused: &mut WindowId,
            remembered: &mut BTreeMap<String, WindowId>,
            focus_tree: &FocusTree,
            direction: NavigationDirection,
            expected: u64,
        ) {
            *focused = select_directional_focus_candidate(
                candidates,
                Some(focused.clone()),
                direction,
                remembered,
                Some(focus_tree),
            )
            .expect("directional target");

            let scope_path = candidates
                .iter()
                .find(|candidate| candidate.window_id == *focused)
                .expect("focused candidate")
                .scope_path
                .clone();

            for scope_key in scope_path {
                remembered.insert(scope_key, focused.clone());
            }

            assert_eq!(*focused, window_id(expected));
        }

        let candidates = vec![
            candidate(1, 0, 0, 600, 600, &["$workspace"]),
            candidate(2, 600, 0, 400, 300, &["$workspace", "$workspace/group[1]:right"]),
            candidate(
                3,
                600,
                300,
                200,
                300,
                &[
                    "$workspace",
                    "$workspace/group[1]:right",
                    "$workspace/group[1]:right/group[1]:bottom",
                ],
            ),
            candidate(
                4,
                800,
                300,
                200,
                300,
                &[
                    "$workspace",
                    "$workspace/group[1]:right",
                    "$workspace/group[1]:right/group[1]:bottom",
                ],
            ),
        ];
        let mut focus_tree = FocusTree::from_resolved_root(&crate::ResolvedLayoutNode::Workspace {
            meta: crate::LayoutNodeMeta::default(),
            children: vec![
                crate::ResolvedLayoutNode::Window {
                    meta: crate::LayoutNodeMeta::default(),
                    window_id: Some(window_id(1)),
                },
                crate::ResolvedLayoutNode::Group {
                    meta: crate::LayoutNodeMeta {
                        id: Some("right".into()),
                        ..crate::LayoutNodeMeta::default()
                    },
                    children: vec![
                        crate::ResolvedLayoutNode::Window {
                            meta: crate::LayoutNodeMeta::default(),
                            window_id: Some(window_id(2)),
                        },
                        crate::ResolvedLayoutNode::Group {
                            meta: crate::LayoutNodeMeta {
                                id: Some("bottom".into()),
                                ..crate::LayoutNodeMeta::default()
                            },
                            children: vec![
                                crate::ResolvedLayoutNode::Window {
                                    meta: crate::LayoutNodeMeta::default(),
                                    window_id: Some(window_id(3)),
                                },
                                crate::ResolvedLayoutNode::Window {
                                    meta: crate::LayoutNodeMeta::default(),
                                    window_id: Some(window_id(4)),
                                },
                            ],
                        },
                    ],
                },
            ],
        });
        focus_tree.set_navigation(
            [
                (
                    "$workspace".to_string(),
                    FocusScopeNavigation {
                        axis: FocusAxis::Horizontal,
                        branches: vec![
                            FocusBranchKey::Window(window_id(1)),
                            FocusBranchKey::Scope("$workspace/group[1]:right".to_string()),
                        ],
                    },
                ),
                (
                    "$workspace/group[1]:right".to_string(),
                    FocusScopeNavigation {
                        axis: FocusAxis::Vertical,
                        branches: vec![
                            FocusBranchKey::Window(window_id(2)),
                            FocusBranchKey::Scope(
                                "$workspace/group[1]:right/group[1]:bottom".to_string(),
                            ),
                        ],
                    },
                ),
                (
                    "$workspace/group[1]:right/group[1]:bottom".to_string(),
                    FocusScopeNavigation {
                        axis: FocusAxis::Horizontal,
                        branches: vec![
                            FocusBranchKey::Window(window_id(3)),
                            FocusBranchKey::Window(window_id(4)),
                        ],
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        let mut remembered = BTreeMap::new();
        let mut focused = window_id(1);

        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Right,
            2,
        );
        focused = window_id(1);
        remembered.insert("$workspace".to_string(), window_id(2));

        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Down,
            3,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Right,
            4,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Up,
            2,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Down,
            4,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Left,
            3,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Left,
            4,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Right,
            3,
        );
    }

    #[test]
    fn directional_focus_wraps_within_axis_and_preserves_column_memory() {
        use crate::focus::{FocusAxis, FocusBranchKey, FocusScopeNavigation, FocusTree};

        fn step(
            candidates: &[WindowGeometryCandidate],
            focused: &mut WindowId,
            remembered: &mut BTreeMap<String, WindowId>,
            focus_tree: &FocusTree,
            direction: NavigationDirection,
            expected: u64,
        ) {
            *focused = select_directional_focus_candidate(
                candidates,
                Some(focused.clone()),
                direction,
                remembered,
                Some(focus_tree),
            )
            .expect("directional target");

            let scope_path = candidates
                .iter()
                .find(|candidate| candidate.window_id == *focused)
                .expect("focused candidate")
                .scope_path
                .clone();

            for scope_key in scope_path {
                remembered.insert(scope_key, focused.clone());
            }

            assert_eq!(*focused, window_id(expected));
        }

        let main_scope = "$workspace/group[0]:main-column";
        let side_scope = "$workspace/group[1]:side-column";
        let candidates = vec![
            candidate(1, 0, 0, 2553, 702, &["$workspace", main_scope]),
            candidate(2, 0, 702, 2553, 702, &["$workspace", main_scope]),
            candidate(3, 2553, 0, 851, 464, &["$workspace", side_scope]),
            candidate(4, 2553, 464, 851, 464, &["$workspace", side_scope]),
            candidate(5, 2553, 928, 851, 464, &["$workspace", side_scope]),
        ];
        let mut focus_tree = FocusTree::from_resolved_root(&crate::ResolvedLayoutNode::Workspace {
            meta: crate::LayoutNodeMeta::default(),
            children: vec![
                crate::ResolvedLayoutNode::Group {
                    meta: crate::LayoutNodeMeta {
                        id: Some("main-column".into()),
                        ..crate::LayoutNodeMeta::default()
                    },
                    children: vec![
                        crate::ResolvedLayoutNode::Window {
                            meta: crate::LayoutNodeMeta::default(),
                            window_id: Some(window_id(1)),
                        },
                        crate::ResolvedLayoutNode::Window {
                            meta: crate::LayoutNodeMeta::default(),
                            window_id: Some(window_id(2)),
                        },
                    ],
                },
                crate::ResolvedLayoutNode::Group {
                    meta: crate::LayoutNodeMeta {
                        id: Some("side-column".into()),
                        ..crate::LayoutNodeMeta::default()
                    },
                    children: vec![
                        crate::ResolvedLayoutNode::Window {
                            meta: crate::LayoutNodeMeta::default(),
                            window_id: Some(window_id(3)),
                        },
                        crate::ResolvedLayoutNode::Window {
                            meta: crate::LayoutNodeMeta::default(),
                            window_id: Some(window_id(4)),
                        },
                        crate::ResolvedLayoutNode::Window {
                            meta: crate::LayoutNodeMeta::default(),
                            window_id: Some(window_id(5)),
                        },
                    ],
                },
            ],
        });
        focus_tree.set_navigation(
            [
                (
                    "$workspace".to_string(),
                    FocusScopeNavigation {
                        axis: FocusAxis::Horizontal,
                        branches: vec![
                            FocusBranchKey::Scope(main_scope.to_string()),
                            FocusBranchKey::Scope(side_scope.to_string()),
                        ],
                    },
                ),
                (
                    main_scope.to_string(),
                    FocusScopeNavigation {
                        axis: FocusAxis::Vertical,
                        branches: vec![
                            FocusBranchKey::Window(window_id(1)),
                            FocusBranchKey::Window(window_id(2)),
                        ],
                    },
                ),
                (
                    side_scope.to_string(),
                    FocusScopeNavigation {
                        axis: FocusAxis::Vertical,
                        branches: vec![
                            FocusBranchKey::Window(window_id(3)),
                            FocusBranchKey::Window(window_id(4)),
                            FocusBranchKey::Window(window_id(5)),
                        ],
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        let mut remembered = BTreeMap::new();
        let mut focused = window_id(1);
        remembered.insert("$workspace".to_string(), focused.clone());
        remembered.insert(main_scope.to_string(), focused.clone());

        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Down,
            2,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Down,
            1,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Right,
            3,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Down,
            4,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Down,
            5,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Left,
            1,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Right,
            5,
        );
        step(
            &candidates,
            &mut focused,
            &mut remembered,
            &focus_tree,
            NavigationDirection::Right,
            1,
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