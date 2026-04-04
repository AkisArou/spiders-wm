use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use spiders_core::command::WmCommand;
use spiders_core::command::FocusDirection;
use spiders_core::focus::{
    FocusScopePath, FocusTree, FocusTreeWindowGeometry, remove_window,
    request_focus_next_window, request_focus_previous_window, request_focus_window,
    set_focused_window,
};
use spiders_core::navigation::{
    NavigationDirection, WindowGeometryCandidate, managed_window_swap_positions,
    select_directional_focus_candidate,
};
use spiders_core::resize::{
    LayoutAdjustmentState, MAX_SPLIT_WEIGHT, MIN_SPLIT_WEIGHT, resize_split_weights,
};
use spiders_core::wm::{WindowGeometry, WmModel};
use spiders_core::workspace::{
    ensure_workspace, request_select_next_workspace, request_select_previous_workspace,
    request_select_workspace,
};
use spiders_core::{OutputId, WindowId, WorkspaceId};
use spiders_core::LayoutId;

use crate::PreviewSnapshotNode;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSession {
    pub active_layout: LayoutId,
    pub active_workspace_name: String,
    pub workspace_names: Vec<String>,
    pub windows: Vec<PreviewWindow>,
    #[serde(default)]
    pub remembered_focus_by_scope: BTreeMap<String, WindowId>,
    #[serde(default)]
    pub layout_adjustments: LayoutAdjustmentState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewWindow {
    pub id: String,
    #[serde(default, rename = "app_id", alias = "appId")]
    pub app_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default, rename = "window_type", alias = "windowType")]
    pub window_type: Option<String>,
    #[serde(default)]
    pub floating: bool,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub focused: bool,
    pub workspace_name: String,
}

pub fn apply_preview_command(
    mut state: PreviewSession,
    command: WmCommand,
    snapshot_root: Option<&PreviewSnapshotNode>,
) -> PreviewSession {
    normalize_preview_state(&mut state);

    let mut model = preview_model_with_snapshot(&state, snapshot_root);

    match command {
        WmCommand::Quit => {}
        WmCommand::ViewWorkspace { workspace } => {
            select_workspace_by_name(
                &state,
                &mut model,
                workspace_name_by_index(&state.workspace_names, workspace),
            );
        }
        WmCommand::SelectWorkspace { workspace_id } | WmCommand::ActivateWorkspace { workspace_id } => {
            select_workspace_by_name(&state, &mut model, Some(workspace_id.as_str().to_string()));
        }
        WmCommand::SelectNextWorkspace => {
            let ordered_window_ids = ordered_window_ids(&state);
            if let Some(selection) = request_select_next_workspace(&mut model, ordered_window_ids) {
                let _ = set_focused_window(&mut model, selection.focused_window_id);
            }
        }
        WmCommand::SelectPreviousWorkspace => {
            let ordered_window_ids = ordered_window_ids(&state);
            if let Some(selection) =
                request_select_previous_workspace(&mut model, ordered_window_ids)
            {
                let _ = set_focused_window(&mut model, selection.focused_window_id);
            }
        }
        WmCommand::AssignFocusedWindowToWorkspace { workspace }
        | WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace } => {
            if let Some(target_name) = workspace_name_by_index(&state.workspace_names, workspace) {
                assign_focused_window_to_workspace(
                    &mut model,
                    WorkspaceId(target_name),
                    ordered_window_ids(&state),
                );
            }
        }
        WmCommand::FocusDirection { direction } => {
            focus_direction(&mut model, snapshot_root, direction);
        }
        WmCommand::SwapDirection { direction } => {
            swap_direction(&mut state, &mut model, snapshot_root, direction);
        }
        WmCommand::FocusNextWindow => {
            let selection = request_focus_next_window(&mut model, ordered_window_ids(&state));
            let _ = set_focused_window(&mut model, selection.focused_window_id);
        }
        WmCommand::FocusPreviousWindow => {
            let selection = request_focus_previous_window(&mut model, ordered_window_ids(&state));
            let _ = set_focused_window(&mut model, selection.focused_window_id);
        }
        WmCommand::FocusWindow { window_id } => {
            let selection = request_focus_window(&mut model, Some(window_id));
            let _ = set_focused_window(&mut model, selection.focused_window_id);
        }
        WmCommand::ToggleFloating => {
            toggle_focused_window_floating(&mut model);
        }
        WmCommand::CloseFocusedWindow => {
            close_focused_window(&mut state, &mut model);
        }
        WmCommand::Spawn { command } => {
            if command == "foot" {
                spawn_foot_window(&mut state, &mut model);
            }
        }
        WmCommand::SpawnTerminal => {
            spawn_foot_window(&mut state, &mut model);
        }
        WmCommand::ResizeDirection { direction } | WmCommand::ResizeTiledDirection { direction } => {
            resize_direction(&mut state, &model, snapshot_root, direction);
        }
        WmCommand::ReloadConfig
        | WmCommand::ToggleViewWorkspace { .. }
        | WmCommand::AssignWorkspace { .. }
        | WmCommand::FocusMonitorLeft
        | WmCommand::FocusMonitorRight
        | WmCommand::SendMonitorLeft
        | WmCommand::SendMonitorRight
        | WmCommand::ToggleFullscreen
        | WmCommand::SetFloatingWindowGeometry { .. }
        | WmCommand::MoveDirection { .. } => {}
        WmCommand::SetLayout { name } => {
            state.active_layout = LayoutId::from(name.as_str());
        }
        WmCommand::CycleLayout { direction } => {
            state.active_layout = cycle_preview_layout(
                &state.active_layout,
                direction.unwrap_or(spiders_core::command::LayoutCycleDirection::Next),
            );
        }
    }

    sync_preview_state(&mut state, &model);
    state
}

