use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::{RemainingTake, ResolvedLayoutNode, SlotTake};
use spiders_css::style::FlexDirectionValue;
use spiders_scene::LayoutSnapshotNode;
use spiders_scene::ast::{AuthoredLayoutNode, AuthoredNodeMeta, ValidatedLayoutTree};
use spiders_scene::pipeline::{LayoutPipelineError, compile_stylesheet, compute_layout_from_sheet};
use wasm_bindgen::JsValue;

use crate::{PreviewDiagnostic, PreviewSnapshotClasses, PreviewSnapshotNode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewLayoutWindow {
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLayoutComputation {
    pub snapshot_root: Option<PreviewSnapshotNode>,
    #[serde(default)]
    pub diagnostics: Vec<PreviewDiagnostic>,
    #[serde(default)]
    pub unclaimed_window_ids: Vec<String>,
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

pub fn compute_layout_preview(
    layout: JsValue,
    windows: &[PreviewLayoutWindow],
    stylesheet_source: &str,
    width: f32,
    height: f32,
) -> PreviewLayoutComputation {
    let authored_root = match deserialize_authored_root(layout) {
        Ok(root) => root,
        Err(error) => {
            return PreviewLayoutComputation {
                snapshot_root: None,
                diagnostics: vec![layout_diagnostic(js_error_to_string(error))],
                unclaimed_window_ids: windows.iter().map(|window| window.id.clone()).collect(),
            };
        }
    };

    let window_snapshots = windows.iter().cloned().map(window_snapshot).collect::<Vec<_>>();

    match ValidatedLayoutTree::from_authored(authored_root) {
        Ok(validated) => match validated.resolve(&window_snapshots) {
            Ok(resolved) => {
                let root = resolved.root;
                let claimed_ids = collect_claimed_window_ids(&root);
                match compile_stylesheet(stylesheet_source)
                    .and_then(|sheet| compute_layout_from_sheet(&root, &sheet, width, height))
                {
                    Ok(laid_out) => PreviewLayoutComputation {
                        snapshot_root: Some(snapshot_node(laid_out.snapshot())),
                        diagnostics: Vec::new(),
                        unclaimed_window_ids: unclaimed_window_ids(windows, &claimed_ids),
                    },
                    Err(error) => PreviewLayoutComputation {
                        snapshot_root: None,
                        diagnostics: vec![css_diagnostic(error)],
                        unclaimed_window_ids: unclaimed_window_ids(windows, &claimed_ids),
                    },
                }
            }
            Err(error) => PreviewLayoutComputation {
                snapshot_root: None,
                diagnostics: vec![layout_diagnostic(error.to_string())],
                unclaimed_window_ids: windows.iter().map(|window| window.id.clone()).collect(),
            },
        },
        Err(error) => PreviewLayoutComputation {
            snapshot_root: None,
            diagnostics: vec![layout_diagnostic(error.to_string())],
            unclaimed_window_ids: windows.iter().map(|window| window.id.clone()).collect(),
        },
    }
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
        "workspace" => {
            Ok(AuthoredLayoutNode::Workspace { meta, children: authored_children(node.children)? })
        }
        "group" => {
            Ok(AuthoredLayoutNode::Group { meta, children: authored_children(node.children)? })
        }
        "window" => Ok(AuthoredLayoutNode::Window { meta, match_expr: node.props.match_expr }),
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

fn window_snapshot(window: PreviewLayoutWindow) -> WindowSnapshot {
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
        ResolvedLayoutNode::Workspace { children, .. }
        | ResolvedLayoutNode::Group { children, .. } => {
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
        LayoutSnapshotNode::Workspace { meta, rect, children, styles } => PreviewSnapshotNode {
            node_type: "workspace".to_string(),
            id: meta.id,
            class_name: snapshot_classes(meta.class),
            rect: Some(rect),
            window_id: None,
            axis: layout_axis(styles.as_ref()).map(str::to_string),
            reverse: layout_reverse(styles.as_ref()),
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Group { meta, rect, children, styles } => PreviewSnapshotNode {
            node_type: "group".to_string(),
            id: meta.id,
            class_name: snapshot_classes(meta.class),
            rect: Some(rect),
            window_id: None,
            axis: layout_axis(styles.as_ref()).map(str::to_string),
            reverse: layout_reverse(styles.as_ref()),
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Window { meta, rect, window_id, .. } => PreviewSnapshotNode {
            node_type: "window".to_string(),
            id: meta.id,
            class_name: snapshot_classes(meta.class),
            rect: Some(rect),
            window_id,
            axis: None,
            reverse: false,
            children: Vec::new(),
        },
    }
}

fn snapshot_classes(classes: Vec<String>) -> Option<PreviewSnapshotClasses> {
    match classes.as_slice() {
        [] => None,
        [single] => Some(PreviewSnapshotClasses::One(single.clone())),
        _ => Some(PreviewSnapshotClasses::Many(classes)),
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

fn unclaimed_window_ids(
    windows: &[PreviewLayoutWindow],
    claimed_ids: &BTreeSet<String>,
) -> Vec<String> {
    windows
        .iter()
        .filter(|window| !claimed_ids.contains(window.id.as_str()))
        .map(|window| window.id.clone())
        .collect()
}

fn layout_diagnostic(message: String) -> PreviewDiagnostic {
    PreviewDiagnostic { source: "layout".to_string(), level: "error".to_string(), message }
}

fn css_diagnostic(error: LayoutPipelineError) -> PreviewDiagnostic {
    PreviewDiagnostic {
        source: "css".to_string(),
        level: "error".to_string(),
        message: error.to_string(),
    }
}

fn into_js_error(error: serde_wasm_bindgen::Error) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn js_error_to_string(error: JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}
