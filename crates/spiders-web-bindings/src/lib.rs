use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::{LayoutNodeMeta, LayoutRect, RemainingTake, ResolvedLayoutNode, SlotTake};
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

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum PreviewSnapshotNode {
    Workspace {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
        children: Vec<PreviewSnapshotNode>,
    },
    Group {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
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
            ..
        } => PreviewSnapshotNode::Workspace {
            meta,
            rect,
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Group {
            meta,
            rect,
            children,
            ..
        } => PreviewSnapshotNode::Group {
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

fn into_js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}