pub fn select_preview_workspace(
    mut state: PreviewSession,
    workspace_name: &str,
    snapshot_root: Option<&PreviewSnapshotNode>,
) -> PreviewSession {
    normalize_preview_state(&mut state);

    if state.active_workspace_name == workspace_name
        || !state.workspace_names.iter().any(|name| name == workspace_name)
    {
        return state;
    }

    let ordered_window_ids = ordered_window_ids(&state);
    let mut model = preview_model_with_snapshot(&state, snapshot_root);

    if let Some(selection) = request_select_workspace(
        &mut model,
        WorkspaceId::from(workspace_name),
        ordered_window_ids,
    ) {
        let _ = set_focused_window(&mut model, selection.focused_window_id);
    }

    sync_preview_state(&mut state, &model);
    state
}

pub fn set_preview_focused_window(
    mut state: PreviewSession,
    focused_window_id: Option<WindowId>,
    snapshot_root: Option<&PreviewSnapshotNode>,
) -> PreviewSession {
    normalize_preview_state(&mut state);

    let mut model = preview_model_with_snapshot(&state, snapshot_root);

    if let Some(window_id) = focused_window_id.as_ref() {
        let Some(window) = state.windows.iter().find(|window| window.id == window_id.as_str()) else {
            return state;
        };

        model.set_current_workspace(WorkspaceId::from(window.workspace_name.as_str()));
    }

    let _ = set_focused_window(&mut model, focused_window_id);
    sync_preview_state(&mut state, &model);
    state
}

fn normalize_preview_state(state: &mut PreviewSession) {
    if state.workspace_names.is_empty() {
        state.workspace_names.push(if state.active_workspace_name.is_empty() {
            "1:dev".to_string()
        } else {
            state.active_workspace_name.clone()
        });
    }

    if state.active_layout.as_str().is_empty() {
        state.active_layout = LayoutId::from("master-stack");
    }

    if !state.workspace_names.contains(&state.active_workspace_name) {
        state.active_workspace_name = state
            .workspace_names
            .first()
            .cloned()
            .unwrap_or_else(|| "1:dev".to_string());
    }
}

fn cycle_preview_layout(
    current: &LayoutId,
    direction: spiders_core::command::LayoutCycleDirection,
) -> LayoutId {
    const PREVIEW_LAYOUT_IDS: [&str; 2] = ["master-stack", "focus-repro"];

    let index = PREVIEW_LAYOUT_IDS
        .iter()
        .position(|layout_id| *layout_id == current.as_str())
        .unwrap_or(0);

    match direction {
        spiders_core::command::LayoutCycleDirection::Next => {
            LayoutId::from(PREVIEW_LAYOUT_IDS[(index + 1) % PREVIEW_LAYOUT_IDS.len()])
        }
        spiders_core::command::LayoutCycleDirection::Previous => LayoutId::from(
            PREVIEW_LAYOUT_IDS[(index + PREVIEW_LAYOUT_IDS.len() - 1) % PREVIEW_LAYOUT_IDS.len()],
        ),
    }
}

