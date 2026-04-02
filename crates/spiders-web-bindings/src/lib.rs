use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use spiders_css::style::FlexDirectionValue;
use spiders_core::command::FocusDirection;
use spiders_core::focus::{
    FocusScopePath, FocusTree, FocusTreeWindowGeometry, remove_window, set_focused_window,
};
use spiders_core::navigation::{
    NavigationDirection, WindowGeometryCandidate, managed_window_swap_positions,
    select_directional_focus_candidate,
};
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::wm::{WindowGeometry, WmModel};
use spiders_core::workspace::{ensure_workspace, request_select_workspace};
use spiders_core::{
    LayoutNodeMeta, LayoutRect, OutputId, RemainingTake, ResolvedLayoutNode, SlotTake, WindowId,
    WorkspaceId,
};
use spiders_scene::LayoutSnapshotNode;
use spiders_scene::ast::{AuthoredLayoutNode, AuthoredNodeMeta, ValidatedLayoutTree};
use spiders_scene::pipeline::{LayoutPipelineError, compile_stylesheet, compute_layout_from_sheet};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize)]
struct PreviewDiagnostic {
    source: &'static str,
    level: &'static str,
    message: String,
    path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ResolvePreviewResult {
    root: Option<ResolvedLayoutNode>,
    diagnostics: Vec<PreviewDiagnostic>,
    unclaimed_windows: Vec<WindowSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
struct ComputePreviewResult {
    resolved_root: Option<ResolvedLayoutNode>,
    snapshot_root: Option<PreviewSnapshotNode>,
    diagnostics: Vec<PreviewDiagnostic>,
    unclaimed_windows: Vec<WindowSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSessionState {
    active_workspace_name: String,
    workspace_names: Vec<String>,
    windows: Vec<PreviewSessionWindow>,
    #[serde(default)]
    remembered_focus_by_scope: BTreeMap<String, WindowId>,
    #[serde(default)]
    master_ratio_by_workspace: BTreeMap<String, f32>,
    #[serde(default)]
    stack_weights_by_workspace: BTreeMap<String, BTreeMap<String, f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSessionWindow {
    id: String,
    #[serde(default, rename = "app_id", alias = "appId")]
    app_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    instance: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default, rename = "window_type", alias = "windowType")]
    window_type: Option<String>,
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    fullscreen: bool,
    #[serde(default)]
    focused: bool,
    workspace_name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PreviewCommand {
    name: String,
    #[serde(default)]
    arg: Option<PreviewCommandArg>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PreviewCommandArg {
    String(String),
    Number(i32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum JsPreviewSnapshotClasses {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsPreviewSnapshotNode {
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "class", alias = "className")]
    class_name: Option<JsPreviewSnapshotClasses>,
    #[serde(default)]
    rect: Option<LayoutRect>,
    #[serde(default, rename = "window_id", alias = "windowId")]
    window_id: Option<WindowId>,
    #[serde(default)]
    axis: Option<String>,
    #[serde(default)]
    reverse: bool,
    #[serde(default)]
    children: Vec<JsPreviewSnapshotNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum PreviewSnapshotNode {
    Workspace {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        axis: Option<&'static str>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reverse: bool,
        children: Vec<PreviewSnapshotNode>,
    },
    Group {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        axis: Option<&'static str>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reverse: bool,
        children: Vec<PreviewSnapshotNode>,
    },
    Window {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_id: Option<spiders_core::WindowId>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(untagged)]
enum JsLayoutValue {
    Node(JsLayoutNode),
    Array(Vec<JsLayoutValue>),
    Null(Option<()>),
    Bool(bool),
    String(String),
    Number(f64),
}

#[derive(Debug, Clone, Deserialize)]
struct JsLayoutNode {
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    props: JsLayoutProps,
    #[serde(default)]
    children: Vec<JsLayoutValue>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct JsLayoutProps {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    class: Option<String>,
    #[serde(default, rename = "match")]
    match_expr: Option<String>,
    #[serde(default)]
    take: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsLayoutWindow {
    id: String,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    instance: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    window_type: Option<String>,
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    fullscreen: bool,
    #[serde(default)]
    focused: bool,
}

#[wasm_bindgen]
pub fn validate_stylesheet(stylesheet_source: &str) -> Result<JsValue, JsValue> {
    let diagnostics = match compile_stylesheet(stylesheet_source) {
        Ok(_) => Vec::new(),
        Err(error) => vec![css_diagnostic(error)],
    };

    serde_wasm_bindgen::to_value(&diagnostics).map_err(into_js_error)
}

#[wasm_bindgen]
pub fn resolve_layout_preview(layout: JsValue, windows: JsValue) -> Result<JsValue, JsValue> {
    let authored_root = deserialize_authored_root(layout)?;
    let windows = deserialize_windows(windows)?;

    let result = match ValidatedLayoutTree::from_authored(authored_root) {
        Ok(validated) => match validated.resolve(&windows) {
            Ok(resolved) => {
                let root = resolved.root;
                let claimed_ids = collect_claimed_window_ids(&root);
                ResolvePreviewResult {
                    root: Some(root),
                    diagnostics: Vec::new(),
                    unclaimed_windows: windows
                        .into_iter()
                        .filter(|window| !claimed_ids.contains(window.id.as_str()))
                        .collect(),
                }
            }
            Err(error) => ResolvePreviewResult {
                root: None,
                diagnostics: vec![layout_diagnostic(error.to_string())],
                unclaimed_windows: windows,
            },
        },
        Err(error) => ResolvePreviewResult {
            root: None,
            diagnostics: vec![layout_diagnostic(error.to_string())],
            unclaimed_windows: windows,
        },
    };

    serde_wasm_bindgen::to_value(&result).map_err(into_js_error)
}

#[wasm_bindgen]
pub fn compute_layout_preview(
    layout: JsValue,
    windows: JsValue,
    stylesheet_source: &str,
    width: f32,
    height: f32,
) -> Result<JsValue, JsValue> {
    let authored_root = deserialize_authored_root(layout)?;
    let windows = deserialize_windows(windows)?;

    let result = match ValidatedLayoutTree::from_authored(authored_root) {
        Ok(validated) => match validated.resolve(&windows) {
            Ok(resolved) => {
                let root = resolved.root;
                let claimed_ids = collect_claimed_window_ids(&root);
                match compile_stylesheet(stylesheet_source)
                    .and_then(|sheet| compute_layout_from_sheet(&root, &sheet, width, height))
                {
                    Ok(laid_out) => ComputePreviewResult {
                        resolved_root: Some(root),
                        snapshot_root: Some(snapshot_node(laid_out.snapshot())),
                        diagnostics: Vec::new(),
                        unclaimed_windows: windows
                            .into_iter()
                            .filter(|window| !claimed_ids.contains(window.id.as_str()))
                            .collect(),
                    },
                    Err(error) => ComputePreviewResult {
                        resolved_root: Some(root),
                        snapshot_root: None,
                        diagnostics: vec![css_diagnostic(error)],
                        unclaimed_windows: windows
                            .into_iter()
                            .filter(|window| !claimed_ids.contains(window.id.as_str()))
                            .collect(),
                    },
                }
            }
            Err(error) => ComputePreviewResult {
                resolved_root: None,
                snapshot_root: None,
                diagnostics: vec![layout_diagnostic(error.to_string())],
                unclaimed_windows: windows,
            },
        },
        Err(error) => ComputePreviewResult {
            resolved_root: None,
            snapshot_root: None,
            diagnostics: vec![layout_diagnostic(error.to_string())],
            unclaimed_windows: windows,
        },
    };

    serde_wasm_bindgen::to_value(&result).map_err(into_js_error)
}

#[wasm_bindgen]
pub fn apply_preview_command(
    state: JsValue,
    command: JsValue,
    snapshot_root: JsValue,
) -> Result<JsValue, JsValue> {
    let state: PreviewSessionState = serde_wasm_bindgen::from_value(state).map_err(into_js_error)?;
    let command: PreviewCommand = serde_wasm_bindgen::from_value(command).map_err(into_js_error)?;
    let snapshot_root: Option<JsPreviewSnapshotNode> = if snapshot_root.is_null() || snapshot_root.is_undefined() {
        None
    } else {
        Some(serde_wasm_bindgen::from_value(snapshot_root).map_err(into_js_error)?)
    };

    let next_state = apply_preview_command_inner(state, command, snapshot_root);

    serde_wasm_bindgen::to_value(&next_state).map_err(into_js_error)
}

#[wasm_bindgen]
pub fn apply_preview_snapshot_overrides(
    state: JsValue,
    snapshot_root: JsValue,
) -> Result<JsValue, JsValue> {
    let state: PreviewSessionState = serde_wasm_bindgen::from_value(state).map_err(into_js_error)?;
    let mut snapshot_root: JsPreviewSnapshotNode =
        serde_wasm_bindgen::from_value(snapshot_root).map_err(into_js_error)?;

    apply_snapshot_overrides(&state, &mut snapshot_root);

    serde_wasm_bindgen::to_value(&snapshot_root).map_err(into_js_error)
}

fn deserialize_authored_root(value: JsValue) -> Result<AuthoredLayoutNode, JsValue> {
    let renderable: JsLayoutValue = serde_wasm_bindgen::from_value(value).map_err(into_js_error)?;
    let mut nodes = Vec::new();
    collect_root_nodes(renderable, &mut nodes)?;

    match nodes.len() {
        0 => Err(JsValue::from_str("layout must return a workspace root node")),
        1 => nodes
            .into_iter()
            .next()
            .ok_or_else(|| JsValue::from_str("layout must return a workspace root node")),
        _ => Err(JsValue::from_str("layout must return exactly one root node")),
    }
}

fn apply_preview_command_inner(
    mut state: PreviewSessionState,
    command: PreviewCommand,
    snapshot_root: Option<JsPreviewSnapshotNode>,
) -> PreviewSessionState {
    normalize_preview_state(&mut state);

    let mut model = preview_model(&state);
    let current_focused_window_id = model.focused_window_id().cloned();
    if let Some(snapshot_root) = snapshot_root.as_ref() {
        model.set_focus_tree_value(Some(focus_tree_from_preview_snapshot(snapshot_root)));
    }
    if let Some(focused_window_id) = current_focused_window_id {
        let _ = set_focused_window(&mut model, Some(focused_window_id));
    }

    match command.name.as_str() {
        "view_workspace" => {
            if let Some(target_name) = command_workspace_name(&state.workspace_names, command.arg.as_ref()) {
                let ordered_window_ids = ordered_window_ids(&state);

                if let Some(selection) = request_select_workspace(
                    &mut model,
                    WorkspaceId(target_name),
                    ordered_window_ids,
                ) {
                    let _ = set_focused_window(&mut model, selection.focused_window_id);
                }
            }
        }
        "assign_workspace" => {
            if let Some(target_name) = command_workspace_name(&state.workspace_names, command.arg.as_ref()) {
                assign_focused_window_to_workspace(
                    &mut model,
                    WorkspaceId(target_name),
                    ordered_window_ids(&state),
                );
            }
        }
        "focus_dir" => {
            if let Some(direction) = command_direction(command.arg.as_ref()) {
                focus_direction(&mut model, snapshot_root.as_ref(), direction);
            }
        }
        "swap_dir" => {
            if let Some(direction) = command_direction(command.arg.as_ref()) {
                swap_direction(&mut state, &mut model, snapshot_root.as_ref(), direction);
            }
        }
        "toggle_floating" => {
            toggle_focused_window_floating(&mut model);
        }
        "kill_client" => {
            close_focused_window(&mut state, &mut model);
        }
        "spawn" => {
            if matches!(command.arg.as_ref(), Some(PreviewCommandArg::String(value)) if value == "foot") {
                spawn_foot_window(&mut state, &mut model);
            }
        }
        "resize_dir" | "resize_tiled" => {
            if let Some(direction) = command_direction(command.arg.as_ref()) {
                resize_direction(&mut state, &model, snapshot_root.as_ref(), direction);
            }
        }
        "cycle_layout" => {}
        _ => {}
    }

    sync_preview_state(&mut state, &model);
    state
}

fn apply_snapshot_overrides(state: &PreviewSessionState, root: &mut JsPreviewSnapshotNode) {
    if !snapshot_uses_master_stack_overrides(root) {
        return;
    }

    let Some(root_rect) = root.rect else {
        return;
    };

    let mut windows = snapshot_windows(root);

    if windows.len() < 2 {
        return;
    }

    windows.sort_by(|left, right| {
        left.rect
            .x
            .partial_cmp(&right.rect.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.rect
                    .y
                    .partial_cmp(&right.rect.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let Some(master) = windows.first().cloned() else {
        return;
    };
    let stack = windows.iter().skip(1).cloned().collect::<Vec<_>>();

    if stack.is_empty() {
        return;
    }

    let workspace_name = state.active_workspace_name.as_str();
    let master_ratio = state
        .master_ratio_by_workspace
        .get(workspace_name)
        .copied()
        .unwrap_or(0.6)
        .clamp(0.2, 0.8);

    let left_padding = master.rect.x - root_rect.x;
    let stack_left = stack
        .iter()
        .map(|window| window.rect.x)
        .fold(f32::INFINITY, f32::min);
    let stack_right = stack
        .iter()
        .map(|window| window.rect.x + window.rect.width)
        .fold(f32::NEG_INFINITY, f32::max);
    let right_padding = (root_rect.x + root_rect.width) - stack_right;
    let gap = (stack_left - (master.rect.x + master.rect.width)).max(0.0);
    let content_width = (root_rect.width - left_padding - right_padding - gap).max(0.0);
    let master_width = content_width * master_ratio;
    let stack_width = (content_width - master_width).max(0.0);
    let master_rect = LayoutRect {
        x: root_rect.x + left_padding,
        y: master.rect.y,
        width: master_width,
        height: master.rect.height,
    };
    let stack_x = master_rect.x + master_rect.width + gap;

    let mut stack_sorted = stack;
    stack_sorted.sort_by(|left, right| {
        left.rect
            .y
            .partial_cmp(&right.rect.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_padding = stack_sorted
        .first()
        .map(|window| window.rect.y - root_rect.y)
        .unwrap_or(0.0);
    let bottom_padding = stack_sorted
        .last()
        .map(|window| (root_rect.y + root_rect.height) - (window.rect.y + window.rect.height))
        .unwrap_or(0.0);
    let gaps = stack_sorted
        .windows(2)
        .map(|pair| (pair[1].rect.y - (pair[0].rect.y + pair[0].rect.height)).max(0.0))
        .collect::<Vec<_>>();
    let gap_total = gaps.iter().sum::<f32>();
    let available_height = (root_rect.height - top_padding - bottom_padding - gap_total).max(0.0);
    let stack_weights = state.stack_weights_by_workspace.get(workspace_name);
    let mut resolved_weights = stack_sorted
        .iter()
        .map(|window| {
            stack_weights
                .and_then(|weights| weights.get(window.window_id.as_str()).copied())
                .unwrap_or(1.0)
                .max(0.1)
        })
        .collect::<Vec<_>>();
    let weight_total = resolved_weights.iter().sum::<f32>().max(0.1);

    for weight in &mut resolved_weights {
        *weight /= weight_total;
    }

    set_window_rect(root, &master.window_id, master_rect);

    let mut cursor_y = root_rect.y + top_padding;

    for (index, window) in stack_sorted.iter().enumerate() {
        let height = if index + 1 == stack_sorted.len() {
            (root_rect.y + root_rect.height - bottom_padding - cursor_y).max(0.0)
        } else {
            available_height * resolved_weights[index]
        };

        set_window_rect(
            root,
            &window.window_id,
            LayoutRect {
                x: stack_x,
                y: cursor_y,
                width: stack_width,
                height,
            },
        );

        cursor_y += height + gaps.get(index).copied().unwrap_or(0.0);
    }

    recompute_group_rects(root, true);
}

fn normalize_preview_state(state: &mut PreviewSessionState) {
    if state.workspace_names.is_empty() {
        state.workspace_names.push(if state.active_workspace_name.is_empty() {
            "1:dev".to_string()
        } else {
            state.active_workspace_name.clone()
        });
    }

    if !state.workspace_names.contains(&state.active_workspace_name) {
        state.active_workspace_name = state
            .workspace_names
            .first()
            .cloned()
            .unwrap_or_else(|| "1:dev".to_string());
    }
}

fn preview_model(state: &PreviewSessionState) -> WmModel {
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

    model.last_focused_window_id_by_scope = remembered_focus_from_serialized(&state.remembered_focus_by_scope);

    let focused_window_id = state
        .windows
        .iter()
        .find(|window| window.focused)
        .map(|window| WindowId::from(window.id.as_str()));
    let _ = set_focused_window(&mut model, focused_window_id);

    model
}

fn ordered_window_ids(state: &PreviewSessionState) -> Vec<WindowId> {
    state
        .windows
        .iter()
        .map(|window| WindowId::from(window.id.as_str()))
        .collect()
}

fn command_workspace_name(workspace_names: &[String], arg: Option<&PreviewCommandArg>) -> Option<String> {
    let PreviewCommandArg::Number(workspace_index) = arg? else {
        return None;
    };

    if *workspace_index <= 0 {
        return None;
    }

    workspace_names.get((*workspace_index as usize) - 1).cloned()
}

fn command_direction(arg: Option<&PreviewCommandArg>) -> Option<FocusDirection> {
    let PreviewCommandArg::String(direction) = arg? else {
        return None;
    };

    match direction.as_str() {
        "left" => Some(FocusDirection::Left),
        "right" => Some(FocusDirection::Right),
        "up" => Some(FocusDirection::Up),
        "down" => Some(FocusDirection::Down),
        _ => None,
    }
}

fn focus_direction(
    model: &mut WmModel,
    snapshot_root: Option<&JsPreviewSnapshotNode>,
    direction: FocusDirection,
) {
    let Some(target_window_id) = directional_target_window(model, snapshot_root, direction) else {
        return;
    };

    let _ = set_focused_window(model, Some(target_window_id));
}

fn resize_direction(
    state: &mut PreviewSessionState,
    model: &WmModel,
    snapshot_root: Option<&JsPreviewSnapshotNode>,
    direction: FocusDirection,
) {
    let Some(snapshot_root) = snapshot_root else {
        return;
    };
    if !snapshot_uses_master_stack_overrides(snapshot_root) {
        return;
    }
    let Some(focused_window_id) = model.focused_window_id().cloned() else {
        return;
    };

    let mut windows = snapshot_windows(snapshot_root);

    if windows.len() < 2 {
        return;
    }

    windows.sort_by(|left, right| {
        left.rect
            .x
            .partial_cmp(&right.rect.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.rect
                    .y
                    .partial_cmp(&right.rect.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let workspace_name = state.active_workspace_name.clone();
    let Some(master_window) = windows.first() else {
        return;
    };
    let focused_is_master = master_window.window_id == focused_window_id;
    let delta = 0.04_f32;

    match direction {
        FocusDirection::Left | FocusDirection::Right => {
            let ratio = state
                .master_ratio_by_workspace
                .entry(workspace_name)
                .or_insert(0.6);
            let signed_delta = match (focused_is_master, direction) {
                (true, FocusDirection::Left) => -delta,
                (true, FocusDirection::Right) => delta,
                (false, FocusDirection::Left) => delta,
                (false, FocusDirection::Right) => -delta,
                _ => 0.0,
            };
            *ratio = (*ratio + signed_delta).clamp(0.2, 0.8);
        }
        FocusDirection::Up | FocusDirection::Down => {
            let stack_windows = windows.iter().skip(1).cloned().collect::<Vec<_>>();
            let Some(current_index) = stack_windows
                .iter()
                .position(|window| window.window_id == focused_window_id)
            else {
                return;
            };
            let neighbor_index = match direction {
                FocusDirection::Up if current_index > 0 => Some(current_index - 1),
                FocusDirection::Down if current_index + 1 < stack_windows.len() => Some(current_index + 1),
                _ => None,
            };
            let Some(neighbor_index) = neighbor_index else {
                return;
            };

            let weights = state
                .stack_weights_by_workspace
                .entry(workspace_name)
                .or_default();
            let current_id = stack_windows[current_index].window_id.as_str().to_string();
            let neighbor_id = stack_windows[neighbor_index].window_id.as_str().to_string();
            let current_weight = weights.get(&current_id).copied().unwrap_or(1.0);
            let neighbor_weight = weights.get(&neighbor_id).copied().unwrap_or(1.0);
            let grow = 0.12_f32;
            let shrink = 0.12_f32.min((neighbor_weight - 0.2).max(0.0));

            if shrink <= 0.0 {
                return;
            }

            weights.insert(current_id, current_weight + grow);
            weights.insert(neighbor_id, (neighbor_weight - shrink).max(0.2));
        }
    }
}

fn swap_direction(
    state: &mut PreviewSessionState,
    model: &mut WmModel,
    snapshot_root: Option<&JsPreviewSnapshotNode>,
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

    let (Some(first_global_index), Some(second_global_index)) = (first_global_index, second_global_index) else {
        return;
    };

    state.windows.swap(first_global_index, second_global_index);
    let _ = set_focused_window(model, Some(focused_window_id));
}

fn directional_candidate_ids(
    state: &PreviewSessionState,
    snapshot_root: Option<&JsPreviewSnapshotNode>,
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
    snapshot_root: Option<&JsPreviewSnapshotNode>,
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

fn focus_tree_from_preview_snapshot(root: &JsPreviewSnapshotNode) -> FocusTree {
    let mut windows = Vec::new();
    collect_preview_focus_tree_window_geometries(root, &mut windows);
    FocusTree::from_window_geometries(&windows)
}

fn collect_preview_focus_tree_window_geometries(
    node: &JsPreviewSnapshotNode,
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
    root: &JsPreviewSnapshotNode,
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

fn collect_snapshot_window_ids(node: &JsPreviewSnapshotNode, out: &mut Vec<WindowId>) {
    if node.node_type == "window" && let Some(window_id) = node.window_id.as_ref() {
        out.push(window_id.clone());
    }

    for child in &node.children {
        collect_snapshot_window_ids(child, out);
    }
}

#[derive(Clone)]
struct SnapshotWindowRect {
    window_id: WindowId,
    rect: LayoutRect,
}

fn snapshot_windows(node: &JsPreviewSnapshotNode) -> Vec<SnapshotWindowRect> {
    let mut windows = Vec::new();
    collect_snapshot_windows(node, &mut windows);
    windows
}

fn snapshot_uses_master_stack_overrides(root: &JsPreviewSnapshotNode) -> bool {
    matches!(
        root.children.as_slice(),
        [master, stack]
            if master.node_type == "window"
                && master.id.as_deref() == Some("master")
                && stack.node_type == "group"
                && stack.id.as_deref() == Some("stack")
    )
}

fn collect_snapshot_windows(node: &JsPreviewSnapshotNode, out: &mut Vec<SnapshotWindowRect>) {
    if node.node_type == "window"
        && let (Some(window_id), Some(rect)) = (node.window_id.as_ref(), node.rect)
    {
        out.push(SnapshotWindowRect {
            window_id: window_id.clone(),
            rect,
        });
    }

    for child in &node.children {
        collect_snapshot_windows(child, out);
    }
}

fn set_window_rect(node: &mut JsPreviewSnapshotNode, target_id: &WindowId, rect: LayoutRect) -> bool {
    if node.node_type == "window" && node.window_id.as_ref() == Some(target_id) {
        node.rect = Some(rect);
        return true;
    }

    for child in &mut node.children {
        if set_window_rect(child, target_id, rect) {
            return true;
        }
    }

    false
}

fn recompute_group_rects(node: &mut JsPreviewSnapshotNode, is_root: bool) -> Option<LayoutRect> {
    if node.node_type == "window" {
        return node.rect;
    }

    let child_rects = node
        .children
        .iter_mut()
        .filter_map(|child| recompute_group_rects(child, false))
        .collect::<Vec<_>>();

    if !is_root {
        node.rect = bounding_rect(&child_rects);
    }

    node.rect
}

fn bounding_rect(rects: &[LayoutRect]) -> Option<LayoutRect> {
    let first = *rects.first()?;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x + first.width;
    let mut bottom = first.y + first.height;

    for rect in rects.iter().skip(1) {
        left = left.min(rect.x);
        top = top.min(rect.y);
        right = right.max(rect.x + rect.width);
        bottom = bottom.max(rect.y + rect.height);
    }

    Some(LayoutRect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    })
}

fn navigation_direction(direction: FocusDirection) -> NavigationDirection {
    match direction {
        FocusDirection::Left => NavigationDirection::Left,
        FocusDirection::Right => NavigationDirection::Right,
        FocusDirection::Up => NavigationDirection::Up,
        FocusDirection::Down => NavigationDirection::Down,
    }
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

fn close_focused_window(state: &mut PreviewSessionState, model: &mut WmModel) {
    let Some(focused_window_id) = model.focused_window_id().cloned() else {
        return;
    };

    let ordered_window_ids = ordered_window_ids(state);
    let _ = remove_window(model, focused_window_id.clone(), ordered_window_ids);
    state
        .windows
        .retain(|window| window.id != focused_window_id.as_str());
}

fn spawn_foot_window(state: &mut PreviewSessionState, model: &mut WmModel) {
    let terminal_number = next_terminal_title_number(state);
    let window_id = format!("win-{}", next_window_id_number(state));
    let current_workspace = model
        .current_workspace_id()
        .cloned()
        .unwrap_or_else(|| WorkspaceId::from(state.active_workspace_name.as_str()));

    state.windows.push(PreviewSessionWindow {
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

fn next_terminal_title_number(state: &PreviewSessionState) -> u32 {
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

fn next_window_id_number(state: &PreviewSessionState) -> u32 {
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

fn sync_preview_state(state: &mut PreviewSessionState, model: &WmModel) {
    if let Some(current_workspace_id) = model.current_workspace_id() {
        state.active_workspace_name = current_workspace_id.as_str().to_string();
    }

    state.remembered_focus_by_scope = remembered_focus_to_serialized(&model.last_focused_window_id_by_scope);

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

fn deserialize_windows(value: JsValue) -> Result<Vec<WindowSnapshot>, JsValue> {
    let windows: Vec<JsLayoutWindow> = serde_wasm_bindgen::from_value(value).map_err(into_js_error)?;
    Ok(windows.into_iter().map(window_snapshot).collect())
}

fn collect_root_nodes(
    value: JsLayoutValue,
    out: &mut Vec<AuthoredLayoutNode>,
) -> Result<(), JsValue> {
    match value {
        JsLayoutValue::Node(node) => out.push(authored_node(node)?),
        JsLayoutValue::Array(values) => {
            for value in values {
                collect_root_nodes(value, out)?;
            }
        }
        JsLayoutValue::Null(_) | JsLayoutValue::Bool(_) => {}
        JsLayoutValue::String(_) | JsLayoutValue::Number(_) => {
            return Err(JsValue::from_str(
                "layout renderables must be workspace/group/window/slot nodes",
            ));
        }
    }

    Ok(())
}

fn authored_node(node: JsLayoutNode) -> Result<AuthoredLayoutNode, JsValue> {
    let meta = AuthoredNodeMeta {
        id: node.props.id,
        class: split_classes(node.props.class),
        name: None,
        data: Default::default(),
    };

    match node.node_type.as_str() {
        "workspace" => Ok(AuthoredLayoutNode::Workspace {
            meta,
            children: authored_children(node.children)?,
        }),
        "group" => Ok(AuthoredLayoutNode::Group {
            meta,
            children: authored_children(node.children)?,
        }),
        "window" => Ok(AuthoredLayoutNode::Window {
            meta,
            match_expr: node.props.match_expr,
        }),
        "slot" => Ok(AuthoredLayoutNode::Slot {
            meta,
            match_expr: node.props.match_expr,
            take: node
                .props
                .take
                .map(SlotTake::Count)
                .unwrap_or(SlotTake::Remaining(RemainingTake::Remaining)),
        }),
        other => Err(JsValue::from_str(&format!("unsupported layout node type: {other}"))),
    }
}

fn authored_children(children: Vec<JsLayoutValue>) -> Result<Vec<AuthoredLayoutNode>, JsValue> {
    let mut nodes = Vec::new();

    for child in children {
        collect_root_nodes(child, &mut nodes)?;
    }

    Ok(nodes)
}

fn split_classes(class_name: Option<String>) -> Vec<String> {
    class_name
        .into_iter()
        .flat_map(|value| {
            value
                .split_whitespace()
                .filter(|segment| !segment.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn window_snapshot(window: JsLayoutWindow) -> WindowSnapshot {
    let mode = if window.fullscreen {
        WindowMode::Fullscreen
    } else if window.floating {
        WindowMode::Floating { rect: None }
    } else {
        WindowMode::Tiled
    };

    WindowSnapshot {
        id: window.id.into(),
        shell: match window.shell.as_deref() {
            Some("x11") => ShellKind::X11,
            Some("xdg-toplevel") | Some("xdg_toplevel") => ShellKind::XdgToplevel,
            _ => ShellKind::Unknown,
        },
        app_id: window.app_id,
        title: window.title,
        class: window.class,
        instance: window.instance,
        role: window.role,
        window_type: window.window_type,
        mapped: true,
        mode,
        focused: window.focused,
        urgent: false,
        closing: false,
        output_id: None,
        workspace_id: None,
        workspaces: Vec::new(),
    }
}

fn collect_claimed_window_ids(root: &ResolvedLayoutNode) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    collect_claimed_window_ids_inner(root, &mut ids);
    ids
}

fn collect_claimed_window_ids_inner(root: &ResolvedLayoutNode, out: &mut BTreeSet<String>) {
    match root {
        ResolvedLayoutNode::Workspace { children, .. } | ResolvedLayoutNode::Group { children, .. } => {
            for child in children {
                collect_claimed_window_ids_inner(child, out);
            }
        }
        ResolvedLayoutNode::Window { window_id, .. } => {
            if let Some(window_id) = window_id.as_ref() {
                out.insert(window_id.as_str().to_owned());
            }
        }
    }
}

fn snapshot_node(node: LayoutSnapshotNode) -> PreviewSnapshotNode {
    match node {
        LayoutSnapshotNode::Workspace {
            meta,
            rect,
            children,
            styles,
        } => PreviewSnapshotNode::Workspace {
            axis: layout_axis(styles.as_ref()),
            reverse: layout_reverse(styles.as_ref()),
            meta,
            rect,
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Group {
            meta,
            rect,
            children,
            styles,
        } => PreviewSnapshotNode::Group {
            axis: layout_axis(styles.as_ref()),
            reverse: layout_reverse(styles.as_ref()),
            meta,
            rect,
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Window {
            meta,
            rect,
            window_id,
            ..
        } => PreviewSnapshotNode::Window {
            meta,
            rect,
            window_id,
        },
    }
}

fn layout_axis(styles: Option<&spiders_scene::SceneNodeStyle>) -> Option<&'static str> {
    match styles?.layout.flex_direction? {
        FlexDirectionValue::Row | FlexDirectionValue::RowReverse => Some("horizontal"),
        FlexDirectionValue::Column | FlexDirectionValue::ColumnReverse => Some("vertical"),
    }
}

fn layout_reverse(styles: Option<&spiders_scene::SceneNodeStyle>) -> bool {
    matches!(
        styles.and_then(|styles| styles.layout.flex_direction),
        Some(FlexDirectionValue::RowReverse | FlexDirectionValue::ColumnReverse)
    )
}

fn layout_diagnostic(message: String) -> PreviewDiagnostic {
    PreviewDiagnostic {
        source: "layout",
        level: "error",
        message,
        path: None,
    }
}

fn css_diagnostic(error: LayoutPipelineError) -> PreviewDiagnostic {
    PreviewDiagnostic {
        source: "css",
        level: "error",
        message: error.to_string(),
        path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focus_command(direction: &str) -> PreviewCommand {
        PreviewCommand {
            name: "focus_dir".to_string(),
            arg: Some(PreviewCommandArg::String(direction.to_string())),
        }
    }

    fn preview_window(id: &str, title: &str, focused: bool) -> PreviewSessionWindow {
        PreviewSessionWindow {
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

    fn window_node(window_id: &str, x: f32, y: f32, width: f32, height: f32) -> JsPreviewSnapshotNode {
        JsPreviewSnapshotNode {
            node_type: "window".to_string(),
            id: None,
            class_name: None,
            rect: Some(LayoutRect {
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

    fn focus_repro_snapshot() -> JsPreviewSnapshotNode {
        JsPreviewSnapshotNode {
            node_type: "workspace".to_string(),
            id: Some("frame".to_string()),
            class_name: None,
            rect: Some(LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 3440.0,
                height: 1440.0,
            }),
            window_id: None,
            axis: Some("horizontal".to_string()),
            reverse: false,
            children: vec![
                JsPreviewSnapshotNode {
                    node_type: "group".to_string(),
                    id: Some("main-column".to_string()),
                    class_name: None,
                    rect: Some(LayoutRect {
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
                JsPreviewSnapshotNode {
                    node_type: "group".to_string(),
                    id: Some("side-column".to_string()),
                    class_name: None,
                    rect: Some(LayoutRect {
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

    fn initial_preview_state() -> PreviewSessionState {
        PreviewSessionState {
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
            master_ratio_by_workspace: BTreeMap::new(),
            stack_weights_by_workspace: BTreeMap::new(),
        }
    }

    fn abcd_preview_state() -> PreviewSessionState {
        PreviewSessionState {
            active_workspace_name: "1".to_string(),
            workspace_names: vec!["1".to_string()],
            windows: vec![
                preview_window("win-1", "Terminal 1", false),
                preview_window("win-2", "Spec Draft", false),
                preview_window("win-3", "Engineering", false),
                preview_window("win-4", "Terminal 4", true),
            ],
            remembered_focus_by_scope: BTreeMap::new(),
            master_ratio_by_workspace: BTreeMap::new(),
            stack_weights_by_workspace: BTreeMap::new(),
        }
    }

    fn focused_window_id(state: &PreviewSessionState) -> &str {
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

        let state = apply_preview_command_inner(
            initial_preview_state(),
            focus_command("right"),
            Some(snapshot_root.clone()),
        );
        assert_eq!(focused_window_id(&state), "win-1");
        assert_eq!(
            state.remembered_focus_by_scope.get(side_scope),
            Some(&WindowId::from("win-5"))
        );

        let state = apply_preview_command_inner(state, focus_command("down"), Some(snapshot_root.clone()));
        assert_eq!(focused_window_id(&state), "win-2");
        assert_eq!(
            state.remembered_focus_by_scope.get(side_scope),
            Some(&WindowId::from("win-5"))
        );
        assert_eq!(
            state.remembered_focus_by_scope.get(main_scope),
            Some(&WindowId::from("win-2"))
        );

        let state = apply_preview_command_inner(state, focus_command("down"), Some(snapshot_root.clone()));
        assert_eq!(focused_window_id(&state), "win-1");
        assert_eq!(
            state.remembered_focus_by_scope.get(side_scope),
            Some(&WindowId::from("win-5"))
        );

        let state = apply_preview_command_inner(state, focus_command("right"), Some(snapshot_root));
        assert_eq!(focused_window_id(&state), "win-5");
        assert_eq!(
            state.remembered_focus_by_scope.get(main_scope),
            Some(&WindowId::from("win-1"))
        );
    }

    #[test]
    fn preview_focus_commands_follow_visual_abcd_neighbors() {
        let snapshot_root = JsPreviewSnapshotNode {
            node_type: "workspace".to_string(),
            id: Some("frame".to_string()),
            class_name: None,
            rect: Some(LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 3440.0,
                height: 1440.0,
            }),
            window_id: None,
            axis: Some("horizontal".to_string()),
            reverse: false,
            children: vec![
                window_node("win-1", 0.0, 0.0, 2052.0, 1416.0),
                JsPreviewSnapshotNode {
                    node_type: "group".to_string(),
                    id: Some("right-column".to_string()),
                    class_name: None,
                    rect: Some(LayoutRect {
                        x: 2052.0,
                        y: 0.0,
                        width: 1352.0,
                        height: 1416.0,
                    }),
                    window_id: None,
                    axis: Some("vertical".to_string()),
                    reverse: false,
                    children: vec![
                        window_node("win-2", 2052.0, 0.0, 1352.0, 714.0),
                        JsPreviewSnapshotNode {
                            node_type: "group".to_string(),
                            id: Some("bottom-row".to_string()),
                            class_name: None,
                            rect: Some(LayoutRect {
                                x: 2052.0,
                                y: 714.0,
                                width: 1352.0,
                                height: 690.0,
                            }),
                            window_id: None,
                            axis: Some("horizontal".to_string()),
                            reverse: false,
                            children: vec![
                                window_node("win-3", 2052.0, 714.0, 670.0, 690.0),
                                window_node("win-4", 2734.0, 714.0, 670.0, 690.0),
                            ],
                        },
                    ],
                },
            ],
        };

        let state = apply_preview_command_inner(
            abcd_preview_state(),
            focus_command("left"),
            Some(snapshot_root.clone()),
        );
        assert_eq!(focused_window_id(&state), "win-3");

        let state = apply_preview_command_inner(state, focus_command("left"), Some(snapshot_root.clone()));
        assert_eq!(focused_window_id(&state), "win-1");

        let state = apply_preview_command_inner(
            abcd_preview_state(),
            focus_command("right"),
            Some(snapshot_root.clone()),
        );
        assert_eq!(focused_window_id(&state), "win-1");
    }
}

fn into_js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}