fn preview_model(state: &PreviewSession) -> WmModel {
    let mut model = WmModel::default();
    let output_id = OutputId::from("preview-output");

    model.upsert_output(output_id.clone(), "preview-output", 0, 0, None);
    model.set_current_output(output_id.clone());

    for workspace_name in &state.workspace_names {
        ensure_workspace(&mut model, workspace_name.clone());
    }

    model.set_current_workspace(WorkspaceId::from(state.active_workspace_name.as_str()));

    for window in &state.windows {
        let window_id = WindowId::from(window.id.as_str());
        let workspace_id = WorkspaceId::from(window.workspace_name.as_str());

        model.insert_window(
            window_id.clone(),
            Some(workspace_id),
            Some(output_id.clone()),
        );
        model.set_window_mapped(window_id.clone(), true);
        model.set_window_identity(window_id.clone(), window.title.clone(), window.app_id.clone());
        model.set_window_floating(window_id.clone(), window.floating);
        model.set_window_fullscreen(window_id.clone(), window.fullscreen);
    }

    model.last_focused_window_id_by_scope =
        remembered_focus_from_serialized(&state.remembered_focus_by_scope);

    let focused_window_id = state
        .windows
        .iter()
        .find(|window| window.focused)
        .map(|window| WindowId::from(window.id.as_str()));
    let _ = set_focused_window(&mut model, focused_window_id);

    model
}

fn preview_model_with_snapshot(
    state: &PreviewSession,
    snapshot_root: Option<&PreviewSnapshotNode>,
) -> WmModel {
    let mut model = preview_model(state);
    let current_focused_window_id = model.focused_window_id().cloned();

    if let Some(snapshot_root) = snapshot_root {
        model.set_focus_tree_value(Some(focus_tree_from_preview_snapshot(snapshot_root)));
    }

    if let Some(focused_window_id) = current_focused_window_id {
        let _ = set_focused_window(&mut model, Some(focused_window_id));
    }

    model
}

fn ordered_window_ids(state: &PreviewSession) -> Vec<WindowId> {
    state
        .windows
        .iter()
        .map(|window| WindowId::from(window.id.as_str()))
        .collect()
}

fn workspace_name_by_index(workspace_names: &[String], workspace_index: u8) -> Option<String> {
    if workspace_index == 0 {
        return None;
    }

    workspace_names.get(workspace_index as usize - 1).cloned()
}

fn select_workspace_by_name(
    state: &PreviewSession,
    model: &mut WmModel,
    workspace_name: Option<String>,
) {
    let Some(workspace_name) = workspace_name else {
        return;
    };
    let ordered_window_ids = ordered_window_ids(state);

    if let Some(selection) =
        request_select_workspace(model, WorkspaceId(workspace_name), ordered_window_ids)
    {
        let _ = set_focused_window(model, selection.focused_window_id);
    }
}

fn focus_direction(
    model: &mut WmModel,
    snapshot_root: Option<&PreviewSnapshotNode>,
    direction: FocusDirection,
) {
    let Some(target_window_id) = directional_target_window(model, snapshot_root, direction) else {
        return;
    };

    let _ = set_focused_window(model, Some(target_window_id));
}

fn resize_direction(
    state: &mut PreviewSession,
    model: &WmModel,
    snapshot_root: Option<&PreviewSnapshotNode>,
    direction: FocusDirection,
) {
    let Some(snapshot_root) = snapshot_root else {
        return;
    };
    let Some(focused_window_id) = model.focused_window_id().cloned() else {
        return;
    };
    let Some(target) = split_resize_target(snapshot_root, &focused_window_id, direction) else {
        return;
    };

    let default_weights = inferred_split_weights(target.node);
    let current_weights = state
        .layout_adjustments
        .split_weights_by_node_id
        .get(target.node_id.as_str())
        .map(Vec::as_slice);
    let Some(weights) = resize_split_weights(
        current_weights,
        target.child_count,
        &default_weights,
        target.grow_child_index,
        target.shrink_child_index,
    ) else {
        return;
    };

    state
        .layout_adjustments
        .split_weights_by_node_id
        .insert(target.node_id, weights);
}

fn swap_direction(
    state: &mut PreviewSession,
    model: &mut WmModel,
    snapshot_root: Option<&PreviewSnapshotNode>,
    direction: FocusDirection,
) {
    let Some(focused_window_id) = model.focused_window_id().cloned() else {
        return;
    };
    let Some(target_window_id) = directional_target_window(model, snapshot_root, direction) else {
        return;
    };

    let candidate_ids = directional_candidate_ids(state, snapshot_root);
    let Some((first_index, second_index)) = managed_window_swap_positions(
        &candidate_ids,
        focused_window_id.clone(),
        target_window_id,
    ) else {
        return;
    };

    let first_global_index = state
        .windows
        .iter()
        .position(|window| window.id == candidate_ids[first_index].as_str());
    let second_global_index = state
        .windows
        .iter()
        .position(|window| window.id == candidate_ids[second_index].as_str());

    let (Some(first_global_index), Some(second_global_index)) =
        (first_global_index, second_global_index)
    else {
        return;
    };

    state.windows.swap(first_global_index, second_global_index);
    let _ = set_focused_window(model, Some(focused_window_id));
}

fn directional_candidate_ids(
    state: &PreviewSession,
    snapshot_root: Option<&PreviewSnapshotNode>,
) -> Vec<WindowId> {
    let mut snapshot_ids = Vec::new();

    if let Some(snapshot_root) = snapshot_root {
        collect_snapshot_window_ids(snapshot_root, &mut snapshot_ids);
    }

    if snapshot_ids.is_empty() {
        return Vec::new();
    }

    state
        .windows
        .iter()
        .filter(|window| !window.floating && snapshot_ids.iter().any(|id| id.as_str() == window.id))
        .map(|window| WindowId::from(window.id.as_str()))
        .collect()
}

fn directional_target_window(
    model: &WmModel,
    snapshot_root: Option<&PreviewSnapshotNode>,
    direction: FocusDirection,
) -> Option<WindowId> {
    let snapshot_root = snapshot_root?;
    let candidates = snapshot_window_geometry_candidates(model, snapshot_root);

    select_directional_focus_candidate(
        &candidates,
        model.focused_window_id().cloned(),
        navigation_direction(direction),
        &model.last_focused_window_id_by_scope,
        model.focus_tree.as_ref(),
    )
}

fn focus_tree_from_preview_snapshot(root: &PreviewSnapshotNode) -> FocusTree {
    let mut windows = Vec::new();
    collect_preview_focus_tree_window_geometries(root, &mut windows);
    FocusTree::from_window_geometries(&windows)
}

fn collect_preview_focus_tree_window_geometries(
    node: &PreviewSnapshotNode,
    out: &mut Vec<FocusTreeWindowGeometry>,
) {
    if node.node_type == "window"
        && let (Some(window_id), Some(rect)) = (node.window_id.as_ref(), node.rect)
    {
        out.push(FocusTreeWindowGeometry {
            window_id: window_id.clone(),
            geometry: WindowGeometry {
                x: rect.x.round() as i32,
                y: rect.y.round() as i32,
                width: rect.width.round() as i32,
                height: rect.height.round() as i32,
            },
        });
    }

    for child in &node.children {
        collect_preview_focus_tree_window_geometries(child, out);
    }
}

fn snapshot_window_geometry_candidates(
    model: &WmModel,
    root: &PreviewSnapshotNode,
) -> Vec<WindowGeometryCandidate> {
    let mut windows = Vec::new();
    collect_preview_focus_tree_window_geometries(root, &mut windows);

    windows
        .into_iter()
        .map(|entry| WindowGeometryCandidate {
            scope_path: model
                .focus_scope_path(&entry.window_id)
                .map(|scope_path| scope_path.to_vec())
                .unwrap_or_else(|| vec![FocusTree::workspace_scope()]),
            window_id: entry.window_id,
            geometry: entry.geometry,
        })
        .collect()
}

fn collect_snapshot_window_ids(node: &PreviewSnapshotNode, out: &mut Vec<WindowId>) {
    if node.node_type == "window" && let Some(window_id) = node.window_id.as_ref() {
        out.push(window_id.clone());
    }

    for child in &node.children {
        collect_snapshot_window_ids(child, out);
    }
}

fn navigation_direction(direction: FocusDirection) -> NavigationDirection {
    match direction {
        FocusDirection::Left => NavigationDirection::Left,
        FocusDirection::Right => NavigationDirection::Right,
        FocusDirection::Up => NavigationDirection::Up,
        FocusDirection::Down => NavigationDirection::Down,
    }
}

struct SplitResizeTarget<'a> {
    node_id: String,
    node: &'a PreviewSnapshotNode,
    child_count: usize,
    grow_child_index: usize,
    shrink_child_index: usize,
}

fn split_resize_target<'a>(
    root: &'a PreviewSnapshotNode,
    focused_window_id: &WindowId,
    direction: FocusDirection,
) -> Option<SplitResizeTarget<'a>> {
    let mut child_path = Vec::new();
    if !window_child_path(root, focused_window_id, &mut child_path) {
        return None;
    }

    let mut nodes = vec![root];
    let mut node = root;
    for child_index in &child_path {
        node = node.children.get(*child_index)?;
        nodes.push(node);
    }

    for depth in (0..child_path.len()).rev() {
        let node = nodes[depth];
        let focused_child_index = child_path[depth];
        if let Some(target) = split_resize_target_for_node(node, focused_child_index, direction) {
            return Some(target);
        }
    }

    None
}

fn window_child_path(
    node: &PreviewSnapshotNode,
    focused_window_id: &WindowId,
    out: &mut Vec<usize>,
) -> bool {
    if node.node_type == "window" && node.window_id.as_ref() == Some(focused_window_id) {
        return true;
    }

    for (index, child) in node.children.iter().enumerate() {
        out.push(index);
        if window_child_path(child, focused_window_id, out) {
            return true;
        }
        out.pop();
    }

    false
}

fn split_resize_target_for_node<'a>(
    node: &'a PreviewSnapshotNode,
    focused_child_index: usize,
    direction: FocusDirection,
) -> Option<SplitResizeTarget<'a>> {
    let axis = match direction {
        FocusDirection::Left | FocusDirection::Right => "horizontal",
        FocusDirection::Up | FocusDirection::Down => "vertical",
    };
    if node.axis.as_deref()? != axis || node.children.len() < 2 {
        return None;
    }

    let node_id = node.id.clone()?;
    let visual_order = if node.reverse {
        (0..node.children.len()).rev().collect::<Vec<_>>()
    } else {
        (0..node.children.len()).collect::<Vec<_>>()
    };
    let focused_position = visual_order
        .iter()
        .position(|index| *index == focused_child_index)?;
    let preferred_step: isize = match direction {
        FocusDirection::Left | FocusDirection::Up => -1,
        FocusDirection::Right | FocusDirection::Down => 1,
    };

    let preferred_neighbor = visual_neighbor_index(&visual_order, focused_position, preferred_step);
    let opposite_neighbor = visual_neighbor_index(&visual_order, focused_position, -preferred_step);

    let (grow_child_index, shrink_child_index) = if let Some(neighbor_index) = preferred_neighbor {
        (focused_child_index, neighbor_index)
    } else {
        let neighbor_index = opposite_neighbor?;
        (neighbor_index, focused_child_index)
    };

    Some(SplitResizeTarget {
        node_id,
        node,
        child_count: node.children.len(),
        grow_child_index,
        shrink_child_index,
    })
}

fn visual_neighbor_index(
    visual_order: &[usize],
    focused_position: usize,
    step: isize,
) -> Option<usize> {
    let neighbor_position = focused_position as isize + step;
    if neighbor_position < 0 || neighbor_position >= visual_order.len() as isize {
        return None;
    }

    visual_order.get(neighbor_position as usize).copied()
}

fn inferred_split_weights(node: &PreviewSnapshotNode) -> Vec<u16> {
    let axis = node.axis.as_deref().unwrap_or("horizontal");
    let lengths = node
        .children
        .iter()
        .map(|child| {
            child
                .rect
                .map(|rect| if axis == "vertical" { rect.height } else { rect.width })
                .unwrap_or(0.0)
                .max(0.0)
        })
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<f32>();

    if lengths.is_empty() || total <= 0.0 {
        return vec![MIN_SPLIT_WEIGHT; node.children.len()];
    }

    let ratios = lengths.iter().map(|length| *length / total).collect::<Vec<_>>();
    let mut best_scale = MIN_SPLIT_WEIGHT;
    let mut best_error = f32::INFINITY;
    let mut best_weights = vec![MIN_SPLIT_WEIGHT; lengths.len()];

    for scale in MIN_SPLIT_WEIGHT..=MAX_SPLIT_WEIGHT {
        let weights = ratios
            .iter()
            .map(|ratio| ((ratio * scale as f32).round() as u16).clamp(MIN_SPLIT_WEIGHT, MAX_SPLIT_WEIGHT))
            .collect::<Vec<_>>();
        let weight_total = weights.iter().map(|weight| *weight as f32).sum::<f32>();
        let error = ratios
            .iter()
            .zip(weights.iter())
            .map(|(ratio, weight)| (ratio - (*weight as f32 / weight_total)).abs())
            .sum::<f32>();

        if error < best_error - 0.0001 || ((error - best_error).abs() <= 0.0001 && scale > best_scale) {
            best_scale = scale;
            best_error = error;
            best_weights = weights;
        }
    }

    best_weights
}

fn assign_focused_window_to_workspace(
    model: &mut WmModel,
    workspace_id: WorkspaceId,
    ordered_window_ids: Vec<WindowId>,
) {
    let Some(focused_window_id) = model.focused_window_id().cloned() else {
        return;
    };

    model.set_window_workspace(focused_window_id, Some(workspace_id));
    let next_focus = model.preferred_focus_window_on_current_workspace(ordered_window_ids);
    let _ = set_focused_window(model, next_focus);
}

fn toggle_focused_window_floating(model: &mut WmModel) {
    let Some(focused_window_id) = model.focused_window_id().cloned() else {
        return;
    };

    let next_floating = model
        .windows
        .get(&focused_window_id)
        .map(|window| !window.floating)
        .unwrap_or(false);
    model.set_window_floating(focused_window_id, next_floating);
}

fn close_focused_window(state: &mut PreviewSession, model: &mut WmModel) {
    let Some(focused_window_id) = model.focused_window_id().cloned() else {
        return;
    };

    let ordered_window_ids = ordered_window_ids(state);
    let _ = remove_window(model, focused_window_id.clone(), ordered_window_ids);
    state
        .windows
        .retain(|window| window.id != focused_window_id.as_str());
}

fn spawn_foot_window(state: &mut PreviewSession, model: &mut WmModel) {
    let terminal_number = next_terminal_title_number(state);
    let window_id = format!("win-{}", next_window_id_number(state));
    let current_workspace = model
        .current_workspace_id()
        .cloned()
        .unwrap_or_else(|| WorkspaceId::from(state.active_workspace_name.as_str()));

    state.windows.push(PreviewWindow {
        id: window_id.clone(),
        app_id: Some("foot".to_string()),
        title: Some(format!("Terminal {terminal_number}")),
        class: Some("foot".to_string()),
        instance: Some("foot".to_string()),
        role: None,
        shell: Some("xdg_toplevel".to_string()),
        window_type: None,
        floating: false,
        fullscreen: false,
        focused: false,
        workspace_name: current_workspace.as_str().to_string(),
    });

    let output_id = model.current_output_id().cloned();
    model.insert_window(WindowId::from(window_id.as_str()), Some(current_workspace), output_id);
    model.set_window_identity(
        WindowId::from(window_id.as_str()),
        Some(format!("Terminal {terminal_number}")),
        Some("foot".to_string()),
    );
    let _ = set_focused_window(model, Some(WindowId::from(window_id.as_str())));
}

fn next_terminal_title_number(state: &PreviewSession) -> u32 {
    state
        .windows
        .iter()
        .filter_map(|window| {
            window
                .title
                .as_deref()
                .and_then(|title| title.strip_prefix("Terminal "))
                .and_then(|suffix| suffix.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1
}

fn next_window_id_number(state: &PreviewSession) -> u32 {
    state
        .windows
        .iter()
        .filter_map(|window| {
            window
                .id
                .strip_prefix("win-")
                .and_then(|suffix| suffix.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1
}

fn sync_preview_state(state: &mut PreviewSession, model: &WmModel) {
    if let Some(current_workspace_id) = model.current_workspace_id() {
        state.active_workspace_name = current_workspace_id.as_str().to_string();
    }

    state.remembered_focus_by_scope =
        remembered_focus_to_serialized(&model.last_focused_window_id_by_scope);

    for window in &mut state.windows {
        let window_id = WindowId::from(window.id.as_str());

        if let Some(model_window) = model.windows.get(&window_id) {
            window.focused = model_window.focused;
            window.floating = model_window.floating;
            window.fullscreen = model_window.fullscreen;

            if let Some(workspace_id) = model_window.workspace_id.as_ref() {
                window.workspace_name = workspace_id.as_str().to_string();
            }
        }
    }
}

fn remembered_focus_from_serialized(
    remembered_focus_by_scope: &BTreeMap<String, WindowId>,
) -> BTreeMap<FocusScopePath, WindowId> {
    remembered_focus_by_scope
        .iter()
        .filter_map(|(scope_key, window_id)| {
            scope_key
                .parse::<FocusScopePath>()
                .ok()
                .map(|scope_path| (scope_path, window_id.clone()))
        })
        .collect()
}

fn remembered_focus_to_serialized(
    remembered_focus_by_scope: &BTreeMap<FocusScopePath, WindowId>,
) -> BTreeMap<String, WindowId> {
    remembered_focus_by_scope
        .iter()
        .map(|(scope_path, window_id)| (scope_path.to_string(), window_id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::command::WmCommand;

    fn focus_command(direction: FocusDirection) -> WmCommand {
        WmCommand::FocusDirection { direction }
    }

    fn preview_window(id: &str, title: &str, focused: bool) -> PreviewWindow {
        PreviewWindow {
            id: id.to_string(),
            app_id: Some("foot".to_string()),
            title: Some(title.to_string()),
            class: Some("foot".to_string()),
            instance: Some("foot".to_string()),
            role: None,
            shell: Some("xdg_toplevel".to_string()),
            window_type: None,
            floating: false,
            fullscreen: false,
            focused,
            workspace_name: "1".to_string(),
        }
    }

    fn window_node(window_id: &str, x: f32, y: f32, width: f32, height: f32) -> PreviewSnapshotNode {
        PreviewSnapshotNode {
            node_type: "window".to_string(),
            id: None,
            class_name: None,
            rect: Some(spiders_core::LayoutRect {
                x,
                y,
                width,
                height,
            }),
            window_id: Some(WindowId::from(window_id)),
            axis: None,
            reverse: false,
            children: Vec::new(),
        }
    }

    fn focus_repro_snapshot() -> PreviewSnapshotNode {
        PreviewSnapshotNode {
            node_type: "workspace".to_string(),
            id: Some("frame".to_string()),
            class_name: None,
            rect: Some(spiders_core::LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 3440.0,
                height: 1440.0,
            }),
            window_id: None,
            axis: Some("horizontal".to_string()),
            reverse: false,
            children: vec![
                PreviewSnapshotNode {
                    node_type: "group".to_string(),
                    id: Some("main-column".to_string()),
                    class_name: None,
                    rect: Some(spiders_core::LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 2553.0,
                        height: 1416.0,
                    }),
                    window_id: None,
                    axis: Some("vertical".to_string()),
                    reverse: false,
                    children: vec![
                        window_node("win-1", 0.0, 0.0, 2553.0, 702.0),
                        window_node("win-2", 0.0, 702.0, 2553.0, 702.0),
                    ],
                },
                PreviewSnapshotNode {
                    node_type: "group".to_string(),
                    id: Some("side-column".to_string()),
                    class_name: None,
                    rect: Some(spiders_core::LayoutRect {
                        x: 2553.0,
                        y: 0.0,
                        width: 851.0,
                        height: 1416.0,
                    }),
                    window_id: None,
                    axis: Some("vertical".to_string()),
                    reverse: false,
                    children: vec![
                        window_node("win-3", 2553.0, 0.0, 851.0, 464.0),
                        window_node("win-4", 2553.0, 464.0, 851.0, 464.0),
                        window_node("win-5", 2553.0, 928.0, 851.0, 464.0),
                    ],
                },
            ],
        }
    }

    fn initial_preview_state() -> PreviewSession {
        PreviewSession {
            active_layout: LayoutId::from("master-stack"),
            active_workspace_name: "1".to_string(),
            workspace_names: vec!["1".to_string()],
            windows: vec![
                preview_window("win-1", "Terminal 1", false),
                preview_window("win-2", "Spec Draft", false),
                preview_window("win-3", "Engineering", false),
                preview_window("win-4", "Terminal 4", false),
                preview_window("win-5", "Terminal 5", true),
            ],
            remembered_focus_by_scope: BTreeMap::new(),
            layout_adjustments: LayoutAdjustmentState::default(),
        }
    }

    fn resize_command(direction: FocusDirection) -> WmCommand {
        WmCommand::ResizeDirection { direction }
    }

    fn master_stack_snapshot() -> PreviewSnapshotNode {
        PreviewSnapshotNode {
            node_type: "workspace".to_string(),
            id: Some("root".to_string()),
            class_name: None,
            rect: Some(spiders_core::LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 3440.0,
                height: 1440.0,
            }),
            window_id: None,
            axis: Some("horizontal".to_string()),
            reverse: false,
            children: vec![PreviewSnapshotNode {
                node_type: "group".to_string(),
                id: Some("frame".to_string()),
                class_name: None,
                rect: Some(spiders_core::LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 3440.0,
                    height: 1440.0,
                }),
                window_id: None,
                axis: Some("horizontal".to_string()),
                reverse: false,
                children: vec![
                    window_node("win-1", 0.0, 0.0, 2064.0, 1440.0),
                    PreviewSnapshotNode {
                        node_type: "group".to_string(),
                        id: Some("stack".to_string()),
                        class_name: None,
                        rect: Some(spiders_core::LayoutRect {
                            x: 2064.0,
                            y: 0.0,
                            width: 1376.0,
                            height: 1440.0,
                        }),
                        window_id: None,
                        axis: Some("vertical".to_string()),
                        reverse: false,
                        children: vec![
                            window_node("win-2", 2064.0, 0.0, 1376.0, 480.0),
                            window_node("win-3", 2064.0, 480.0, 1376.0, 480.0),
                            window_node("win-5", 2064.0, 960.0, 1376.0, 480.0),
                        ],
                    },
                ],
            }],
        }
    }

    fn focused_window_id(state: &PreviewSession) -> &str {
        state
            .windows
            .iter()
            .find(|window| window.focused)
            .map(|window| window.id.as_str())
            .expect("focused window")
    }

    #[test]
    fn preview_focus_commands_preserve_side_memory_across_multiple_steps() {
        let snapshot_root = focus_repro_snapshot();
        let main_scope = "$workspace/visual[0]";
        let side_scope = "$workspace/visual[1]";

        let state = apply_preview_command(
            initial_preview_state(),
            focus_command(FocusDirection::Right),
            Some(&snapshot_root),
        );
        assert_eq!(focused_window_id(&state), "win-1");
        assert_eq!(
            state.remembered_focus_by_scope.get(side_scope),
            Some(&WindowId::from("win-5"))
        );

        let state = apply_preview_command(
            state,
            focus_command(FocusDirection::Down),
            Some(&snapshot_root),
        );
        assert_eq!(focused_window_id(&state), "win-2");
        assert_eq!(
            state.remembered_focus_by_scope.get(side_scope),
            Some(&WindowId::from("win-5"))
        );
        assert_eq!(
            state.remembered_focus_by_scope.get(main_scope),
            Some(&WindowId::from("win-2"))
        );

        let state = apply_preview_command(
            state,
            focus_command(FocusDirection::Down),
            Some(&snapshot_root),
        );
        assert_eq!(focused_window_id(&state), "win-1");
        assert_eq!(
            state.remembered_focus_by_scope.get(side_scope),
            Some(&WindowId::from("win-5"))
        );

        let state = apply_preview_command(
            state,
            focus_command(FocusDirection::Right),
            Some(&snapshot_root),
        );
        assert_eq!(focused_window_id(&state), "win-5");
        assert_eq!(
            state.remembered_focus_by_scope.get(main_scope),
            Some(&WindowId::from("win-1"))
        );
    }

    #[test]
    fn direct_focus_selection_updates_workspace_and_remembered_scope_focus() {
        let snapshot_root = focus_repro_snapshot();
        let state = set_preview_focused_window(
            initial_preview_state(),
            Some(WindowId::from("win-2")),
            Some(&snapshot_root),
        );

        assert_eq!(state.active_workspace_name, "1");
        assert_eq!(focused_window_id(&state), "win-2");
        assert_eq!(
            state.remembered_focus_by_scope.get("$workspace/visual[0]"),
            Some(&WindowId::from("win-2"))
        );
    }

    #[test]
    fn direct_workspace_selection_chooses_preferred_focus_on_target_workspace() {
        let mut state = initial_preview_state();
        state.workspace_names = vec!["1".to_string(), "2".to_string()];
        state.windows.push(PreviewWindow {
            id: "win-6".to_string(),
            app_id: Some("foot".to_string()),
            title: Some("Terminal 6".to_string()),
            class: Some("foot".to_string()),
            instance: Some("foot".to_string()),
            role: None,
            shell: Some("xdg_toplevel".to_string()),
            window_type: None,
            floating: false,
            fullscreen: false,
            focused: false,
            workspace_name: "2".to_string(),
        });

        let state = select_preview_workspace(state, "2", None);

        assert_eq!(state.active_workspace_name, "2");
        assert_eq!(focused_window_id(&state), "win-6");
    }

    #[test]
    fn preview_resize_updates_horizontal_split_weights_by_node_id() {
        let state = apply_preview_command(
            initial_preview_state(),
            resize_command(FocusDirection::Left),
            Some(&master_stack_snapshot()),
        );

        assert_eq!(
            state.layout_adjustments.split_weights_by_node_id.get("frame"),
            Some(&vec![11, 9])
        );
    }

    #[test]
    fn preview_resize_updates_vertical_split_weights_by_node_id() {
        let mut state = initial_preview_state();
        for window in &mut state.windows {
            window.focused = window.id == "win-5";
        }

        let state = apply_preview_command(
            state,
            resize_command(FocusDirection::Up),
            Some(&master_stack_snapshot()),
        );

        assert_eq!(
            state.layout_adjustments.split_weights_by_node_id.get("stack"),
            Some(&vec![8, 7, 9])
        );
    }

    #[test]
    fn preview_set_layout_updates_runtime_layout_state() {
        let state = apply_preview_command(
            initial_preview_state(),
            WmCommand::SetLayout {
                name: "focus-repro".to_string(),
            },
            None,
        );

        assert_eq!(state.active_layout, LayoutId::from("focus-repro"));
    }

    #[test]
    fn preview_cycle_layout_updates_runtime_layout_state() {
        let state = apply_preview_command(
            initial_preview_state(),
            WmCommand::CycleLayout { direction: None },
            None,
        );

        assert_eq!(state.active_layout, LayoutId::from("focus-repro"));
    }
}
